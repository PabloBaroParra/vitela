// Add blank page (T-253): an A4 page, portrait or landscape, inserted after
// the selection in Organize as one undo step — then saved, and the saved
// bytes read back: how many pages, which `/MediaBox`, and that the new page
// draws nothing. The refusal of a document that withholds assembly is in
// organize.spec.ts, with the other assembly edits.

import { expect, test, type Page } from '@playwright/test';
import { cards, expectThumbnailsRendered, openOrganize, saveAsDownload, shapes, SHAPE_HEIGHT, tool, toolbar } from './organize';
import { buildPdf } from './pdf';
import { pageMediaBoxes } from './pdfObjects';
import { inkedPixelCount, loadApp, openBytes, pageSlot } from './support';

/** Two portrait pages, 300 and 400 pt wide: told apart from an A4 page (595 × 842, drawn "500") by their shape. */
const TWO = buildPdf([
  { widthPt: 300, heightPt: SHAPE_HEIGHT, lines: ['one'] },
  { widthPt: 400, heightPt: SHAPE_HEIGHT, lines: ['two'] },
]);
const A4 = [0, 0, 595, 842];
const A4_LANDSCAPE = [0, 0, 842, 595];

async function addBlank(page: Page, orientation: 'Portrait A4' | 'Landscape A4'): Promise<void> {
  await tool(page, 'Add blank page').click();
  await page.getByRole('menuitem', { name: orientation }).click();
}

test('add a blank page after page 1, undo and redo it, then save: one more page, A4 and empty', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'two.pdf', TWO);
  await openOrganize(page);
  await expect(toolbar(page).getByText('Page 1 of 2 selected')).toBeVisible();

  await addBlank(page, 'Portrait A4');
  await expect(cards(page)).toHaveCount(3);
  await expectThumbnailsRendered(page);
  expect(await shapes(page)).toEqual(['300', '500', '400']);
  // The new page is selected, and the focus is back on the menu button.
  await expect(toolbar(page).getByText('Page 2 of 3 selected')).toBeVisible();
  await expect(tool(page, 'Add blank page')).toBeFocused();
  await expect(page.getByText('Unsaved changes.')).toBeVisible();

  await tool(page, 'Undo').click();
  await expect(cards(page)).toHaveCount(2);
  await expect.poll(() => shapes(page)).toEqual(['300', '400']);
  await tool(page, 'Redo').click();
  await expect(cards(page)).toHaveCount(3);
  await expectThumbnailsRendered(page);
  expect(await shapes(page)).toEqual(['300', '500', '400']);

  const { bytes } = await saveAsDownload(page);
  const boxes = pageMediaBoxes(bytes);
  expect(boxes).toHaveLength(3);
  expect(boxes.filter((box) => box.join() === A4.join())).toHaveLength(1);

  await openBytes(page, 'reopened.pdf', bytes);
  await expect(page.getByText('reopened.pdf — 3 pages')).toBeVisible();
  await pageSlot(page, 2).scrollIntoViewIfNeeded();
  await expect(pageSlot(page, 2).locator('canvas.page-canvas')).toHaveAttribute('data-painted', 'true', {
    timeout: 30_000,
  });
  const blank = await inkedPixelCount(page, 2);
  expect(blank.inked).toBe(0);
  expect(blank.width / blank.height).toBeCloseTo(595 / 842, 2);
  // The pages around it kept their text.
  expect((await inkedPixelCount(page, 1)).inked).toBeGreaterThan(0);
});

test('in the Documents view, a landscape blank page goes in after the selected block as a block of its own', async ({
  page,
}) => {
  await loadApp(page);
  await openBytes(page, 'two.pdf', TWO);
  await openOrganize(page);
  await toolbar(page).getByRole('button', { name: 'Documents', exact: true }).click();
  await expect(cards(page)).toHaveCount(1);

  await addBlank(page, 'Landscape A4');
  await expect(cards(page)).toHaveCount(2);
  await expect(cards(page).nth(1)).toHaveAttribute('aria-label', 'Blank pages, 1 page');
  await expect(cards(page).nth(1)).toHaveAttribute('aria-selected', 'true');
  await expect(toolbar(page).getByText('Document 2 of 2 selected (1 page)')).toBeVisible();

  const { bytes } = await saveAsDownload(page);
  const boxes = pageMediaBoxes(bytes);
  expect(boxes).toHaveLength(3);
  expect(boxes.filter((box) => box.join() === A4_LANDSCAPE.join())).toHaveLength(1);
  await openBytes(page, 'reopened.pdf', bytes);
  await expect(page.getByText('reopened.pdf — 3 pages')).toBeVisible();
});
