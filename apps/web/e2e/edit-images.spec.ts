// Editing images in the "Edit content" mode (B26 T-260): a selected image
// moves by the arrow keys and by a drag of its body, resizes from its eight
// handles (a corner keeps the proportions, Shift frees them), is deleted by
// Delete or its toolbar, and has its picture replaced (the core is asked
// first, the file picker after); Insert image picks a file and arms a click
// on the page. Each edit is one undo step, marks the document dirty, and is
// in the saved file, read back by opening it again.

import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { MODIFY_PROHIBITED, buildPdf, type PageSpec } from './pdf';
import { subtypeDictionaries } from './pdfObjects';
import { buildPng } from './png';
import { expectOpened, loadApp, openBytes, pageSlot, toolsButton } from './support';

/** One 200 x 100 pt image, left 100 and bottom 550 in PDF space: drawn at (100, 142), well inside the viewport. */
const PHOTO: PageSpec = { widthPt: 612, heightPt: 792, images: [[100, 550, 200, 100]] };
const DRAWN = { left: 100, top: 142, width: 200, height: 100 };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;

const item = (page: Page) => page.getByRole('menu', { name: 'Tools' }).getByRole('menuitemcheckbox', { name: 'Edit content' });
const mode = (page: Page) => page.getByRole('group', { name: 'Edit content' });
const status = (page: Page) => mode(page).getByRole('status');
const layer = (page: Page) => pageSlot(page, 1).locator('.content-layer');
const selectedOutline = (page: Page) => layer(page).locator('.content-outline[data-state="selected"]');
const imageToolbar = (page: Page) => page.getByRole('toolbar', { name: 'Image' });
const saveButton = (page: Page) => page.locator('.workspace__bar').getByRole('button', { name: 'Save', exact: true });
const history = (page: Page, name: 'Undo' | 'Redo') =>
  page.getByRole('toolbar', { name: 'History' }).getByRole('button', { name, exact: true });
const IMAGE_SELECTED = 'Image selected on page 1.';
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

/** Selects the image under (x, y) once the page is read, and waits for the status to say so. */
async function selectImage(page: Page, x = DRAWN.left + 50, y = DRAWN.top + 50): Promise<void> {
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, x, y);
  await expect(status(page)).toHaveText(IMAGE_SELECTED, { timeout: 30_000 });
  await expect(selectedOutline(page)).toHaveAttribute('data-kind', 'image');
}

/** The selected image's box in points on page 1 as drawn. */
async function selectedBox(page: Page): Promise<{ left: number; top: number; width: number; height: number } | null> {
  const slot = await pageSlot(page, 1).boundingBox();
  // Between an edit landing and the image being found again there is no outline.
  const box = await selectedOutline(page).boundingBox({ timeout: 1_000 }).catch(() => null);
  if (!slot || !box) return null;
  return { left: (box.x - slot.x) / SCALE, top: (box.y - slot.y) / SCALE, width: box.width / SCALE, height: box.height / SCALE };
}

/** Waits until the selected image's box is `expected` (to half a point). */
async function expectSelectedAt(page: Page, expected: { left: number; top: number; width: number; height: number }): Promise<void> {
  await expect(status(page)).toHaveText(IMAGE_SELECTED, { timeout: 30_000 });
  await expect
    .poll(async () => {
      const box = await selectedBox(page);
      if (!box) return null;
      return [box.left, box.top, box.width, box.height].map((value, at) => Math.abs(value - Object.values(expected)[at]!) < 0.75);
    }, { timeout: 30_000 })
    .toEqual([true, true, true, true]);
}

async function savedBytes(page: Page): Promise<Buffer> {
  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  return readFileSync(await download.path());
}

async function reopen(page: Page, name: string, bytes: Buffer): Promise<void> {
  await openBytes(page, name, bytes);
  await enterMode(page);
}

/** The `/Width` of every image XObject dictionary the saved bytes hold. */
function imageWidths(bytes: Buffer): number[] {
  return subtypeDictionaries(bytes, 'Image').map((dictionary) => Number(/\/Width\s+(\d+)/.exec(dictionary)?.[1]));
}

