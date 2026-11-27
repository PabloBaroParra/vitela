// Several items at once and the content clipboard in the "Edit content" mode
// (B26 T-262): a marquee dragged from where nothing is gathers what it
// touches, SHIFT/CTRL+click adds or takes out one item, and the group moves
// or is deleted as one step; Copy, Cut and Paste (the status row's buttons,
// or Ctrl/⌘ + C, X, V) take it to another page — and to another document
// opened after it — one undo step each, a paste dragged into place still its
// one step. Every edit is in the saved file, read back by opening it again.

import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { buildPdf, lineBaseline, type PageSpec } from './pdf';
import { loadApp, openBytes, pageSlot, toolsButton } from './support';

/** CSS px per point at 100%. */
const SCALE = 96 / 72;
const PAGE: PageSpec = { widthPt: 612, heightPt: 792, lines: ['Copy me'], images: [[300, 560, 100, 50]] };
/** The line's box top on the page as drawn, roughly, and the image's box. */
const LINE = { left: 72, top: 792 - lineBaseline(PAGE, 0) - 13 };
const IMAGE = { left: 300, top: 792 - 610, width: 100, height: 50 };
const SECOND: PageSpec = { widthPt: 612, heightPt: 792, lines: ['Second page line'] };

const item = (page: Page) => page.getByRole('menu', { name: 'Tools' }).getByRole('menuitemcheckbox', { name: 'Edit content' });
const mode = (page: Page) => page.getByRole('group', { name: 'Edit content' });
const status = (page: Page) => mode(page).getByRole('status');
const clipboardButton = (page: Page, name: 'Copy' | 'Cut' | 'Paste') =>
  mode(page).getByRole('group', { name: 'Clipboard' }).getByRole('button', { name, exact: true });
const layer = (page: Page, number: number) => pageSlot(page, number).locator('.content-layer');
const selected = (page: Page, number: number) => layer(page, number).locator('.content-outline[data-state="selected"]');
const selectedImage = (page: Page, number: number) => layer(page, number).locator('.content-outline[data-kind="image"][data-state="selected"]');
const saveButton = (page: Page) => page.locator('.workspace__bar').getByRole('button', { name: 'Save', exact: true });
const history = (page: Page, name: 'Undo' | 'Redo') =>
  page.getByRole('toolbar', { name: 'History' }).getByRole('button', { name, exact: true });
const HINT = 'Point at text, an image or a shape on a page, and click to select it.';
const TWO_SELECTED = '2 items selected on page 1. Drag them, press Delete, or copy them.';
const modifier = process.platform === 'darwin' ? 'Meta' : 'Control';

