// Editing text runs in the "Edit content" mode (B26 T-259): with a run
// selected, Enter (or a double-click) opens a camouflaged editor over it and
// Enter commits; the arrow keys and a drag of its outline move it; Delete
// removes it; Bold toggles its face. Each edit is one undo step, marks the
// document dirty, and is in the saved file, read back by opening it again.

import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { LINE_FONT_PT, MODIFY_PROHIBITED, buildPdf, lineBaseline, type PageSpec } from './pdf';
import { occurrences } from './pdfObjects';
import { expectOpened, loadApp, openBytes, pageSlot, toolsButton } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792, lines: ['Vitela content line'] };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;
/** The middle of the line's first letters, in points from the page's top. */
const LINE_TOP = LETTER.heightPt - lineBaseline(LETTER, 0) - LINE_FONT_PT / 2;

const item = (page: Page) => page.getByRole('menu', { name: 'Tools' }).getByRole('menuitemcheckbox', { name: 'Edit content' });
const mode = (page: Page) => page.getByRole('group', { name: 'Edit content' });
const status = (page: Page) => mode(page).getByRole('status');
const layer = (page: Page) => pageSlot(page, 1).locator('.content-layer');
const selectedOutline = (page: Page) => layer(page).locator('.content-outline[data-state="selected"]');
const toolbar = (page: Page) => page.getByRole('toolbar', { name: 'Text run' });
const pages = (page: Page) => page.getByLabel('Pages', { exact: true });
const saveButton = (page: Page) => page.locator('.workspace__bar').getByRole('button', { name: 'Save', exact: true });
const history = (page: Page, name: 'Undo' | 'Redo') =>
  page.getByRole('toolbar', { name: 'History' }).getByRole('button', { name, exact: true });

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

/** Selects the run under (x, y) and waits for the status to name `text`. */
async function selectRun(page: Page, text: string, x = 90, y = LINE_TOP): Promise<void> {
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, x, y);
  await expect(status(page)).toHaveText(`Text “${text}” selected on page 1.`);
}

async function savedBytes(page: Page): Promise<Buffer> {
  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  return readFileSync(await download.path());
}

/** Opens `bytes` as another document and turns the mode on in it. */
async function reopen(page: Page, name: string, bytes: Buffer): Promise<void> {
  await openBytes(page, name, bytes);
  await enterMode(page);
}

test('a run is retyped in place, one undo step, and the saved file reads the new text', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'retype.pdf', buildPdf([LETTER]));
  await enterMode(page);
  await selectRun(page, 'Vitela content line');
  await page.keyboard.press('Enter');
  const editor = page.getByRole('textbox', { name: 'Edit text “Vitela content line”' });
  await expect(editor).toBeFocused();
  await expect(editor).toHaveValue('Vitela content line');
  // Laid over the run, in its size.
  const outline = (await selectedOutline(page).boundingBox())!;
  const field = (await editor.boundingBox())!;
  expect(Math.abs(field.x - outline.x)).toBeLessThan(2);
  expect(Math.abs(field.y - outline.y)).toBeLessThan(2);
  await expect(editor).toHaveCSS('font-size', `${LINE_FONT_PT * SCALE}px`);

  await editor.fill('Vitela web line');
  await editor.press('Enter');
  await expect(editor).toHaveCount(0);
  // The run is found again where it was, with its new text, and the pages have the keyboard.
  await expect(status(page)).toHaveText('Text “Vitela web line” selected on page 1.', { timeout: 30_000 });
  await expect(pages(page)).toBeFocused();
  await expect(saveButton(page)).toBeEnabled();

  await history(page, 'Undo').click();
  await selectRun(page, 'Vitela content line');
  await history(page, 'Redo').click();
  await selectRun(page, 'Vitela web line');

  const bytes = await savedBytes(page);
  expect(occurrences(bytes, '(Vitela web line)')).toBeGreaterThan(0);
  expect(occurrences(bytes, '(Vitela content line)')).toBe(0);
  await reopen(page, 'retyped.pdf', bytes);
  await selectRun(page, 'Vitela web line');
});

test('a double-click opens the editor too, Escape cancels it, and leaving it commits', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'dblclick.pdf', buildPdf([LETTER]));
  await enterMode(page);
  await selectRun(page, 'Vitela content line');
  const box = (await pageSlot(page, 1).boundingBox())!;
  await page.mouse.dblclick(box.x + 90 * SCALE, box.y + LINE_TOP * SCALE);
  const editor = page.getByRole('textbox', { name: 'Edit text “Vitela content line”' });
  await expect(editor).toBeFocused();
  await editor.fill('Never kept');
  await editor.press('Escape');
  await expect(editor).toHaveCount(0);
  await expect(history(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');

  await page.keyboard.press('Enter');
  await expect(editor).toBeFocused();
  await editor.fill('Kept on leaving');
  // A press on the page beside the run (on nothing, inside the viewport) commits what was typed.
  await clickAt(page, 450, 250);
  await expect(editor).toHaveCount(0);
  await selectRun(page, 'Kept on leaving');
});

