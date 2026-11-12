// The viewer: virtualized scrolling, rotation, and zoom (buttons, fit width,
// Ctrl+wheel anchored at the pointer).

import { expect, test, type Page } from '@playwright/test';
import { buildPdf, type PageSpec } from './pdf';
import { expectPagePainted, inkedPixelCount, loadApp, openBytes, openFixture, pageSlot } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792 };

const scroller = (page: Page) => page.getByLabel('Pages', { exact: true });
const zoomLevel = (page: Page) => page.getByLabel('Zoom level');

async function box(page: Page, number: number) {
  const rect = await pageSlot(page, number).boundingBox();
  if (!rect) throw new Error(`page ${number} has no box`);
  return rect;
}

test('scrolls a long document to its last page, mounting only pages near the viewport', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'sixty.pdf', buildPdf(Array(60).fill(LETTER)));
  await expect(page.getByText('Page 1 of 60')).toBeVisible();

  // Virtualized: nowhere near 60 canvases exist.
  expect(await page.locator('.page-slot').count()).toBeLessThanOrEqual(5);

  await scroller(page).evaluate((element) => element.scrollTo({ top: element.scrollHeight }));
  await expectPagePainted(page, 60);
  await expect(page.getByText('Page 60 of 60')).toBeVisible();
  expect((await inkedPixelCount(page, 60)).inked).toBeGreaterThan(1_000);
  // Far-away pages were dropped, canvases and all.
  await expect(pageSlot(page, 1)).toHaveCount(0);
  expect(await page.locator('.page-slot').count()).toBeLessThanOrEqual(5);
});

test('lays out and renders a rotated page sideways', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'rotated.pdf', buildPdf([LETTER, { ...LETTER, rotate: 90 }]));
  await scroller(page).evaluate((element) => element.scrollTo({ top: element.scrollHeight }));
  await expectPagePainted(page, 2);

  const slot = await box(page, 2);
  expect(slot.width).toBeGreaterThan(slot.height);
  const canvas = await inkedPixelCount(page, 2);
  expect(canvas.width).toBeGreaterThan(canvas.height);
});

test('zooms with the buttons and fits the page width', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, 'forms/reportlab_acroform.pdf');
  await expect(zoomLevel(page)).toHaveText('100%');
  const before = await box(page, 1);

  await page.getByRole('button', { name: 'Zoom in' }).click();
  await expect(zoomLevel(page)).toHaveText('110%');
  expect((await box(page, 1)).width).toBeCloseTo(before.width * 1.1, 0);
  // Re-rendered at the new resolution, not just stretched: one device pixel
  // per CSS pixel (WebKit's desktop profile runs at devicePixelRatio 2).
  const ratio = await page.evaluate(() => window.devicePixelRatio);
  const dpi = Math.round(96 * 1.1 * ratio);
  await expect(pageSlot(page, 1)).toHaveAttribute('data-dpi', String(dpi));
  await expectPagePainted(page, 1);
  // 612 pt wide at that DPI; the core rounds a fractional pixel either way.
  expect(Math.abs((await inkedPixelCount(page)).width - (612 * dpi) / 72)).toBeLessThanOrEqual(1);

  await page.getByRole('button', { name: 'Zoom out' }).click();
  await page.getByRole('button', { name: 'Zoom out' }).click();
  await expect(zoomLevel(page)).toHaveText('90%');
  expect((await box(page, 1)).width).toBeCloseTo(before.width * 0.9, 0);

  await page.getByRole('button', { name: 'Fit width' }).click();
  const viewportWidth = await scroller(page).evaluate((element) => element.clientWidth);
  await expect.poll(async () => Math.round((await box(page, 1)).width)).toBe(viewportWidth - 32);
});

test('Ctrl+wheel zooms around the pointer; a plain wheel scrolls', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'three.pdf', buildPdf(Array(3).fill(LETTER)));

  // A point off the page centre, where the zoomed view has scroll room on
  // every side to keep it put (no scroll can go past the content's edges).
  const start = await box(page, 1);
  const pointer = { x: start.x + start.width * 0.55, y: start.y + start.height * 0.4 };
  await page.mouse.move(pointer.x, pointer.y);

  await page.keyboard.down('Control');
  await page.mouse.wheel(0, -300);
  await page.keyboard.up('Control');
  await expect(zoomLevel(page)).not.toHaveText('100%');
  await expect.poll(async () => (await box(page, 1)).width).toBeGreaterThan(start.width * 1.2);

  const zoomed = await box(page, 1);
  // The page point under the pointer did not move.
  expect((pointer.x - zoomed.x) / zoomed.width).toBeCloseTo(0.55, 2);
  expect((pointer.y - zoomed.y) / zoomed.height).toBeCloseTo(0.4, 2);

  const level = await zoomLevel(page).textContent();
  const top = await scroller(page).evaluate((element) => element.scrollTop);
  await page.mouse.wheel(0, 400);
  await expect.poll(() => scroller(page).evaluate((element) => element.scrollTop)).toBeGreaterThan(top);
  await expect(zoomLevel(page)).toHaveText(level!);
});