test('an image moves by the arrow keys and by a drag of its body, one step, saved where it was left', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'move-image.pdf', buildPdf([PHOTO]));
  await enterMode(page);
  await selectImage(page);
  await expect(imageToolbar(page)).toBeVisible();

  // Two Shift+Down: 20 points.
  await page.keyboard.press('Shift+ArrowDown');
  await page.keyboard.press('Shift+ArrowDown');
  await expectSelectedAt(page, { ...DRAWN, top: DRAWN.top + 20 });
  await expect(saveButton(page)).toBeEnabled();

  // Dragged 60 x 30 px further, by its body.
  const body = (await selectedOutline(page).boundingBox())!;
  await page.mouse.move(body.x + 40, body.y + 40);
  await page.mouse.down();
  await page.mouse.move(body.x + 70, body.y + 55, { steps: 4 });
  await page.mouse.move(body.x + 100, body.y + 70, { steps: 4 });
  await page.mouse.up();
  const moved = { ...DRAWN, left: DRAWN.left + 60 / SCALE, top: DRAWN.top + 20 + 30 / SCALE };
  await expectSelectedAt(page, moved);

  // The moves of one image are one step.
  await history(page, 'Undo').click();
  await selectImage(page);
  await expectSelectedAt(page, DRAWN);
  await history(page, 'Redo').click();
  await selectImage(page, moved.left + 50, moved.top + 50);
  await expectSelectedAt(page, moved);

  const bytes = await savedBytes(page);
  await reopen(page, 'moved-image.pdf', bytes);
  await selectImage(page, moved.left + 50, moved.top + 50);
  await expectSelectedAt(page, moved);
  // Nothing is left where it was.
  await clickAt(page, DRAWN.left + 5, DRAWN.top + 5);
  await expect(status(page)).toHaveText(HINT);
});

test('a corner handle resizes in proportion, Shift frees it, and the saved file paints the new box', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'resize-image.pdf', buildPdf([PHOTO]));
  await enterMode(page);
  await selectImage(page);

  const corner = page.getByRole('button', { name: 'Resize from the bottom-right corner' });
  let handle = (await corner.boundingBox())!;
  await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2);
  await page.mouse.down();
  await page.mouse.move(handle.x + handle.width / 2 + 50, handle.y + handle.height / 2 + 5, { steps: 4 });
  await page.mouse.move(handle.x + handle.width / 2 + 100 * SCALE, handle.y + handle.height / 2 + 10, { steps: 4 });
  await expect(page.locator('.image-edit__preview')).toBeVisible();
  await page.mouse.up();
  // 100 pt wider: 300 x 150, the top-left corner kept.
  const proportional = { ...DRAWN, width: 300, height: 150 };
  await expectSelectedAt(page, proportional);

  // With Shift held, the same handle changes only what it is dragged by.
  handle = (await corner.boundingBox())!;
  await page.keyboard.down('Shift');
  await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2);
  await page.mouse.down();
  await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2 + 15 * SCALE, { steps: 4 });
  await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2 + 30 * SCALE, { steps: 4 });
  await page.mouse.up();
  await page.keyboard.up('Shift');
  const free = { ...proportional, height: 180 };
  await expectSelectedAt(page, free);

  // A resize folds into the image's geometry step: one undo puts it back.
  await history(page, 'Undo').click();
  await selectImage(page);
  await expectSelectedAt(page, DRAWN);
  await history(page, 'Redo').click();
  await selectImage(page);
  await expectSelectedAt(page, free);

  const bytes = await savedBytes(page);
  await reopen(page, 'resized-image.pdf', bytes);
  await selectImage(page);
  await expectSelectedAt(page, free);
});

test('Delete removes the selected image; undo and redo, and the saved file paints no image there', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'delete-image.pdf', buildPdf([PHOTO]));
  await enterMode(page);
  await selectImage(page);
  await page.keyboard.press('Delete');
  await expect(status(page)).toHaveText(HINT, { timeout: 30_000 });
  await expect(selectedOutline(page)).toHaveCount(0);
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, DRAWN.left + 50, DRAWN.top + 50);
  await expect(status(page)).toHaveText(HINT);

  await history(page, 'Undo').click();
  await selectImage(page);
  // The toolbar's Delete does the same.
  await imageToolbar(page).getByRole('button', { name: 'Delete image' }).click();
  await expect(status(page)).toHaveText(HINT, { timeout: 30_000 });
  await history(page, 'Undo').click();
  await selectImage(page);
  await history(page, 'Redo').click();
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, DRAWN.left + 50, DRAWN.top + 50);
  await expect(status(page)).toHaveText(HINT);

  const bytes = await savedBytes(page);
  await reopen(page, 'deleted-image.pdf', bytes);
  await clickAt(page, DRAWN.left + 50, DRAWN.top + 50);
  await expect(status(page)).toHaveText(HINT);
});

