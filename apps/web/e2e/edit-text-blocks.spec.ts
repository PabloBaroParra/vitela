// Text as blocks in the "Edit content" mode (B26 T-261): Insert text arms a
// click on the page, where an empty box opens and what is typed becomes a new
// line of page content; a run that is part of a paragraph offers Edit
// paragraph, a multi-line editor over the whole paragraph whose text the core
// wraps back to its width. Each edit is one undo step, marks the document
// dirty, and is in the saved file, read back by opening it again.

import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { LINE_FONT_PT, MODIFY_PROHIBITED, buildPdf, lineBaseline, type PageSpec } from './pdf';
import { occurrences } from './pdfObjects';
import { expectOpened, loadApp, openBytes, pageSlot, toolsButton } from './support';

/** Two lines a leading apart in one font: the core reads them as one paragraph. */
const PARAGRAPH: PageSpec = { widthPt: 612, heightPt: 792, lines: ['Vitela paragraph opening line', 'and the line under it'] };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;
/** The middle of line `index`'s letters, in points from the page's top. */
const lineTop = (index: number) => PARAGRAPH.heightPt - lineBaseline(PARAGRAPH, index) - LINE_FONT_PT / 2;

const item = (page: Page) => page.getByRole('menu', { name: 'Tools' }).getByRole('menuitemcheckbox', { name: 'Edit content' });
const mode = (page: Page) => page.getByRole('group', { name: 'Edit content' });
const status = (page: Page) => mode(page).getByRole('status');
const layer = (page: Page) => pageSlot(page, 1).locator('.content-layer');
const toolbar = (page: Page) => page.getByRole('toolbar', { name: 'Text run' });
const pages = (page: Page) => page.getByLabel('Pages', { exact: true });
const saveButton = (page: Page) => page.locator('.workspace__bar').getByRole('button', { name: 'Save', exact: true });
const history = (page: Page, name: 'Undo' | 'Redo') =>
  page.getByRole('toolbar', { name: 'History' }).getByRole('button', { name, exact: true });
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

/** Selects the run under (x, y) and waits for the status to name `text`. */
async function selectRun(page: Page, text: string, x: number, y: number): Promise<void> {
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

test('Insert text arms a click, the typed line is one undo step, and the saved file has it', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'insert-text.pdf', buildPdf([PARAGRAPH]));
  await enterMode(page);
  const insert = mode(page).getByRole('button', { name: 'Insert text' });

  // Escape at the pages disarms: nothing is added.
  await insert.click();
  await expect(insert).toHaveAttribute('aria-pressed', 'true');
  await expect(status(page)).toHaveText('Click the page where the text goes. Press Escape to cancel.');
  await expect(pages(page)).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(insert).toHaveAttribute('aria-pressed', 'false');
  await expect(status(page)).toHaveText(HINT);

  // Escape in the box records nothing either.
  await insert.click();
  await clickAt(page, 100, 300);
  const box = page.getByRole('textbox', { name: 'New text' });
  await expect(box).toBeFocused();
  await expect(insert).toHaveAttribute('aria-pressed', 'false');
  await box.fill('Never kept');
  await box.press('Escape');
  await expect(box).toHaveCount(0);
  await expect(history(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');

  await insert.click();
  await clickAt(page, 100, 300);
  // It hangs from the click, in the size the core paints it.
  const pageBox = (await pageSlot(page, 1).boundingBox())!;
  const field = (await box.boundingBox())!;
  expect(Math.abs(field.x - (pageBox.x + 100 * SCALE))).toBeLessThan(2);
  expect(Math.abs(field.y - (pageBox.y + 300 * SCALE))).toBeLessThan(2);
  const fontPx = await box.evaluate((field) => parseFloat(getComputedStyle(field).fontSize));
  expect(fontPx).toBeCloseTo(14 * SCALE, 1);
  await box.fill('Added on the web');
  await box.press('Enter');
  await expect(box).toHaveCount(0);
  // The new line is selected where it landed, and the pages have the keyboard.
  await expect(status(page)).toHaveText('Text “Added on the web” selected on page 1.', { timeout: 30_000 });
  await expect(pages(page)).toBeFocused();
  await expect(saveButton(page)).toBeEnabled();

  await history(page, 'Undo').click();
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, 110, 307);
  await expect(status(page)).toHaveText(HINT);
  await history(page, 'Redo').click();
  await selectRun(page, 'Added on the web', 110, 307);

  const bytes = await savedBytes(page);
  expect(occurrences(bytes, '(Added on the web)')).toBeGreaterThan(0);
  await reopen(page, 'inserted.pdf', bytes);
  await selectRun(page, 'Added on the web', 110, 307);
});

test('a line inserted and not saved yet is retyped like any other run', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'insert-retype.pdf', buildPdf([PARAGRAPH]));
  await enterMode(page);
  await mode(page).getByRole('button', { name: 'Insert text' }).click();
  await clickAt(page, 100, 300);
  const box = page.getByRole('textbox', { name: 'New text' });
  await box.fill('First draft');
  await box.press('Enter');
  await expect(status(page)).toHaveText('Text “First draft” selected on page 1.', { timeout: 30_000 });

  await page.keyboard.press('Enter');
  const editor = page.getByRole('textbox', { name: 'Edit text “First draft”' });
  await expect(editor).toBeFocused();
  await editor.fill('Final words');
  await editor.press('Enter');
  await expect(status(page)).toHaveText('Text “Final words” selected on page 1.', { timeout: 30_000 });

  const bytes = await savedBytes(page);
  await reopen(page, 'insert-retyped.pdf', bytes);
  await selectRun(page, 'Final words', 110, 307);
});

