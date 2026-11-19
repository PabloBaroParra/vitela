// Image stamps: a PNG dropped on a page is stamped where it was dropped, in
// the box the core gives it (longest side two inches, the image's top-left
// corner at the drop point), and drawn with its own image; Image picks one
// and stamps it where the page is clicked. Undo takes a stamp away and redo
// brings it back with its image. Saved, the download carries `/Stamp`
// annotations with an appearance that draws the image, and reopens with
// pdfium painting them. On a page turned 90° the image turns with the page.

import { expect, test, type Locator, type Page } from '@playwright/test';
import { saveAsDownload } from './organize';
import { annotationRects, subtypeDictionaries } from './pdfObjects';
import { buildPdf, type PageSpec } from './pdf';
import { buildPng } from './png';
import { dropFileAt, expectPagePainted, inkedPixelCount, loadApp, openBytes, pageSlot } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792 };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;
/** A 2:1 red image: stamped 144 × 72 points. */
const RED_PNG = { name: 'red.png', mimeType: 'image/png', buffer: buildPng(200, 100, [220, 30, 30]) };
/** A 1:2 blue image: stamped 72 × 144 points. */
const BLUE_PNG = { name: 'blue.png', mimeType: 'image/png', buffer: buildPng(50, 100, [30, 60, 220]) };

const stamps = (page: Page) => page.locator('.annotation-layer [data-kind="stamp"]');
const button = (page: Page, name: string) => page.getByRole('button', { name, exact: true });

async function box(locator: Locator) {
  const rect = await locator.boundingBox();
  if (!rect) throw new Error('no box');
  return rect;
}

/** Viewport coordinates of (`x`, `y`) points on page 1 as drawn. */
async function onPage(page: Page, x: number, y: number) {
  const slot = await box(pageSlot(page, 1));
  return { x: slot.x + x * SCALE, y: slot.y + y * SCALE };
}

/** The drawn box of `mark` in points on page 1. */
async function drawnBox(page: Page, mark: Locator) {
  const slot = await box(pageSlot(page, 1));
  const rect = await box(mark);
  return {
    left: (rect.x - slot.x) / SCALE,
    top: (rect.y - slot.y) / SCALE,
    width: rect.width / SCALE,
    height: rect.height / SCALE,
  };
}

/** `rect` as [x0, y0, x1, y1] is within `tolerance` points of `expected`. */
function expectRect(rect: readonly number[] | undefined, expected: readonly number[], tolerance = 1) {
  expect(rect).toHaveLength(4);
  rect!.forEach((value, index) => expect(Math.abs(value - expected[index]!)).toBeLessThan(tolerance));
}