test('a run moves by the arrow keys and by a drag of its outline, saved where it was left', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'move.pdf', buildPdf([LETTER]));
  await enterMode(page);
  await selectRun(page, 'Vitela content line');
  const before = (await selectedOutline(page).boundingBox())!;

  // Three Shift+Right: 30 points, however fast they come.
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.press('Shift+ArrowRight');
  await expect
    .poll(async () => ((await selectedOutline(page).boundingBox())?.x ?? 0) - before.x, { timeout: 30_000 })
    .toBeCloseTo(30 * SCALE, 0);
  await expect(status(page)).toHaveText('Text “Vitela content line” selected on page 1.');

  // Dragged 40 x 20 px further, by its outline.
  const moved = (await selectedOutline(page).boundingBox())!;
  await page.mouse.move(moved.x + 10, moved.y + moved.height / 2);
  await page.mouse.down();
  await page.mouse.move(moved.x + 30, moved.y + moved.height / 2 + 10, { steps: 4 });
  await page.mouse.move(moved.x + 50, moved.y + moved.height / 2 + 20, { steps: 4 });
  await page.mouse.up();
  await expect
    .poll(async () => ((await selectedOutline(page).boundingBox())?.y ?? 0) - before.y, { timeout: 30_000 })
    .toBeCloseTo(20, 0);
  const dropped = (await selectedOutline(page).boundingBox())!;
  expect(dropped.x - before.x).toBeCloseTo(30 * SCALE + 40, 0);

  // The moves of one run are one step: one undo puts it back, one redo moves it again.
  await history(page, 'Undo').click();
  await selectRun(page, 'Vitela content line');
  expect(((await selectedOutline(page).boundingBox())!.x - before.x)).toBeCloseTo(0, 0);
  await history(page, 'Redo').click();
  const pageBox = (await pageSlot(page, 1).boundingBox())!;
  const dropX = (dropped.x - pageBox.x) / SCALE + 10;
  const dropY = (dropped.y - pageBox.y + dropped.height / 2) / SCALE;
  await selectRun(page, 'Vitela content line', dropX, dropY);

  const bytes = await savedBytes(page);
  await reopen(page, 'moved.pdf', bytes);
  await selectRun(page, 'Vitela content line', dropX, dropY);
  const saved = (await selectedOutline(page).boundingBox())!;
  const reopenedPage = (await pageSlot(page, 1).boundingBox())!;
  expect(saved.x - reopenedPage.x).toBeCloseTo(dropped.x - pageBox.x, 0);
  expect(saved.y - reopenedPage.y).toBeCloseTo(dropped.y - pageBox.y, 0);
});

test('Delete removes the selected run; undo brings it back, redo removes it again, and the save leaves it out', async ({
  page,
}) => {
  await loadApp(page);
  await openBytes(page, 'delete.pdf', buildPdf([LETTER]));
  await enterMode(page);
  await selectRun(page, 'Vitela content line');
  await page.keyboard.press('Delete');
  await expect(status(page)).toHaveText('Point at text, an image or a shape on a page, and click to select it.', {
    timeout: 30_000,
  });
  await expect(selectedOutline(page)).toHaveCount(0);
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, 90, LINE_TOP);
  await expect(selectedOutline(page)).toHaveCount(0);

  await history(page, 'Undo').click();
  await selectRun(page, 'Vitela content line');
  await history(page, 'Redo').click();
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, 90, LINE_TOP);
  await expect(selectedOutline(page)).toHaveCount(0);

  // Read back from the saved file, the run is gone and the rest of the page is not. (The
  // stream it was painted in stays in the file unreferenced, as on every shell: the core
  // copies a stream before editing it and leaves the sweep to a compressed save.)
  const bytes = await savedBytes(page);
  expect(occurrences(bytes, '(Page 1)')).toBeGreaterThan(0);
  await reopen(page, 'deleted.pdf', bytes);
  await clickAt(page, 90, LINE_TOP);
  await expect(selectedOutline(page)).toHaveCount(0);
  await expect(status(page)).toHaveText('Point at text, an image or a shape on a page, and click to select it.');
});

test("Bold turns the run's face bold, read back as the core's look after the save", async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'bold.pdf', buildPdf([LETTER]));
  await enterMode(page);
  await selectRun(page, 'Vitela content line');
  const bold = toolbar(page).getByRole('button', { name: 'Bold' });
  await expect(bold).toHaveAttribute('aria-pressed', 'false');
  await expect(bold).toBeEnabled({ timeout: 30_000 });
  await bold.click();
  await expect(bold).toHaveAttribute('aria-pressed', 'true', { timeout: 30_000 });
  await expect(toolbar(page).getByRole('button', { name: 'Italic' })).toHaveAttribute('aria-pressed', 'false');

  await history(page, 'Undo').click();
  await selectRun(page, 'Vitela content line');
  await expect(bold).toHaveAttribute('aria-pressed', 'false', { timeout: 30_000 });
  await history(page, 'Redo').click();
  await selectRun(page, 'Vitela content line');
  await expect(bold).toHaveAttribute('aria-pressed', 'true', { timeout: 30_000 });

  const bytes = await savedBytes(page);
  await reopen(page, 'bolded.pdf', bytes);
  await selectRun(page, 'Vitela content line');
  await expect(toolbar(page).getByRole('button', { name: 'Bold' })).toHaveAttribute('aria-pressed', 'true', {
    timeout: 30_000,
  });
});

test('a document that forbids modification offers no text edit at all', async ({ page }) => {
  const locked = buildPdf([LETTER], { encrypt: { permissions: MODIFY_PROHIBITED, userPassword: 'user-e2e' } });
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: locked });
  const prompt = page.getByRole('dialog', { name: 'Password required' });
  await expect(prompt).toBeVisible({ timeout: 30_000 });
  await prompt.getByLabel('Password').fill('user-e2e');
  await prompt.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');

  await toolsButton(page).click();
  await expect(item(page)).toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  await item(page).click({ force: true });
  await page.keyboard.press('Escape');
  await expect(page.locator('.content-layer')).toHaveCount(0);
  // The keys a selected run takes do nothing on these pages.
  await pages(page).focus();
  for (const key of ['Enter', 'Delete', 'ArrowRight']) await page.keyboard.press(key);
  await expect(toolbar(page)).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: /^Edit text/ })).toHaveCount(0);
  await expect(history(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');
});
