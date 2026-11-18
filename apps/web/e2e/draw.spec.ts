// Draw: the viewer's drawing tools — ink, a rectangle, a note and a text box
// drawn with the mouse, each through its own tool, with a colour; the note
// and the text box ask for their text in a dialog, and a text box keeps its
// dialog open with the core's error for a character Helvetica (WinAnsi)
// cannot show. Saved, the download carries `/Ink`, `/Square`, `/Text` and
// `/FreeText`, and reopens with pdfium painting them. A shape on a page
// turned 90° lands in the file where it was drawn.

import { expect, test, type Locator, type Page } from '@playwright/test';
import { saveAsDownload } from './organize';
import { annotationRects } from './pdfObjects';
import { buildPdf, type PageSpec } from './pdf';
import { expectPagePainted, inkedPixelCount, loadApp, openBytes, pageSlot } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792 };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;

const marks = (page: Page, kind: string) => page.locator(`.annotation-layer [data-kind="${kind}"]`);
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

/** Presses at the first point, moves through the rest, releases at the last (points on page 1). */
async function stroke(page: Page, ...points: readonly (readonly [number, number])[]) {
  const [first, ...rest] = points;
  const start = await onPage(page, first![0], first![1]);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  for (const [x, y] of rest) {
    const next = await onPage(page, x, y);
    await page.mouse.move(next.x, next.y, { steps: 4 });
  }
  await page.mouse.up();
}

/** `rect` as [x0, y0, x1, y1] is within `tolerance` points of `expected`. */
function expectRect(rect: readonly number[] | undefined, expected: readonly number[], tolerance = 2) {
  expect(rect).toHaveLength(4);
  rect!.forEach((value, index) => expect(Math.abs(value - expected[index]!)).toBeLessThan(tolerance));
}

test('draws ink, a shape, a note and a text box, and saves /Ink /Square /Text /FreeText', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'draw.pdf', buildPdf([LETTER]));
  await expectPagePainted(page, 1);
  const blank = (await inkedPixelCount(page, 1)).inked;
  const toolbar = page.getByRole('toolbar', { name: 'Draw' });
  await expect(button(page, 'Select')).toHaveAttribute('aria-pressed', 'true');

  // Ink, in blue: one stroke, one annotation.
  await toolbar.getByRole('button', { name: 'Blue' }).click();
  await button(page, 'Ink').click();
  await expect(button(page, 'Ink')).toHaveAttribute('aria-pressed', 'true');
  await expect(pageSlot(page, 1).locator('.text-layer')).toHaveAttribute('data-selectable', 'false');
  await stroke(page, [80, 100], [120, 140], [160, 100], [200, 140]);
  await expect(marks(page, 'ink')).toHaveCount(1);
  await expect(marks(page, 'ink')).toHaveAttribute('stroke', 'rgb(30 100 230)');
  await expect(page.getByText('Unsaved changes.')).toBeVisible();

  // A rectangle traced from (300, 100) to (400, 160).
  await button(page, 'Shape').click();
  await stroke(page, [300, 100], [350, 130], [400, 160]);
  await expect(marks(page, 'shape')).toHaveCount(1);

  // A note: a click, then its text.
  await button(page, 'Note').click();
  const notePoint = await onPage(page, 100, 300);
  await page.mouse.click(notePoint.x, notePoint.y);
  const noteDialog = page.getByRole('dialog', { name: 'Add a note' });
  await expect(noteDialog.getByRole('textbox')).toBeFocused();
  await noteDialog.getByRole('textbox').fill('Check the totals');
  await noteDialog.getByRole('button', { name: 'Add note' }).click();
  await expect(noteDialog).toHaveCount(0);
  await expect(marks(page, 'note')).toHaveCount(1);

  // A text box traced from (300, 300) to (500, 360): a character outside
  // WinAnsi keeps the dialog open with the core's error, typed or added.
  await button(page, 'Free text').click();
  await stroke(page, [300, 300], [400, 330], [500, 360]);
  const textDialog = page.getByRole('dialog', { name: 'Add a text box' });
  const field = textDialog.getByRole('textbox');
  await expect(field).toBeFocused();
  await field.fill('Total 日本');
  await expect(textDialog.getByRole('alert')).toContainText('cannot be shown in Helvetica (WinAnsi)');
  await textDialog.getByRole('button', { name: 'Add text box' }).click();
  await expect(textDialog).toBeVisible();
  await expect(textDialog.getByRole('alert')).toContainText("'日' cannot be shown in Helvetica (WinAnsi)");
  await expect(marks(page, 'freeText')).toHaveCount(0);
  await field.fill('Total checked');
  await expect(textDialog.getByRole('alert')).toHaveCount(0);
  // The core's layout, previewed in the box.
  await expect(textDialog.locator('.free-text-preview text')).toHaveText('Total checked');
  await textDialog.getByRole('button', { name: 'Add text box' }).click();
  await expect(textDialog).toHaveCount(0);
  await expect(marks(page, 'freeText')).toHaveCount(1);
  await expect(marks(page, 'freeText').locator('text')).toHaveText('Total checked');

  // Escape puts the pointer back to selecting text.
  await page.keyboard.press('Escape');
  await expect(button(page, 'Select')).toHaveAttribute('aria-pressed', 'true');
  await expect(pageSlot(page, 1).locator('.drawing-layer')).toHaveCount(0);

  // One undo step each.
  await button(page, 'Undo').click();
  await expect(marks(page, 'freeText')).toHaveCount(0);
  await button(page, 'Redo').click();
  await expect(marks(page, 'freeText')).toHaveCount(1);

  const { bytes } = await saveAsDownload(page);
  // In PDF space (origin bottom-left, y = 792 − drawn y).
  expect(annotationRects(bytes, 'Ink')).toHaveLength(1);
  const [ink] = annotationRects(bytes, 'Ink');
  expect(ink![0]).toBeLessThan(82);
  expect(ink![2]).toBeGreaterThan(198);
  expectRect(annotationRects(bytes, 'Square')[0], [300, 792 - 160, 400, 792 - 100]);
  expect(annotationRects(bytes, 'Text')).toHaveLength(1);
  expectRect(annotationRects(bytes, 'Text')[0], [100, 792 - 324, 124, 792 - 300]);
  expectRect(annotationRects(bytes, 'FreeText')[0], [300, 792 - 360, 500, 792 - 300]);

  // Reopened, they are the file's: pdfium paints them; the session has none.
  await openBytes(page, 'drawn.pdf', bytes);
  await expectPagePainted(page, 1);
  await expect(page.locator('.annotation-layer [data-annotation-id]')).toHaveCount(0);
  expect((await inkedPixelCount(page, 1)).inked).toBeGreaterThan(blank + 200);
});

test('a shape on a page turned 90° lands in the file where it was drawn', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'turned.pdf', buildPdf([{ ...LETTER, rotate: 90 }]));
  // Drawn landscape: 792 × 612 points.
  const slot = await box(pageSlot(page, 1));
  expect(Math.round(slot.width / SCALE)).toBe(792);

  await button(page, 'Shape').click();
  await stroke(page, [100, 100], [150, 120], [200, 150]);
  await expect(marks(page, 'shape')).toHaveCount(1);

  const { bytes } = await saveAsDownload(page);
  // A quarter turn clockwise draws PDF (x, y) at (y, x): the drawn box
  // 100–200 across and 100–150 down is PDF x 100–150, y 100–200.
  expectRect(annotationRects(bytes, 'Square')[0], [100, 100, 150, 200]);
});