test('stamps a dropped PNG and a picked one, undoes and redoes them, and saves /Stamp with its appearance', async ({
  page,
}) => {
  await loadApp(page);
  await openBytes(page, 'stamp.pdf', buildPdf([LETTER]));
  await expectPagePainted(page, 1);
  const blank = (await inkedPixelCount(page, 1)).inked;

  // Dropped at (100, 150): its image's top-left corner there, 144 × 72.
  const dropAt = await onPage(page, 100, 150);
  await dropFileAt(page, RED_PNG, dropAt.x, dropAt.y);
  await expect(stamps(page)).toHaveCount(1);
  const image = stamps(page).locator('image');
  await expect(image).toHaveAttribute('href', /^data:image\/png;base64,/);
  const dropped = await drawnBox(page, image);
  expect(Math.abs(dropped.left - 100)).toBeLessThan(1);
  expect(Math.abs(dropped.top - 150)).toBeLessThan(1);
  expect(Math.abs(dropped.width - 144)).toBeLessThan(1);
  expect(Math.abs(dropped.height - 72)).toBeLessThan(1);
  // Stamped, not opened: the document on screen is still this one.
  await expect(page.getByText('stamp.pdf —')).toBeVisible();
  await expect(page.getByText('Unsaved changes.')).toBeVisible();

  // Picked with Image, then placed with a click at (300, 250).
  await page.getByLabel('Stamp image').setInputFiles(BLUE_PNG);
  await expect(button(page, 'Image')).toHaveAttribute('aria-pressed', 'true');
  const clickAt = await onPage(page, 300, 250);
  await page.mouse.click(clickAt.x, clickAt.y);
  await expect(stamps(page)).toHaveCount(2);
  await expect(stamps(page).nth(1).locator('image')).toHaveAttribute('href', /^data:image\/png;base64,/);
  // The tool stays armed; Escape puts the pointer back to selecting text.
  await expect(button(page, 'Image')).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Escape');
  await expect(button(page, 'Select')).toHaveAttribute('aria-pressed', 'true');

  // One undo step each; redo paints the image again.
  await button(page, 'Undo').click();
  await expect(stamps(page)).toHaveCount(1);
  await button(page, 'Undo').click();
  await expect(stamps(page)).toHaveCount(0);
  await button(page, 'Redo').click();
  await button(page, 'Redo').click();
  await expect(stamps(page)).toHaveCount(2);
  await expect(stamps(page).locator('image')).toHaveCount(2);
  await expect(page.locator('.annotation-mark--stamp')).toHaveCount(0);

  const { bytes } = await saveAsDownload(page);
  // In PDF space (origin bottom-left, y = 792 − drawn y).
  const rects = annotationRects(bytes, 'Stamp').sort((a, b) => a[0]! - b[0]!);
  expect(rects).toHaveLength(2);
  expectRect(rects[0], [100, 792 - 150 - 72, 244, 792 - 150]);
  expectRect(rects[1], [300, 792 - 250 - 144, 372, 792 - 250]);
  // Each with its appearance, which draws the image.
  for (const stamp of subtypeDictionaries(bytes, 'Stamp')) expect(stamp).toMatch(/\/AP\s*<<\s*\/N\s+\d+\s+\d+\s+R/);
  const images = subtypeDictionaries(bytes, 'Image');
  expect(images.some((image) => /\/Width\s+200\b/.test(image) && /\/Height\s+100\b/.test(image))).toBe(true);
  expect(images.some((image) => /\/Width\s+50\b/.test(image) && /\/Height\s+100\b/.test(image))).toBe(true);

  // Reopened, they are the file's: pdfium paints them; the session has none.
  await openBytes(page, 'stamped.pdf', bytes);
  await expectPagePainted(page, 1);
  await expect(page.locator('.annotation-layer [data-annotation-id]')).toHaveCount(0);
  // Two solid images, 144 × 72 and 72 × 144 points: thousands of pixels at any DPI.
  expect((await inkedPixelCount(page, 1)).inked).toBeGreaterThan(blank + 10_000);
});

test('a stamp on a page turned 90° turns with it, its anchor at the drawn top-right corner', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'turned.pdf', buildPdf([{ ...LETTER, rotate: 90 }]));
  await expectPagePainted(page, 1);

  const dropAt = await onPage(page, 300, 200);
  await dropFileAt(page, RED_PNG, dropAt.x, dropAt.y);
  await expect(stamps(page)).toHaveCount(1);
  await expect(stamps(page)).toHaveAttribute('transform', /rotate\(90\)/);
  // The 2:1 image drawn sideways: 72 wide, 144 high, its own top-left (drawn
  // top-right) at the drop point.
  const drawn = await drawnBox(page, stamps(page).locator('image'));
  expect(Math.abs(drawn.width - 72)).toBeLessThan(1);
  expect(Math.abs(drawn.height - 144)).toBeLessThan(1);
  expect(Math.abs(drawn.left + drawn.width - 300)).toBeLessThan(1);
  expect(Math.abs(drawn.top - 200)).toBeLessThan(1);

  const { bytes } = await saveAsDownload(page);
  expect(annotationRects(bytes, 'Stamp')).toHaveLength(1);
});
