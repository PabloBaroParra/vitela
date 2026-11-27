// Export images (T-252): the dialog, the real core's page grammar, names,
// raster ceiling and encoder, and each page as its own download — parity
// with the native Export images. The pages are drawn from what a save would
// write now, so a pending page edit is in the image.

import { readFileSync } from 'node:fs';
import { expect, test, type Download, type Locator, type Page } from '@playwright/test';
import { cards, openOrganize, tool } from './organize';
import { loadApp, openFixture, openTool, toolsButton } from './support';

// Three US Letter pages (612 x 792 pt).
const THREE_PAGES = 'compress/vector_only.pdf';

const exportDialog = (page: Page) => page.getByRole('dialog', { name: 'Export pages as images' });

async function openDialog(page: Page): Promise<Locator> {
  await openTool(page, 'Export images');
  const dialog = exportDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  return dialog;
}

/** Clicks Export and collects the `count` downloads it starts, in order. */
async function exportDownloads(page: Page, dialog: Locator, count: number): Promise<{ name: string; bytes: Buffer }[]> {
  const downloads: Download[] = [];
  const all = new Promise<void>((resolve) => {
    page.on('download', (download) => {
      downloads.push(download);
      if (downloads.length === count) resolve();
    });
  });
  await dialog.getByRole('button', { name: 'Export', exact: true }).click();
  await all;
  return Promise.all(downloads.map(async (download) => ({ name: download.suggestedFilename(), bytes: readFileSync(await download.path()) })));
}

/** A PNG's width and height, after its signature. */
function pngSize(bytes: Buffer): [number, number] {
  expect([...bytes.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
}

/** A JPEG's width and height from its first start-of-frame marker, after its signature. */
function jpegSize(bytes: Buffer): [number, number] {
  expect([...bytes.subarray(0, 3)]).toEqual([0xff, 0xd8, 0xff]);
  let at = 2;
  while (at < bytes.length) {
    const marker = bytes[at + 1]!;
    const length = bytes.readUInt16BE(at + 2);
    // SOF0..SOF15, except DHT (C4), JPG (C8) and DAC (CC).
    if (marker >= 0xc0 && marker <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(marker)) {
      return [bytes.readUInt16BE(at + 7), bytes.readUInt16BE(at + 5)];
    }
    at += 2 + length;
  }
  throw new Error('no start-of-frame marker');
}

test('the current page exports as one PNG, sized by the DPI and named by the core', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, THREE_PAGES);
  await page.getByLabel('Pages', { exact: true }).evaluate((element) => element.scrollTo({ top: element.scrollHeight }));
  await expect(page.getByText('Page 3 of 3')).toBeVisible();

  const dialog = await openDialog(page);
  await expect(dialog.getByRole('radio', { name: 'All pages' })).toBeChecked();
  await expect(dialog.getByRole('radio', { name: 'PNG' })).toBeChecked();
  await dialog.getByRole('radio', { name: 'Current page' }).check();
  await dialog.getByLabel('Resolution (72-400 DPI)').fill('144');

  const [image] = await exportDownloads(page, dialog, 1);
  expect(image!.name).toBe('vector_only-3.png');
  expect(pngSize(image!.bytes)).toEqual([1224, 1584]);
  await expect(exportDialog(page)).toHaveCount(0);
  await expect(page.getByText('Exported 1 page as PNG.')).toBeVisible();
  await expect(toolsButton(page)).toBeFocused();
});

test('a range exports as one JPEG per page', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, THREE_PAGES);

  const dialog = await openDialog(page);
  // Typing the range picks "Custom range".
  await dialog.getByRole('textbox', { name: 'Page range' }).fill('2-3');
  await expect(dialog.getByRole('radio', { name: 'Custom range' })).toBeChecked();
  await dialog.getByRole('radio', { name: 'JPEG' }).check();
  await dialog.getByLabel('Resolution (72-400 DPI)').fill('72');

  const images = await exportDownloads(page, dialog, 2);
  expect(images.map(({ name }) => name)).toEqual(['vector_only-2.jpg', 'vector_only-3.jpg']);
  for (const { bytes } of images) expect(jpegSize(bytes)).toEqual([612, 792]);
  await expect(page.getByText('Exported 2 pages as JPEG.')).toBeVisible();
});

test('a pending page edit is in the image, and the edit stays unsaved', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, THREE_PAGES);
  await openOrganize(page);
  await cards(page).nth(0).click();
  await tool(page, 'Rotate right').click();
  await expect(page.getByText('Unsaved changes.')).toBeVisible();

  const dialog = await openDialog(page);
  await dialog.getByRole('textbox', { name: 'Page range' }).fill('1');
  await dialog.getByLabel('Resolution (72-400 DPI)').fill('72');
  const [image] = await exportDownloads(page, dialog, 1);
  expect(image!.name).toBe('vector_only-1.png');
  expect(pngSize(image!.bytes)).toEqual([792, 612]);
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  await expect(tool(page, 'Undo')).toHaveAttribute('aria-disabled', 'false');
});

test('a range the core cannot read and a resolution out of range are refused in the dialog, and nothing downloads', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, THREE_PAGES);
  let downloads = 0;
  page.on('download', () => downloads++);

  const dialog = await openDialog(page);
  await dialog.getByRole('textbox', { name: 'Page range' }).fill('2-9');
  await dialog.getByRole('button', { name: 'Export', exact: true }).click();
  await expect(dialog.getByRole('alert')).toHaveText('This document has 3 pages, so page 9 does not exist.');
  await expect(dialog).toBeVisible();

  await dialog.getByRole('textbox', { name: 'Page range' }).fill('1-3');
  await expect(dialog.getByRole('alert')).toHaveCount(0);
  await dialog.getByLabel('Resolution (72-400 DPI)').fill('600');
  await dialog.getByRole('button', { name: 'Export', exact: true }).click();
  await expect(dialog.getByRole('alert')).toHaveText('Choose a resolution between 72 and 400 DPI.');

  await page.keyboard.press('Escape');
  await expect(exportDialog(page)).toHaveCount(0);
  await expect(toolsButton(page)).toBeFocused();
  expect(downloads).toBe(0);
});