test('Replace asks the core, then the file picker, and the saved file holds the new picture in the same box', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'replace-image.pdf', buildPdf([PHOTO]));
  await enterMode(page);
  await selectImage(page);
  const replace = imageToolbar(page).getByRole('button', { name: 'Replace image' });
  await expect(replace).not.toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), replace.click()]);
  await chooser.setFiles({ name: 'green.png', mimeType: 'image/png', buffer: buildPng(30, 10, [20, 200, 40]) });
  await expect(history(page, 'Undo')).not.toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  // Found again where it was, the same size.
  await expectSelectedAt(page, DRAWN);

  await history(page, 'Undo').click();
  await selectImage(page);
  await history(page, 'Redo').click();
  await selectImage(page);

  const bytes = await savedBytes(page);
  expect(imageWidths(bytes)).toContain(30);
  await reopen(page, 'replaced-image.pdf', bytes);
  await selectImage(page);
  await expectSelectedAt(page, DRAWN);
});

test('Insert image picks a file, arms a click on the page, and the core sizes what it places there', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'insert-image.pdf', buildPdf([PHOTO]));
  await enterMode(page);
  const insert = mode(page).getByRole('button', { name: 'Insert image' });

  // Escape disarms: nothing is added.
  let [chooser] = await Promise.all([page.waitForEvent('filechooser'), insert.click()]);
  await chooser.setFiles({ name: 'wide.png', mimeType: 'image/png', buffer: buildPng(200, 100, [220, 30, 30]) });
  await expect(insert).toHaveAttribute('aria-pressed', 'true');
  await expect(status(page)).toHaveText('Click the page where the image goes. Press Escape to cancel.');
  await page.getByLabel('Pages', { exact: true }).focus();
  await page.keyboard.press('Escape');
  await expect(insert).toHaveAttribute('aria-pressed', 'false');
  await expect(status(page)).toHaveText(HINT);

  [chooser] = await Promise.all([page.waitForEvent('filechooser'), insert.click()]);
  await chooser.setFiles({ name: 'wide.png', mimeType: 'image/png', buffer: buildPng(200, 100, [220, 30, 30]) });
  await clickAt(page, 350, 280);
  // Its own 2:1, the longest side two inches, its top-left corner where the page was clicked.
  const inserted = { left: 350, top: 280, width: 144, height: 72 };
  await expectSelectedAt(page, inserted);
  await expect(insert).toHaveAttribute('aria-pressed', 'false');

  // Not saved yet, it can be moved like any other image.
  await page.keyboard.press('Shift+ArrowRight');
  const nudged = { ...inserted, left: 360 };
  await expectSelectedAt(page, nudged);

  await history(page, 'Undo').click();
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await history(page, 'Undo').click();
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  await clickAt(page, 380, 310);
  await expect(status(page)).toHaveText(HINT);
  await history(page, 'Redo').click();
  await history(page, 'Redo').click();
  await selectImage(page, 400, 310);
  await expectSelectedAt(page, nudged);

  const bytes = await savedBytes(page);
  expect(imageWidths(bytes)).toEqual(expect.arrayContaining([2, 200]));
  await reopen(page, 'inserted-image.pdf', bytes);
  await selectImage(page, 400, 310);
  await expectSelectedAt(page, nudged);
  await selectImage(page);
  await expectSelectedAt(page, DRAWN);
});

test('a document that forbids modification offers no image edit at all', async ({ page }) => {
  const locked = buildPdf([PHOTO], { encrypt: { permissions: MODIFY_PROHIBITED, userPassword: 'user-e2e' } });
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
  await expect(page.getByRole('button', { name: 'Insert image' })).toHaveCount(0);
  await expect(imageToolbar(page)).toHaveCount(0);
  await page.getByLabel('Pages', { exact: true }).focus();
  for (const key of ['Delete', 'ArrowRight']) await page.keyboard.press(key);
  await expect(history(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');
});
