// Organize: Add PDFs (picker and drop) and the Documents view — import a PDF,
// move its block, save, and reopen the download to check the page order and
// count the core wrote; and a locked PDF asks for its own password.
//
// Pages are known by their shape (see organize.ts): the opened document's
// pages are 300 and 400 pt wide, an added one's 500 and 600.

import { expect, test, type Page } from '@playwright/test';
import {
  cards,
  drag,
  expectThumbnailsRendered,
  grid,
  openOrganize,
  saveAsDownload,
  shapes,
  SHAPE_HEIGHT,
  tool,
  toolbar,
} from './organize';
import { buildPdf } from './pdf';
import { loadApp, openBytes } from './support';

const shapesPdf = (widths: readonly number[], encrypt?: { permissions: number; userPassword: string }) =>
  buildPdf(
    widths.map((widthPt) => ({ widthPt, heightPt: SHAPE_HEIGHT })),
    encrypt ? { encrypt } : {},
  );

const BASE = shapesPdf([300, 400]);
const ADDED = shapesPdf([500, 600]);
/** Every permission granted (`/P` -4), so its pages may be copied out once it is unlocked. */
const LOCKED = shapesPdf([500, 600], { permissions: -4, userPassword: 'add-pass' });

const addPdfs = (page: Page) => page.getByLabel('Add PDFs');
const pdfFile = (name: string, buffer: Buffer) => ({ name, mimeType: 'application/pdf', buffer });
const blockLabels = (page: Page) => cards(page).evaluateAll((nodes) => nodes.map((node) => node.getAttribute('aria-label')));

test('add a PDF, move its block to the front, save: the PDF holds every page in the new order', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'base.pdf', BASE);
  await openOrganize(page);

  await addPdfs(page).setInputFiles(pdfFile('added.pdf', ADDED));
  await expect(page.getByText('Added 2 pages from added.pdf.')).toBeVisible({ timeout: 30_000 });
  await expect(cards(page)).toHaveCount(4);
  await expectThumbnailsRendered(page);
  expect(await shapes(page)).toEqual(['300', '400', '500', '600']);
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  await expect(page.getByText('base.pdf — 4 pages')).toBeVisible();

  // The whole pick is one undo step.
  await tool(page, 'Undo').click();
  await expect(cards(page)).toHaveCount(2);
  await tool(page, 'Redo').click();
  await expect(cards(page)).toHaveCount(4);

  await toolbar(page).getByRole('button', { name: 'Documents', exact: true }).click();
  await expect(grid(page)).toHaveAccessibleName('Documents in base.pdf');
  await expectThumbnailsRendered(page);
  expect(await blockLabels(page)).toEqual(['base.pdf, 2 pages', 'added.pdf, 2 pages']);

  // The added block dropped left of the first block's middle: in front of it.
  await drag(page, cards(page).nth(1), cards(page).nth(0), -40);
  await expect.poll(() => blockLabels(page)).toEqual(['added.pdf, 2 pages', 'base.pdf, 2 pages']);

  await toolbar(page).getByRole('button', { name: 'Pages', exact: true }).click();
  await expectThumbnailsRendered(page);
  expect(await shapes(page)).toEqual(['500', '600', '300', '400']);

  const { bytes } = await saveAsDownload(page);
  await openBytes(page, 'reopened.pdf', bytes);
  await expect(page.getByText('reopened.pdf — 4 pages')).toBeVisible();
  await openOrganize(page);
  expect(await shapes(page)).toEqual(['500', '600', '300', '400']);
  // Saved as one PDF: the Documents view has a single block now.
  await toolbar(page).getByRole('button', { name: 'Documents', exact: true }).click();
  await expect(cards(page)).toHaveCount(1);
  expect(await blockLabels(page)).toEqual(['reopened.pdf, 4 pages']);
});

test('adding a locked PDF asks for its password, says when it is wrong, and adds it with the right one', async ({
  page,
}) => {
  await loadApp(page);
  await openBytes(page, 'base.pdf', BASE);
  await openOrganize(page);

  await addPdfs(page).setInputFiles(pdfFile('locked.pdf', LOCKED));
  const dialog = page.getByRole('dialog', { name: 'Password required' });
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  await expect(dialog.getByText('locked.pdf is protected. Enter its password to add its pages.')).toBeVisible();
  await dialog.getByLabel('Password').fill('nope');
  await dialog.getByRole('button', { name: 'Add' }).click();
  await expect(dialog.getByText('Wrong password. Try again.')).toBeVisible({ timeout: 30_000 });
  await expect(cards(page)).toHaveCount(2);

  await dialog.getByLabel('Password').fill('add-pass');
  await dialog.getByLabel('Password').press('Enter');
  await expect(dialog).toHaveCount(0, { timeout: 30_000 });
  await expect(cards(page)).toHaveCount(4);
  await expectThumbnailsRendered(page);
  expect(await shapes(page)).toEqual(['300', '400', '500', '600']);

  // The saved document is the opened one's (unencrypted) with the unlocked pages in it.
  const { bytes } = await saveAsDownload(page);
  await openBytes(page, 'reopened.pdf', bytes);
  await openOrganize(page);
  expect(await shapes(page)).toEqual(['300', '400', '500', '600']);
});

test('cancelling the password of a locked PDF adds nothing', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'base.pdf', BASE);
  await openOrganize(page);

  await addPdfs(page).setInputFiles([pdfFile('added.pdf', ADDED), pdfFile('locked.pdf', LOCKED)]);
  const dialog = page.getByRole('dialog', { name: 'Password required' });
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(dialog).toHaveCount(0);
  // The whole pick is abandoned, the unlocked file too.
  await expect(cards(page)).toHaveCount(2);
  await expect(tool(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');
});

test('a PDF dropped on Organize is added to the document, not opened in its place', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'base.pdf', BASE);
  await openOrganize(page);

  await page.evaluate(
    ({ name, bytes }) => {
      const transfer = new DataTransfer();
      transfer.items.add(new File([new Uint8Array(bytes)], name, { type: 'application/pdf' }));
      const target = document.querySelector('.organize__grid')!;
      for (const type of ['dragenter', 'dragover', 'drop']) {
        target.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: transfer }));
      }
    },
    { name: 'dropped.pdf', bytes: [...ADDED] },
  );
  await expect(page.getByText('Added 2 pages from dropped.pdf.')).toBeVisible({ timeout: 30_000 });
  await expect(cards(page)).toHaveCount(4);
  await expect(page.getByText('base.pdf — 4 pages')).toBeVisible();
});
