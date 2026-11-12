import { expect, test } from '@playwright/test';
import { inkedPixelCount, loadApp, openFixture } from './support';

test('opens a repo fixture and paints its first page', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));

  await loadApp(page);
  await openFixture(page, 'forms/reportlab_acroform.pdf');

  await expect(page.getByText('reportlab_acroform.pdf — 1 page')).toBeVisible();
  const canvas = await inkedPixelCount(page);
  // US Letter (8.5 x 11 in) at the viewer's 144 dpi.
  expect(canvas.width).toBe(1224);
  expect(canvas.height).toBe(1584);
  // Blank-but-painted would pass a size check; demand real ink.
  expect(canvas.inked).toBeGreaterThan(5_000);
  expect(errors).toEqual([]);
});

test('reports a file that is not a PDF instead of painting', async ({ page }) => {
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles({
    name: 'not-a.pdf',
    mimeType: 'application/pdf',
    buffer: Buffer.from('this is not a PDF'),
  });
  await expect(page.getByRole('alert')).toContainText('Could not open not-a.pdf');
  await expect(page.locator('canvas.page-canvas')).toHaveCount(0);
});
