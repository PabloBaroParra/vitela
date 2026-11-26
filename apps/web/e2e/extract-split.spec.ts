// Extract pages and Split (T-254): Organize's two buttons, their dialogs,
// the real core's page grammar, cut, split plan and part names, and each
// file as its own download — parity with the native Extract pages and
// Split. Pages are told apart by their width (organize.ts), so a downloaded
// PDF's /MediaBox list says which pages it holds.

import { readFileSync } from 'node:fs';
import { expect, test, type Download, type Locator, type Page } from '@playwright/test';
import { cards, openOrganize, tool, SHAPE_HEIGHT } from './organize';
import { buildPdf, COPY_PROHIBITED } from './pdf';
import { pageMediaBoxes } from './pdfObjects';
import { expectOpened, loadApp, openBytes } from './support';

const WIDTHS = [300, 400, 500, 600, 700];
const FIVE_PAGES = buildPdf(WIDTHS.map((widthPt) => ({ widthPt, heightPt: SHAPE_HEIGHT })));

const extractDialog = (page: Page) => page.getByRole('dialog', { name: 'Extract pages' });
const splitDialog = (page: Page) => page.getByRole('dialog', { name: 'Split PDF' });

/** The page widths a PDF's pages have, in file order. */
const widths = (bytes: Buffer) => pageMediaBoxes(bytes).map(([x0, , x1]) => x1! - x0!);

/** Clicks `confirm` and collects the `count` downloads it starts, in order. */
async function downloadsOf(page: Page, confirm: Locator, count: number): Promise<{ name: string; bytes: Buffer }[]> {
  const downloads: Download[] = [];
  const all = new Promise<void>((resolve) => {
    page.on('download', (download) => {
      downloads.push(download);
      if (downloads.length === count) resolve();
    });
  });
  await confirm.click();
  await all;
  return Promise.all(downloads.map(async (download) => ({ name: download.suggestedFilename(), bytes: readFileSync(await download.path()) })));
}

test('extracts pages 2-3 to a PDF holding exactly those pages, which opens', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'five.pdf', FIVE_PAGES);
  await openOrganize(page);

  await tool(page, 'Extract').click();
  const dialog = extractDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  await expect(dialog.getByText('This document has 5 pages.')).toBeVisible();
  // It opens on the selection (page 1, as opened); a typed range replaces it.
  await expect(dialog.getByLabel('Pages to extract')).toHaveValue('1');
  await dialog.getByLabel('Pages to extract').fill('2-3');
  const [extracted] = await downloadsOf(page, dialog.getByRole('button', { name: 'Extract', exact: true }), 1);

  expect(extracted!.name).toBe('five-extract.pdf');
  expect(extracted!.bytes.subarray(0, 5).toString('latin1')).toBe('%PDF-');
  expect(widths(extracted!.bytes).sort()).toEqual([400, 500]);
  await expect(extractDialog(page)).toHaveCount(0);
  await expect(page.getByText('Downloaded five-extract.pdf: 2 pages extracted to a new PDF.')).toBeVisible();
  await expect(tool(page, 'Extract')).toBeFocused();

  await openBytes(page, 'five-extract.pdf', extracted!.bytes);
  await expect(page.getByText('five-extract.pdf — 2 pages')).toBeVisible();
});

test("Organize's Extract fills the dialog with the selection, and the pending edits are in the PDF", async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'five.pdf', FIVE_PAGES);
  await openOrganize(page);
  // Delete page 1 (300), unsaved: page 2-3 are now 500 and 600.
  await cards(page).nth(0).click();
  await tool(page, 'Delete').click();
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  await cards(page).nth(1).click();
  await cards(page).nth(2).click({ modifiers: ['Shift'] });

  await tool(page, 'Extract').click();
  const dialog = extractDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  await expect(dialog.getByLabel('Pages to extract')).toHaveValue('2-3');
  const [extracted] = await downloadsOf(page, dialog.getByRole('button', { name: 'Extract', exact: true }), 1);
  expect(widths(extracted!.bytes).sort()).toEqual([500, 600]);
  // Focus goes back to what opened the dialog; the deletion is still unsaved and undoable.
  await expect(tool(page, 'Extract')).toBeFocused();
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  await expect(tool(page, 'Undo')).toHaveAttribute('aria-disabled', 'false');
});

test('splits a five-page document after pages 2 and 4 into three PDFs named by the core', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'five.pdf', FIVE_PAGES);
  await openOrganize(page);

  await tool(page, 'Split').click();
  const dialog = splitDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  await expect(dialog.getByText('This document has 5 pages. Each cut starts a new file.')).toBeVisible();
  await dialog.getByLabel('Split after page').fill('2,4');
  const parts = await downloadsOf(page, dialog.getByRole('button', { name: 'Split', exact: true }), 3);

  expect(parts.map(({ name }) => name)).toEqual(['five-part1.pdf', 'five-part2.pdf', 'five-part3.pdf']);
  expect(parts.map(({ bytes }) => widths(bytes).sort())).toEqual([[300, 400], [500, 600], [700]]);
  await expect(page.getByText('Split into 3 PDFs.')).toBeVisible();
  await expect(tool(page, 'Split')).toBeFocused();
});

test('a cut the core refuses stays in the dialog, and nothing downloads', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'five.pdf', FIVE_PAGES);
  await openOrganize(page);
  let downloads = 0;
  page.on('download', () => downloads++);

  await tool(page, 'Split').click();
  const dialog = splitDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  await dialog.getByLabel('Split after page').fill('5');
  await dialog.getByRole('button', { name: 'Split', exact: true }).click();
  await expect(dialog.getByRole('alert')).toHaveText('There is nothing after page 5, so the document cannot be split there.');
  await expect(dialog.getByLabel('Split after page')).toHaveValue('5');
  await page.keyboard.press('Escape');
  await expect(splitDialog(page)).toHaveCount(0);
  await expect(tool(page, 'Split')).toBeFocused();
  expect(downloads).toBe(0);
});

test('a document that forbids extraction refuses both in the dialog, and nothing downloads', async ({ page }) => {
  const locked = buildPdf(
    WIDTHS.slice(0, 2).map((widthPt) => ({ widthPt, heightPt: SHAPE_HEIGHT })),
    { encrypt: { permissions: COPY_PROHIBITED, userPassword: 'user-e2e' } },
  );
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: locked });
  const prompt = page.getByRole('dialog', { name: 'Password required' });
  await expect(prompt).toBeVisible({ timeout: 30_000 });
  await prompt.getByLabel('Password').fill('user-e2e');
  await prompt.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');
  await openOrganize(page);
  let downloads = 0;
  page.on('download', () => downloads++);

  for (const [button, dialog, confirm] of [
    ['Extract', extractDialog(page), 'Extract'],
    ['Split', splitDialog(page), 'Split'],
  ] as const) {
    await tool(page, button).click();
    await expect(dialog).toBeVisible({ timeout: 30_000 });
    await expect(dialog.getByRole('alert')).toHaveText('This document does not permit extracting its pages.');
    await expect(dialog.getByRole('button', { name: confirm, exact: true })).toBeDisabled();
    await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(tool(page, button)).toBeFocused();
  }
  expect(downloads).toBe(0);
});

test('on a phone the Split dialog fits the screen, and the focus comes back to Split', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 600 });
  await loadApp(page);
  await openBytes(page, 'five.pdf', FIVE_PAGES);
  await openOrganize(page);
  await tool(page, 'Split').click();
  const dialog = splitDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  const box = (await dialog.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(390);
  await expect(dialog.getByLabel('Split after page')).toBeFocused();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(tool(page, 'Split')).toBeFocused();
});