async function enterMode(page: Page): Promise<void> {
  await toolsButton(page).click();
  await expect(item(page)).not.toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  await item(page).click();
  await expect(layer(page, 1)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
}

/** The point (x, y) on page `number` as drawn, in viewport pixels. */
async function at(page: Page, number: number, x: number, y: number): Promise<{ x: number; y: number }> {
  const box = (await pageSlot(page, number).boundingBox())!;
  return { x: box.x + x * SCALE, y: box.y + y * SCALE };
}

async function clickAt(page: Page, number: number, x: number, y: number, modifiers: ('Shift' | 'Control')[] = []): Promise<void> {
  const point = await at(page, number, x, y);
  for (const key of modifiers) await page.keyboard.down(key);
  await page.mouse.click(point.x, point.y);
  for (const key of modifiers) await page.keyboard.up(key);
}

/** Drags a marquee on page `number` from one drawn point to another. */
async function marquee(page: Page, number: number, from: [number, number], to: [number, number]): Promise<void> {
  const start = await at(page, number, ...from);
  const end = await at(page, number, ...to);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move((start.x + end.x) / 2, (start.y + end.y) / 2, { steps: 4 });
  await page.mouse.move(end.x, end.y, { steps: 4 });
  await page.mouse.up();
}

/** Drags the group on page `number` by its member at drawn (x, y), `dx, dy` CSS px. */
async function dragFrom(page: Page, number: number, x: number, y: number, dx: number, dy: number): Promise<void> {
  const start = await at(page, number, x, y);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(start.x + dx / 2, start.y + dy / 2, { steps: 4 });
  await page.mouse.move(start.x + dx, start.y + dy, { steps: 4 });
  await page.mouse.up();
}

/** Brings page `number` to the top of the scroller and waits until it is read. */
async function showPage(page: Page, number: number): Promise<void> {
  // Only pages near the visible ones are mounted: scroll towards it first.
  const scroller = page.locator('.viewer__scroller');
  await scroller.evaluate((element, last) => (element.scrollTop = last ? element.scrollHeight : 0), number > 1);
  await pageSlot(page, number).evaluate((slot) => slot.scrollIntoView({ block: 'start' }));
  await expect(layer(page, number)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
}

/** Waits for the content on page `number` to be read, then selects what is at (x, y) and says it. */
async function select(page: Page, number: number, x: number, y: number, said: string): Promise<void> {
  await expect(layer(page, number)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await expect(async () => {
    await clickAt(page, number, x, y);
    await expect(status(page)).toHaveText(said, { timeout: 2_000 });
  }).toPass({ timeout: 30_000 });
}

async function savedBytes(page: Page): Promise<Buffer> {
  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  return readFileSync(await download.path());
}

async function reopen(page: Page, name: string, bytes: Buffer): Promise<void> {
  await openBytes(page, name, bytes);
  await enterMode(page);
}

const undoEnabled = (page: Page) => expect(history(page, 'Undo')).not.toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
const undoDisabled = (page: Page) => expect(history(page, 'Undo')).toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });

test('a marquee gathers a line and an image; copied with the keys and pasted on the next page at the pointer, dragged into place as one step', async ({
  page,
}) => {
  await loadApp(page);
  await openBytes(page, 'copy-between-pages.pdf', buildPdf([PAGE, SECOND]));
  await enterMode(page);
  await expect(clipboardButton(page, 'Copy')).toBeDisabled();
  await expect(clipboardButton(page, 'Paste')).toBeDisabled();

  // From the empty margin, round the line and the image (not the page number further down).
  await marquee(page, 1, [50, 125], [420, 250]);
  await expect(status(page)).toHaveText(TWO_SELECTED, { timeout: 30_000 });
  await expect(selected(page, 1)).toHaveCount(2);
  await expect(layer(page, 1).locator('.content-group')).toHaveCount(1);

  await page.keyboard.press(`${modifier}+c`);
  await expect(status(page)).toHaveText('Copied 2 items.', { timeout: 30_000 });
  await expect(clipboardButton(page, 'Paste')).toBeEnabled();
  // A copy changes nothing.
  await undoDisabled(page);

  // Pasted where the pointer is over page 2.
  await showPage(page, 2);
  const target = await at(page, 2, 100, 300);
  await page.mouse.move(target.x, target.y);
  await page.keyboard.press(`${modifier}+v`);
  await expect(status(page)).toHaveText('Pasted on page 2. Drag it into place.', { timeout: 30_000 });
  await expect(selected(page, 2)).toHaveCount(2, { timeout: 30_000 });
  await undoEnabled(page);
  await expect(saveButton(page)).toBeEnabled();

  // The paste hangs from the pointer: its line is at the top-left, its image right of it.
  const outlines = await selected(page, 2).evaluateAll((nodes) =>
    nodes.map((node) => ({ kind: node.getAttribute('data-kind'), box: node.getBoundingClientRect() })),
  );
  const slot = (await pageSlot(page, 2).boundingBox())!;
  const image = outlines.find((outline) => outline.kind === 'image')!.box;
  expect((image.x - slot.x) / SCALE).toBeCloseTo(100 + IMAGE.left - LINE.left, -1);

  // Dragged 60 px right: the paste moves, still one step.
  await dragFrom(page, 2, 100 + IMAGE.left - LINE.left + 20, 300 + IMAGE.top - LINE.top + 20, 60, 0);
  await expect
    .poll(async () => ((await selectedImage(page, 2).boundingBox())?.x ?? 0) - slot.x, {
      timeout: 30_000,
    })
    .toBeGreaterThan(image.x - slot.x + 50);
  await history(page, 'Undo').click();
  await undoDisabled(page);
  await showPage(page, 2);
  await select(page, 2, 100 + 10, 300 + 5, HINT);
  await history(page, 'Redo').click();
  await undoEnabled(page);

  const bytes = await savedBytes(page);
  await reopen(page, 'pasted.pdf', bytes);
  // Both pages have the line and the image now; the paste is where it was dragged.
  await select(page, 1, LINE.left + 10, LINE.top + 8, 'Text “Copy me” selected on page 1.');
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 25, 'Image selected on page 1.');
  await showPage(page, 2);
  const dragged = 60 / SCALE;
  await select(page, 2, 100 + dragged + 10, 300 + 8, 'Text “Copy me” selected on page 2.');
  await select(page, 2, 100 + dragged + IMAGE.left - LINE.left + 50, 300 + IMAGE.top - LINE.top + 25, 'Image selected on page 2.');
});

