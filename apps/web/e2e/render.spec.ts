import { expect, test } from '@playwright/test';
import { inkedPixelCount, loadApp, openFixture } from './support';

test('opens a repo fixture and paints its first page', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));

  await loadApp(page);
  await openFixture(page, 'forms/reportlab_acroform.pdf');

  await expect(page.getByText('reportlab_acroform.pdf — 1 page')).toBeVisible();
  const canvas = await inkedPixelCount(page);
  expect(canvas.width).toBe(1200);
  // US Letter at 1200 px wide: 1200 * 11 / 8.5.
  expect(canvas.height).toBe(1553);
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