test('a paragraph is retyped as a whole, grows a line, and the saved file reads it back', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'paragraph.pdf', buildPdf([PARAGRAPH]));
  await enterMode(page);
  await selectRun(page, 'and the line under it', 90, lineTop(1));
  const editParagraph = toolbar(page).getByRole('button', { name: 'Edit paragraph' });
  await expect(editParagraph).toBeEnabled({ timeout: 30_000 });
  await editParagraph.click();

  const editor = page.getByRole('textbox', { name: 'Edit paragraph “Vitela paragraph opening line…”' });
  await expect(editor).toBeFocused();
  await expect(editor).toHaveValue('Vitela paragraph opening line\nand the line under it');
  // Over the paragraph, in its size: the toolbar gives way to it.
  await expect(toolbar(page)).toHaveCount(0);
  await expect(editor).toHaveCSS('font-size', `${LINE_FONT_PT * SCALE}px`);

  // Enter is a line break; Ctrl+Enter commits.
  await editor.press('ControlOrMeta+a');
  await editor.press('Delete');
  await editor.pressSequentially('Rewritten opening line');
  await editor.press('Enter');
  await editor.pressSequentially('and the line under it');
  await editor.press('Enter');
  await editor.pressSequentially('plus one more');
  await expect(editor).toHaveValue('Rewritten opening line\nand the line under it\nplus one more');
  await editor.press('Control+Enter');
  await expect(editor).toHaveCount(0);
  // The first line is selected again, and the pages have the keyboard.
  await expect(status(page)).toHaveText('Text “Rewritten opening line” selected on page 1.', { timeout: 30_000 });
  await expect(pages(page)).toBeFocused();
  // The new third line sits a leading under the second.
  await selectRun(page, 'plus one more', 90, lineTop(2));

  // One step: one undo takes the whole paragraph back.
  await history(page, 'Undo').click();
  await selectRun(page, 'Vitela paragraph opening line', 90, lineTop(0));
  await expect(history(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');
  await history(page, 'Redo').click();
  await selectRun(page, 'Rewritten opening line', 90, lineTop(0));

  const bytes = await savedBytes(page);
  expect(occurrences(bytes, '(Rewritten opening line)')).toBeGreaterThan(0);
  expect(occurrences(bytes, '(plus one more)')).toBeGreaterThan(0);
  await reopen(page, 'paragraph-retyped.pdf', bytes);
  await selectRun(page, 'Rewritten opening line', 90, lineTop(0));
  await selectRun(page, 'plus one more', 90, lineTop(2));
  // Still one paragraph of three lines.
  await expect(toolbar(page).getByRole('button', { name: 'Edit paragraph' })).toBeEnabled({ timeout: 30_000 });
  await toolbar(page).getByRole('button', { name: 'Edit paragraph' }).click();
  await expect(page.getByRole('textbox', { name: /^Edit paragraph/ })).toHaveValue('Rewritten opening line\nand the line under it\nplus one more');
  await page.keyboard.press('Escape');
});

test('Shift+Enter opens the paragraph editor, Escape cancels it, and leaving it commits', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'paragraph-keys.pdf', buildPdf([PARAGRAPH]));
  await enterMode(page);
  await selectRun(page, 'Vitela paragraph opening line', 90, lineTop(0));
  await expect(toolbar(page).getByRole('button', { name: 'Edit paragraph' })).toBeEnabled({ timeout: 30_000 });
  await page.keyboard.press('Shift+Enter');
  const editor = page.getByRole('textbox', { name: /^Edit paragraph/ });
  await expect(editor).toBeFocused();
  await editor.fill('Never kept');
  await editor.press('Escape');
  await expect(editor).toHaveCount(0);
  await expect(history(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');

  await expect(pages(page)).toBeFocused();
  await page.keyboard.press('Shift+Enter');
  await expect(editor).toBeFocused();
  await editor.fill('One line now');
  // A press beside it commits it.
  await clickAt(page, 450, 320);
  await expect(editor).toHaveCount(0);
  await expect(history(page, 'Undo')).not.toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  await selectRun(page, 'One line now', 90, lineTop(0));
});

test('a run on one line offers no paragraph editor, and a document that forbids modification offers no insert', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'single.pdf', buildPdf([{ widthPt: 612, heightPt: 792, lines: ['Only line'] }]));
  await enterMode(page);
  await selectRun(page, 'Only line', 80, lineTop(0));
  await expect(toolbar(page).getByRole('button', { name: 'Edit text' })).toBeVisible();
  // A one-line block is retyped by the run editor: no paragraph to offer.
  await expect(toolbar(page).getByRole('button', { name: 'Edit paragraph' })).toHaveCount(0);

  const locked = buildPdf([PARAGRAPH], { encrypt: { permissions: MODIFY_PROHIBITED, userPassword: 'user-e2e' } });
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: locked });
  const prompt = page.getByRole('dialog', { name: 'Password required' });
  await expect(prompt).toBeVisible({ timeout: 30_000 });
  await prompt.getByLabel('Password').fill('user-e2e');
  await prompt.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');
  await toolsButton(page).click();
  await expect(item(page)).toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  await page.keyboard.press('Escape');
  await expect(mode(page).getByRole('button', { name: 'Insert text' })).toHaveCount(0);
});
