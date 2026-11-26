// Add watermark (T-256): the same text or picture stamped on the pages the
// reader picks, as one undoable edit through the real core, painted into
// each page's content stream by Save — parity with the native Add
// watermark. The look starts from the core's; a document whose permissions
// withhold modification refuses, the gate every native shell asks.

import { readFileSync } from 'node:fs';
import { expect, test, type Locator, type Page } from '@playwright/test';
import { buildPdf, MODIFY_PROHIBITED } from './pdf';
import { occurrences, subtypeDictionaries } from './pdfObjects';
import { cards, openOrganize } from './organize';
import { buildPng } from './png';
import { expectOpened, inkedPixelCount, loadApp, openBytes, openTool, toolsButton } from './support';

const LETTER = { widthPt: 612, heightPt: 792 };
const STAMP = 'VITELASTAMP';
/** Every stamp is a marked-content artifact of this subtype, once per page it is on. */
const ARTIFACT = '/Subtype /Watermark';
const watermarkDialog = (page: Page) => page.getByRole('dialog', { name: 'Add watermark' });
const saveButton = (page: Page) => page.locator('.workspace__bar').getByRole('button', { name: 'Save', exact: true });
const undo = (page: Page) => page.getByRole('button', { name: 'Undo', exact: true });
const redo = (page: Page) => page.getByRole('button', { name: 'Redo', exact: true });

async function openDialog(page: Page): Promise<Locator> {
  await openTool(page, 'Watermark');
  const dialog = watermarkDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  return dialog;
}

async function savedBytes(page: Page): Promise<Buffer> {
  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  return readFileSync(await download.path());
}

test('stamps a text watermark on every page as one undoable edit, and Save writes it into each page', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER, LETTER, LETTER]));
  const before = (await inkedPixelCount(page, 1)).inked;

  const dialog = await openDialog(page);
  // The core's look: diagonal, faded, over the content, on every page.
  await expect(dialog.getByRole('radio', { name: 'Text' })).toBeFocused();
  await expect(dialog.getByLabel('Watermark text')).toHaveValue('DRAFT');
  await expect(dialog.getByText('Angle: 45°')).toBeVisible();
  await expect(dialog.getByRole('radio', { name: 'All pages' })).toBeChecked();
  await dialog.getByLabel('Watermark text').fill(STAMP);
  // Fully opaque, so the grey reads as ink on the page.
  await dialog.getByRole('slider', { name: 'Opacity' }).fill('100');
  await expect(dialog.getByText('Opacity: 100%')).toBeVisible();
  await dialog.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(watermarkDialog(page)).toHaveCount(0);
  await expect(page.getByText('Watermark added to 3 pages.')).toBeVisible();
  await expect(toolsButton(page)).toBeFocused();
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  // The preview shows it before any save.
  await expect.poll(async () => (await inkedPixelCount(page, 1)).inked, { timeout: 30_000 }).toBeGreaterThan(before);

  // One step: Undo takes it off every page, Redo puts it back.
  await undo(page).click();
  await expect(redo(page)).toBeEnabled();
  await expect(undo(page)).toBeDisabled();
  await expect.poll(async () => (await inkedPixelCount(page, 1)).inked, { timeout: 30_000 }).toBe(before);
  await redo(page).click();
  await expect(undo(page)).toBeEnabled();

  const bytes = await savedBytes(page);
  expect(occurrences(bytes, `(${STAMP}) Tj`)).toBe(3);
  expect(occurrences(bytes, ARTIFACT)).toBe(3);

  // Read back by the app itself, from the saved file: the text is on the pages.
  await openBytes(page, 'saved.pdf', bytes);
  await page.getByLabel('Find in document').fill(STAMP);
  await page.getByLabel('Find in document').press('Enter');
  await expect(page.getByLabel('Search results')).toHaveText('1 of 3', { timeout: 30_000 });
});

