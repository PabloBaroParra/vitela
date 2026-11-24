// Annotate: markup over the text selection (Highlight, Underline,
// Strikeout), drawn by the viewer's annotation layer, undone and redone, then
// saved — the download is read back for its `/Highlight` annotations and
// reopened, where pdfium paints them. A PDF that forbids annotations keeps
// the buttons off and says why. Documents are built in memory (pdf.ts), so
// each test knows where its text is.

import { expect, test, type Locator, type Page } from '@playwright/test';
import { saveAsDownload } from './organize';
import { annotationRects } from './pdfObjects';
import { buildPdf, lineBaseline, type PageSpec } from './pdf';
import { buildPng } from './png';
import { dropFileAt, expectOpened, expectPagePainted, loadApp, openBytes, pageSlot } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792 };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;
/** `/P` granting everything but bit 6, adding or changing annotations. */
const ANNOTATE_PROHIBITED = -4 & ~(1 << 5);

const marks = (page: Page, kind: string) => page.locator(`.annotation-layer [data-kind="${kind}"]`);
const button = (page: Page, name: string) => page.getByRole('button', { name, exact: true });

async function box(locator: Locator) {
  const rect = await locator.boundingBox();
  if (!rect) throw new Error('no box');
  return rect;
}

/** The middle of line `index` of `spec` on page 1, `x` points in, in viewport coordinates. */
async function onLine(page: Page, spec: PageSpec, index: number, x: number) {
  const slot = await box(pageSlot(page, 1));
  return { x: slot.x + x * SCALE, y: slot.y + (spec.heightPt - lineBaseline(spec, index) - 6) * SCALE };
}

/** Drags across the text from line `from` at `x0` to line `to` at `x1` (points). */
async function selectAcross(page: Page, spec: PageSpec, from: number, x0: number, to: number, x1: number) {
  const start = await onLine(page, spec, from, x0);
  const end = await onLine(page, spec, to, x1);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move((start.x + end.x) / 2, (start.y + end.y) / 2, { steps: 4 });
  await page.mouse.move(end.x, end.y, { steps: 4 });
  await page.mouse.up();
}

/** How many canvas pixels of page 1 look like the highlight's yellow, multiplied over paper. */
function yellowPixels(page: Page): Promise<number> {
  return pageSlot(page, 1)
    .locator('canvas.page-canvas')
    .evaluate((node) => {
      const canvas = node as HTMLCanvasElement;
      const { data } = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height);
      let yellow = 0;
      for (let i = 0; i < data.length; i += 4) if (data[i]! > 200 && data[i + 1]! > 170 && data[i + 2]! < 120) yellow++;
      return yellow;
    });
}