test('Cut takes an image off its page as one step; Paste puts it at the same place on the page in view', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'cut-between-pages.pdf', buildPdf([PAGE, SECOND]));
  await enterMode(page);
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 25, 'Image selected on page 1.');
  await expect(clipboardButton(page, 'Cut')).toBeEnabled();
  await clipboardButton(page, 'Cut').click();
  await expect(status(page)).toHaveText('Cut 1 item.', { timeout: 30_000 });
  await undoEnabled(page);
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 25, HINT);

  await showPage(page, 2);
  await clipboardButton(page, 'Paste').click();
  await expect(status(page)).toHaveText('Pasted on page 2. Drag it into place.', { timeout: 30_000 });
  // One image alone is selected as a click selects it.
  await expect(page.getByRole('toolbar', { name: 'Image' })).toBeVisible({ timeout: 30_000 });

  // The cut and the paste: two steps.
  await history(page, 'Undo').click();
  await select(page, 2, IMAGE.left + 50, IMAGE.top + 25, HINT);
  await history(page, 'Undo').click();
  await undoDisabled(page);
  await showPage(page, 1);
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 25, 'Image selected on page 1.');
  await history(page, 'Redo').click();
  await history(page, 'Redo').click();
  await expect(history(page, 'Redo')).toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });

  const bytes = await savedBytes(page);
  await reopen(page, 'cut.pdf', bytes);
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 25, HINT);
  await select(page, 1, LINE.left + 10, LINE.top + 8, 'Text “Copy me” selected on page 1.');
  await showPage(page, 2);
  await select(page, 2, IMAGE.left + 50, IMAGE.top + 25, 'Image selected on page 2.');
});

test('Shift+click adds and takes out one item; the group moves with the keys and is deleted as one step', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'group.pdf', buildPdf([PAGE]));
  await enterMode(page);
  await select(page, 1, LINE.left + 10, LINE.top + 8, 'Text “Copy me” selected on page 1.');
  await clickAt(page, 1, IMAGE.left + 50, IMAGE.top + 25, ['Shift']);
  await expect(status(page)).toHaveText(TWO_SELECTED, { timeout: 30_000 });
  // Taken out again: the line alone is the selection, its toolbar back.
  await clickAt(page, 1, IMAGE.left + 50, IMAGE.top + 25, ['Shift']);
  await expect(status(page)).toHaveText('Text “Copy me” selected on page 1.', { timeout: 30_000 });
  await clickAt(page, 1, IMAGE.left + 50, IMAGE.top + 25, ['Control']);
  await expect(status(page)).toHaveText(TWO_SELECTED, { timeout: 30_000 });

  // Two Shift+Down: both 20 pt lower, one step.
  await page.keyboard.press('Shift+ArrowDown');
  await page.keyboard.press('Shift+ArrowDown');
  await expect(status(page)).toHaveText(TWO_SELECTED, { timeout: 30_000 });
  await expect
    .poll(async () => {
      const box = await selectedImage(page, 1).boundingBox();
      const slot = (await pageSlot(page, 1).boundingBox())!;
      return box ? Math.round((box.y - slot.y) / SCALE) : null;
    }, { timeout: 30_000 })
    .toBe(IMAGE.top + 20);
  await history(page, 'Undo').click();
  await undoDisabled(page);
  await history(page, 'Redo').click();

  // Delete takes both off, one step.
  await marquee(page, 1, [50, 125], [420, 280]);
  await expect(status(page)).toHaveText(TWO_SELECTED, { timeout: 30_000 });
  await page.keyboard.press('Delete');
  await expect(status(page)).toHaveText(HINT, { timeout: 30_000 });
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 45, HINT);
  await history(page, 'Undo').click();
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 45, 'Image selected on page 1.');
  await history(page, 'Redo').click();

  const bytes = await savedBytes(page);
  await reopen(page, 'deleted.pdf', bytes);
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 45, HINT);
  await select(page, 1, LINE.left + 10, LINE.top + 28, HINT);
});

test('a clip outlives its document: copied in one PDF, pasted into the next one opened, and a text field keeps its own keys', async ({
  page,
}) => {
  await loadApp(page);
  await openBytes(page, 'source.pdf', buildPdf([PAGE]));
  await enterMode(page);
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 25, 'Image selected on page 1.');
  await clipboardButton(page, 'Copy').click();
  await expect(status(page)).toHaveText('Copied 1 item.', { timeout: 30_000 });

  await reopen(page, 'target.pdf', buildPdf([SECOND]));
  await expect(clipboardButton(page, 'Paste')).toBeEnabled();
  // Ctrl+V typed into the search box is the field's: nothing is pasted on the page.
  const search = page.getByRole('searchbox');
  await search.focus();
  await page.keyboard.press(`${modifier}+v`);
  await expect(status(page)).toHaveText(HINT);
  await undoDisabled(page);

  await clipboardButton(page, 'Paste').click();
  await expect(status(page)).toHaveText('Pasted on page 1. Drag it into place.', { timeout: 30_000 });
  await undoEnabled(page);
  const bytes = await savedBytes(page);
  await reopen(page, 'target-pasted.pdf', bytes);
  await select(page, 1, IMAGE.left + 50, IMAGE.top + 25, 'Image selected on page 1.');
  await select(page, 1, LINE.left + 10, LINE.top + 8, 'Text “Second page line” selected on page 1.');
});
