// Editing vector paths in the "Edit content" mode (B26 T-263): a selected
// shape or rule moves by the arrow keys and by a drag of its body, resizes
// from its handles (eight on a shape, a corner free; two at the ends of a
// rule, which stays flat), and is deleted by Delete or its toolbar. Each
// edit is one undo step, marks the document dirty, and is in the saved file,
// read back by opening it again — on an upright page and on one turned 90°.

import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { buildPdf, type PageSpec } from './pdf';
import { loadApp, openBytes, pageSlot, toolsButton } from './support';

/**
 * A blue box 100 x 50 pt at left 300, bottom 550 in PDF space (drawn at
 * (300, 192)), and a blue rule from (72, 500) to (272, 500) (drawn at
 * (72, 292), 200 long, no height): both well inside the viewport.
 */
const SHAPES: PageSpec = { widthPt: 612, heightPt: 792, paths: [{ box: [300, 550, 100, 50] }, { rule: [72, 500, 272, 500] }] };
const BOX = { left: 300, top: 192, width: 100, height: 50 };
const RULE = { left: 72, top: 292, width: 200, height: 0 };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;
type Box = typeof BOX;

const item = (page: Page) => page.getByRole('menu', { name: 'Tools' }).getByRole('menuitemcheckbox', { name: 'Edit content' });
const mode = (page: Page) => page.getByRole('group', { name: 'Edit content' });
const status = (page: Page) => mode(page).getByRole('status');
const layer = (page: Page) => pageSlot(page, 1).locator('.content-layer');
const selectedOutline = (page: Page) => layer(page).locator('.content-outline[data-state="selected"]');
const shapeToolbar = (page: Page) => page.getByRole('toolbar', { name: 'Shape' });
const saveButton = (page: Page) => page.locator('.workspace__bar').getByRole('button', { name: 'Save', exact: true });
const history = (page: Page, name: 'Undo' | 'Redo') =>
  page.getByRole('toolbar', { name: 'History' }).getByRole('button', { name, exact: true });
const SHAPE_SELECTED = 'Shape selected on page 1.';
const HINT = 'Point at text, an image or a shape on a page, and click to select it.';

async function enterMode(page: Page): Promise<void> {
  await toolsButton(page).click();
  await expect(item(page)).not.toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  await item(page).click();
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
}

/** Clicks at (x, y) points on page 1 as drawn. */
async function clickAt(page: Page, x: number, y: number): Promise<void> {
  const box = (await pageSlot(page, 1).boundingBox())!;
  await page.mouse.click(box.x + x * SCALE, box.y + y * SCALE);
}

/** Selects the path under (x, y) once the page is read, and waits for the status to say so. */
async function selectPath(page: Page, x: number, y: number): Promise<void> {
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, x, y);
  await expect(status(page)).toHaveText(SHAPE_SELECTED, { timeout: 30_000 });
  await expect(selectedOutline(page)).toHaveAttribute('data-kind', 'path');
}

/** Waits until the selected path's box is `expected` (to under a point). */
async function expectSelectedAt(page: Page, expected: Box): Promise<void> {
  await expect(status(page)).toHaveText(SHAPE_SELECTED, { timeout: 30_000 });
  await expect
    .poll(
      async () => {
        const slot = await pageSlot(page, 1).boundingBox();
        // Between an edit landing and the path being found again there is no outline.
        const box = await selectedOutline(page).boundingBox({ timeout: 1_000 }).catch(() => null);
        if (!slot || !box) return null;
        const shown = [(box.x - slot.x) / SCALE, (box.y - slot.y) / SCALE, box.width / SCALE, box.height / SCALE];
        // A rule of no height is still outlined: its border is all there is of it.
        if (expected.height === 0) shown[3] = box.height <= 6 ? 0 : shown[3]!;
        return shown.map((value, at) => Math.abs(value - Object.values(expected)[at]!) < 0.75);
      },
      { timeout: 30_000 },
    )
    .toEqual([true, true, true, true]);
}

/** Drags a resize handle by `dx, dy` px, in two legs, as a hand would. */
async function dragHandle(page: Page, name: string, dx: number, dy: number): Promise<void> {
  const handle = (await page.getByRole('button', { name }).boundingBox())!;
  const x = handle.x + handle.width / 2;
  const y = handle.y + handle.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx / 2, y + dy / 2, { steps: 4 });
  await page.mouse.move(x + dx, y + dy, { steps: 4 });
  await page.mouse.up();
}

async function savedBytes(page: Page): Promise<Buffer> {
  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  return readFileSync(await download.path());
}

async function reopen(page: Page, name: string, bytes: Buffer): Promise<void> {
  await openBytes(page, name, bytes);
  await enterMode(page);
}

test('a shape moves by the arrow keys and by a drag of its body, one step, saved where it was left', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'move-path.pdf', buildPdf([SHAPES]));
  await enterMode(page);
  await selectPath(page, BOX.left + 50, BOX.top + 25);
  await expect(shapeToolbar(page)).toBeVisible();
  await expectSelectedAt(page, BOX);

  // Two Shift+Down: 20 points.
  await page.keyboard.press('Shift+ArrowDown');
  await page.keyboard.press('Shift+ArrowDown');
  await expectSelectedAt(page, { ...BOX, top: BOX.top + 20 });
  await expect(saveButton(page)).toBeEnabled();

  // Dragged 60 x 30 px further, by its body.
  const body = (await selectedOutline(page).boundingBox())!;
  await page.mouse.move(body.x + 40, body.y + 30);
  await page.mouse.down();
  await page.mouse.move(body.x + 70, body.y + 45, { steps: 4 });
  await page.mouse.move(body.x + 100, body.y + 60, { steps: 4 });
  await page.mouse.up();
  const moved = { ...BOX, left: BOX.left + 60 / SCALE, top: BOX.top + 20 + 30 / SCALE };
  await expectSelectedAt(page, moved);

  // The moves of one path are one step.
  await history(page, 'Undo').click();
  await selectPath(page, BOX.left + 50, BOX.top + 25);
  await expectSelectedAt(page, BOX);
  await expect(history(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');
  await history(page, 'Redo').click();
  await selectPath(page, moved.left + 50, moved.top + 25);
  await expectSelectedAt(page, moved);

  const bytes = await savedBytes(page);
  await reopen(page, 'moved-path.pdf', bytes);
  await selectPath(page, moved.left + 50, moved.top + 25);
  await expectSelectedAt(page, moved);
  // Nothing is left where it was.
  await clickAt(page, BOX.left + 5, BOX.top + 5);
  await expect(status(page)).toHaveText(HINT);
});