test('highlights a two-line selection, undoes and redoes it, and saves /Highlight with its rects', async ({ page }) => {
  await loadApp(page);
  const spec: PageSpec = { ...LETTER, lines: ['first marked line', 'second marked line'] };
  await openBytes(page, 'markup.pdf', buildPdf([spec]));
  await expect(button(page, 'Highlight')).toHaveAttribute('aria-disabled', 'true');
  await expect(button(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');

  // From the start of line 1 to the end of line 2 (Helvetica 18 pt: about 72–230 pt).
  await selectAcross(page, spec, 0, 70, 1, 240);
  await expect(pageSlot(page, 1).locator('.text-selection')).toHaveCount(2);
  await button(page, 'Highlight').click();

  // One annotation per line, drawn over the line the selection covered.
  await expect(marks(page, 'highlight')).toHaveCount(2);
  await expect(pageSlot(page, 1).locator('.text-selection')).toHaveCount(0);
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  const slot = await box(pageSlot(page, 1));
  const first = await box(marks(page, 'highlight').first());
  expect(Math.abs(first.x - slot.x - 72 * SCALE)).toBeLessThan(4 * SCALE);
  const middle = first.y - slot.y + first.height / 2;
  expect(Math.abs(middle - (792 - lineBaseline(spec, 0) - 6) * SCALE)).toBeLessThan(8 * SCALE);

  // One undo step per line, from the toolbar and from the keyboard.
  await button(page, 'Undo').click();
  await expect(marks(page, 'highlight')).toHaveCount(1);
  await button(page, 'Undo').click();
  await expect(marks(page, 'highlight')).toHaveCount(0);
  await expect(button(page, 'Undo')).toHaveAttribute('aria-disabled', 'true');
  await button(page, 'Redo').click();
  await expect(marks(page, 'highlight')).toHaveCount(1);
  await page.keyboard.press('ControlOrMeta+Shift+Z');
  await expect(marks(page, 'highlight')).toHaveCount(2);
  await page.keyboard.press('ControlOrMeta+Z');
  await expect(marks(page, 'highlight')).toHaveCount(1);
  // Ctrl+Y redoes too — not on a Mac, where ⌘+Y is the browser's History.
  const apple = await page.evaluate(() => /mac|iphone|ipad|ipod/i.test(navigator.platform));
  await page.keyboard.press(apple ? 'Meta+Shift+Z' : 'Control+Y');
  await expect(marks(page, 'highlight')).toHaveCount(2);
  await expect(button(page, 'Redo')).toHaveAttribute('aria-disabled', 'true');

  const { download, bytes } = await saveAsDownload(page);
  expect(download.suggestedFilename()).toBe('markup.pdf');
  // The saved file carries both lines, each with its own /Rect in PDF space
  // (origin bottom-left): 72 pt in, around each line's baseline.
  const rects = annotationRects(bytes, 'Highlight').sort((a, b) => b[1] - a[1]);
  expect(rects).toHaveLength(2);
  rects.forEach(([x0, y0, x1, y1], index) => {
    const baseline = lineBaseline(spec, index);
    expect(Math.abs(x0 - 72)).toBeLessThan(3);
    expect(x1).toBeGreaterThan(180);
    expect(y0).toBeLessThan(baseline);
    expect(y0).toBeGreaterThan(baseline - 10);
    expect(y1).toBeGreaterThan(baseline + 10);
    expect(y1).toBeLessThan(baseline + 22);
  });

  // Reopened, the highlights are in the file: pdfium paints them, and the
  // session has no annotations of its own.
  await openBytes(page, 'reopened.pdf', bytes);
  await expectPagePainted(page, 1);
  await expect(page.locator('.annotation-layer [data-annotation-id]')).toHaveCount(0);
  expect(await yellowPixels(page)).toBeGreaterThan(1000);
});

test('underlines and strikes out a word, and saves both', async ({ page }) => {
  await loadApp(page);
  const spec: PageSpec = { ...LETTER, lines: ['under struck'] };
  await openBytes(page, 'rules.pdf', buildPdf([spec]));

  const under = await onLine(page, spec, 0, 90);
  await page.mouse.dblclick(under.x, under.y);
  await expect(pageSlot(page, 1).locator('.text-selection')).toHaveCount(1);
  await button(page, 'Underline').click();
  await expect(marks(page, 'underline')).toHaveCount(1);

  const struck = await onLine(page, spec, 0, 150);
  await page.mouse.dblclick(struck.x, struck.y);
  await expect(pageSlot(page, 1).locator('.text-selection')).toHaveCount(1);
  await button(page, 'Strikeout').click();
  await expect(marks(page, 'strikeout')).toHaveCount(1);

  const { bytes } = await saveAsDownload(page);
  expect(annotationRects(bytes, 'Underline')).toHaveLength(1);
  expect(annotationRects(bytes, 'StrikeOut')).toHaveLength(1);
});

test('a PDF that forbids annotations keeps markup off and says why', async ({ page }) => {
  await loadApp(page);
  const spec: PageSpec = { ...LETTER, lines: ['Hello Vitela'] };
  const bytes = buildPdf([spec], { encrypt: { permissions: ANNOTATE_PROHIBITED, userPassword: 'reader' } });
  await page.getByLabel('Open PDF').setInputFiles({ name: 'no-annotate.pdf', mimeType: 'application/pdf', buffer: bytes });
  const dialog = page.getByRole('dialog', { name: 'Password required' });
  await dialog.getByLabel('Password').fill('reader');
  await dialog.getByLabel('Password').press('Enter');
  await expectOpened(page, 'no-annotate.pdf');

  // The text can still be selected (and copied); marking it up cannot.
  const word = await onLine(page, spec, 0, 100);
  await page.mouse.dblclick(word.x, word.y);
  await expect(pageSlot(page, 1).locator('.text-selection')).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Copy' })).toBeEnabled();
  for (const name of ['Highlight', 'Underline', 'Strikeout']) {
    await expect(button(page, name)).toHaveAttribute('aria-disabled', 'true');
    await expect(button(page, name)).toHaveAccessibleDescription(
      'This PDF does not allow annotations, so Highlight, Underline and Strikeout are turned off.',
    );
  }
  // The drawing tools too (T-238), and Image (T-239).
  for (const name of ['Ink', 'Shape', 'Note', 'Free text', 'Image']) {
    await expect(button(page, name)).toHaveAttribute('aria-disabled', 'true');
    await expect(button(page, name)).toHaveAccessibleDescription(
      'This PDF does not allow annotations, so Ink, Shape, Note, Free text and Image are turned off.',
    );
  }
  // A PNG dropped on the page is refused with the reason (and not opened in its place).
  const slot = await pageSlot(page, 1).boundingBox();
  await dropFileAt(
    page,
    { name: 'logo.png', mimeType: 'image/png', buffer: buildPng(20, 10, [200, 0, 0]) },
    slot!.x + 100,
    slot!.y + 100,
  );
  await expect(page.getByRole('alert')).toHaveText('This PDF does not allow annotations, so the image was not added.');
  await expect(page.getByText('no-annotate.pdf —')).toBeVisible();
  // `force`: Playwright would otherwise wait for an aria-disabled button to turn on.
  await button(page, 'Ink').click({ force: true });
  await expect(button(page, 'Select')).toHaveAttribute('aria-pressed', 'true');
  await expect(pageSlot(page, 1).locator('.drawing-layer')).toHaveCount(0);
  await button(page, 'Highlight').click({ force: true });
  await expect(page.locator('.annotation-layer [data-annotation-id]')).toHaveCount(0);
  // Nothing to select or edit (T-240).
  await expect(page.locator('.annotation-target')).toHaveCount(0);
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
});