test('stamps a picture behind the content of a typed range, as one image object', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER, LETTER, LETTER]));
  const dialog = await openDialog(page);
  await dialog.getByRole('radio', { name: 'Picture' }).check();
  // The picture's own defaults: upright, behind the page's content.
  await expect(dialog.getByText('Angle: 0°')).toBeVisible();
  await expect(dialog.getByRole('radio', { name: 'Behind the page content' })).toBeChecked();
  await dialog.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(dialog.getByRole('alert')).toHaveText('Choose a PNG or JPEG picture to stamp on the pages.');

  await dialog
    .getByLabel('Choose picture…')
    .setInputFiles({ name: 'logo.png', mimeType: 'image/png', buffer: buildPng(40, 20, [200, 30, 30]) });
  await expect(dialog.getByText('logo.png (1 KB).')).toBeVisible();
  await dialog.getByLabel('Page range').fill('1,3');
  await dialog.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByText('Watermark added to 2 pages.')).toBeVisible({ timeout: 30_000 });

  const bytes = await savedBytes(page);
  // One image object, painted on the two pages named.
  expect(subtypeDictionaries(bytes, 'Image')).toHaveLength(1);
  expect(occurrences(bytes, ARTIFACT)).toBe(2);
});

test('keeps the dialog open for text the watermark font cannot show, and for a bad range', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER]));
  const dialog = await openDialog(page);
  await dialog.getByLabel('Watermark text').fill('日本');
  await dialog.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('Use Latin letters, digits and punctuation.');
  await dialog.getByLabel('Watermark text').fill('COPY');
  await dialog.getByLabel('Page range').fill('9');
  await dialog.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await expect(dialog.getByLabel('Watermark text')).toHaveValue('COPY');
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(watermarkDialog(page)).toHaveCount(0);
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
  await expect(undo(page)).toBeDisabled();
});

test('a document whose permissions forbid modification refuses a watermark', async ({ page }) => {
  const locked = buildPdf([LETTER], { encrypt: { permissions: MODIFY_PROHIBITED, userPassword: 'user-e2e' } });
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: locked });
  const prompt = page.getByRole('dialog', { name: 'Password required' });
  await expect(prompt).toBeVisible({ timeout: 30_000 });
  await prompt.getByLabel('Password').fill('user-e2e');
  await prompt.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');

  const dialog = await openDialog(page);
  await expect(dialog.getByRole('alert')).toHaveText('Content editing is not permitted.');
  await expect(dialog.getByLabel('Watermark text')).toBeDisabled();
  await expect(dialog.getByRole('button', { name: 'Add', exact: true })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Close' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(watermarkDialog(page)).toHaveCount(0);
  await expect(toolsButton(page)).toBeFocused();
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
});

test('in Organize the thumbnails show the stamp, and Undo takes it off them', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER, LETTER]));
  await openOrganize(page);
  /** The ink on the first card's thumbnail. */
  const ink = () =>
    cards(page)
      .first()
      .locator('canvas')
      .evaluate((node) => {
        const canvas = node as HTMLCanvasElement;
        const { data } = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height);
        let inked = 0;
        for (let i = 0; i < data.length; i += 4) if (data[i]! < 200) inked++;
        return inked;
      });
  const before = await ink();

  const dialog = await openDialog(page);
  await dialog.getByRole('slider', { name: 'Opacity' }).fill('100');
  await dialog.getByRole('slider', { name: 'Size' }).fill('200');
  await dialog.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByText('Watermark added to 2 pages.')).toBeVisible({ timeout: 30_000 });
  await expect.poll(ink, { timeout: 30_000 }).toBeGreaterThan(before);
  await undo(page).click();
  await expect.poll(ink, { timeout: 30_000 }).toBe(before);
});

test('on a phone, Tools opens Add watermark, which fits the screen', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 600 });
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER]));
  const dialog = await openDialog(page);
  const box = (await dialog.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(390);
  await dialog.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('Watermark added to 1 page.')).toBeAttached();
  await expect(toolsButton(page)).toBeFocused();
});