test('a corner resizes a shape freely, a rule lengthens from its end and stays flat, and the saved file keeps both', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'resize-path.pdf', buildPdf([SHAPES]));
  await enterMode(page);
  await selectPath(page, BOX.left + 50, BOX.top + 25);
  // A shape has eight handles; whole pixels, as every browser moves the pointer: 60 x 30 px is 45 x 22.5 pt.
  await expect(page.getByRole('button', { name: /^Resize from/ })).toHaveCount(8);
  await dragHandle(page, 'Resize from the bottom-right corner', 60, 30);
  const resized = { ...BOX, width: BOX.width + 45, height: BOX.height + 22.5 };
  await expectSelectedAt(page, resized);

  await selectPath(page, RULE.left + 100, RULE.top);
  await expectSelectedAt(page, RULE);
  // A rule has only the two handles at its ends.
  await expect(page.getByRole('button', { name: /^Resize from/ })).toHaveCount(2);
  await dragHandle(page, 'Resize from the right side', 60, 40);
  const lengthened = { ...RULE, width: RULE.width + 45 };
  await expectSelectedAt(page, lengthened);

  const bytes = await savedBytes(page);
  await reopen(page, 'resized-path.pdf', bytes);
  await selectPath(page, resized.left + 100, resized.top + 50);
  await expectSelectedAt(page, resized);
  await selectPath(page, lengthened.left + 230, lengthened.top);
  await expectSelectedAt(page, lengthened);
});

test('Delete removes the selected shape, the toolbar a rule; undo and redo, and the saved file paints neither', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'delete-path.pdf', buildPdf([SHAPES]));
  await enterMode(page);
  await selectPath(page, BOX.left + 50, BOX.top + 25);
  await page.keyboard.press('Delete');
  await expect(status(page)).toHaveText(HINT, { timeout: 30_000 });
  await expect(selectedOutline(page)).toHaveCount(0);
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, BOX.left + 50, BOX.top + 25);
  await expect(status(page)).toHaveText(HINT);

  await selectPath(page, RULE.left + 100, RULE.top);
  await shapeToolbar(page).getByRole('button', { name: 'Delete shape' }).click();
  await expect(status(page)).toHaveText(HINT, { timeout: 30_000 });
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, RULE.left + 100, RULE.top);
  await expect(status(page)).toHaveText(HINT);

  // Two steps: the rule comes back first, then the box.
  await history(page, 'Undo').click();
  await selectPath(page, RULE.left + 100, RULE.top);
  await history(page, 'Undo').click();
  await selectPath(page, BOX.left + 50, BOX.top + 25);
  await history(page, 'Redo').click();
  await history(page, 'Redo').click();
  await expect(history(page, 'Redo')).toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, BOX.left + 50, BOX.top + 25);
  await expect(status(page)).toHaveText(HINT);

  const bytes = await savedBytes(page);
  await reopen(page, 'deleted-path.pdf', bytes);
  await clickAt(page, BOX.left + 50, BOX.top + 25);
  await expect(status(page)).toHaveText(HINT);
  await clickAt(page, RULE.left + 100, RULE.top);
  await expect(status(page)).toHaveText(HINT);
});

test('a shape on a page turned 90° moves and resizes as drawn, and is saved where it was left', async ({ page }) => {
  await loadApp(page);
  // Turned a quarter clockwise: 792 x 612 as drawn, the box's PDF
  // (100, 550)–(200, 600) drawn 50 wide and 100 tall at (550, 100).
  await openBytes(page, 'rotated-path.pdf', buildPdf([{ widthPt: 612, heightPt: 792, rotate: 90, paths: [{ box: [100, 550, 100, 50] }] }]));
  await enterMode(page);
  const drawn = { left: 550, top: 100, width: 50, height: 100 };
  await selectPath(page, drawn.left + 25, drawn.top + 50);
  await expectSelectedAt(page, drawn);

  // Shift+Right: 10 points to the right on screen.
  await page.keyboard.press('Shift+ArrowRight');
  const moved = { ...drawn, left: drawn.left + 10 };
  await expectSelectedAt(page, moved);

  // The bottom-right corner as drawn, 60 x 30 px: 45 x 22.5 points, freely.
  await dragHandle(page, 'Resize from the bottom-right corner', 60, 30);
  const resized = { ...moved, width: 95, height: 122.5 };
  await expectSelectedAt(page, resized);

  const bytes = await savedBytes(page);
  await reopen(page, 'rotated-path-saved.pdf', bytes);
  await selectPath(page, resized.left + 45, resized.top + 60);
  await expectSelectedAt(page, resized);
  // Nothing is left where it was drawn first.
  await clickAt(page, drawn.left + 3, drawn.top + 3);
  await expect(status(page)).toHaveText(HINT);
});
