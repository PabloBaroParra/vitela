// Editing an annotation already on the page: a press in Select mode selects
// it; it is moved by dragging, with the arrow keys and with "Move to", resized
// by its handles and with "Resize", recoloured, retyped and deleted, each one
// undo step — then saved, and the download's rects, colour and text checked.
// A stamp keeps its proportions and its image; on a page turned 90° the file
// gets the box where it was left on screen. Pressing anywhere else still
// selects text, and the arrows still scroll while nothing is selected.

import { expect, test, type Locator, type Page } from '@playwright/test';
import { saveAsDownload } from './organize';
import { annotationRects, subtypeDictionaries } from './pdfObjects';
import { buildPdf, lineBaseline, type PageSpec } from './pdf';
import { buildPng } from './png';
import { dropFileAt, expectPagePainted, loadApp, openBytes, pageSlot } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792 };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;

const marks = (page: Page, kind: string) => page.locator(`.annotation-layer [data-kind="${kind}"]`);
const button = (page: Page, name: string) => page.getByRole('button', { name, exact: true });
const annotationBar = (page: Page) => page.getByRole('toolbar', { name: 'Annotation', exact: true });

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

/** Presses at the first point of page 1 (drawn points), moves to the second and lets go. */
async function drag(page: Page, from: readonly [number, number], to: readonly [number, number]) {
  const start = await onPage(page, ...from);
  const end = await onPage(page, ...to);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move((start.x + end.x) / 2, (start.y + end.y) / 2, { steps: 4 });
  await page.mouse.move(end.x, end.y, { steps: 4 });
  await page.mouse.up();
}

async function click(page: Page, x: number, y: number) {
  const point = await onPage(page, x, y);
  await page.mouse.click(point.x, point.y);
}

/** Draws a shape from one corner to the other and goes back to Select. */
async function drawShape(page: Page, from: readonly [number, number], to: readonly [number, number]) {
  await button(page, 'Shape').click();
  await drag(page, from, to);
  await expect(marks(page, 'shape')).toHaveCount(1);
  await button(page, 'Select').click();
}

/** Waits for the (only) shape to be drawn as [left, top, width, height] points. */
async function expectShape(page: Page, expected: readonly number[], tolerance = 2) {
  const shape = marks(page, 'shape');
  await expect(async () => {
    for (const [index, name] of ['x', 'y', 'width', 'height'].entries()) {
      expect(Math.abs(Number(await shape.getAttribute(name)) - expected[index]!)).toBeLessThan(tolerance);
    }
  }).toPass({ timeout: 5_000 });
}

function expectRect(rect: readonly number[] | undefined, expected: readonly number[], tolerance = 1.5) {
  expect(rect).toHaveLength(4);
  rect!.forEach((value, index) => expect(Math.abs(value - expected[index]!)).toBeLessThan(tolerance));
}

test('moves, resizes, recolours, deletes and undoes a shape, and saves its rect and colour', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'edit.pdf', buildPdf([LETTER]));
  await expectPagePainted(page, 1);
  await drawShape(page, [100, 150], [200, 210]);
  await expect(annotationBar(page)).toHaveCount(0);

  // A press selects it, and records nothing.
  await click(page, 150, 180);
  const target = button(page, 'Shape on page 1');
  await expect(target).toHaveAttribute('aria-pressed', 'true');
  await expect(target).toBeFocused();
  await expect(annotationBar(page)).toBeVisible();
  await expect(button(page, 'Undo')).toHaveAttribute('aria-disabled', 'false');

  // Dragged: one step, which one undo takes back (a step drawn, a step moved).
  await drag(page, [150, 180], [190, 200]);
  await expectShape(page, [140, 170, 100, 60]);
  await button(page, 'Undo').click();
  await expectShape(page, [100, 150, 100, 60]);
  await button(page, 'Redo').click();
  await expectShape(page, [140, 170, 100, 60]);

  // Arrows: a point, or twelve with Shift; several in a row are all kept.
  await target.focus();
  for (let i = 0; i < 5; i++) await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Shift+ArrowDown');
  await expectShape(page, [145, 182, 100, 60], 0.9);

  // Resized by the bottom-right handle, then with Alt+arrows.
  await drag(page, [245, 242], [275, 262]);
  await expectShape(page, [145, 182, 130, 80]);
  await page.keyboard.press('Alt+Shift+ArrowLeft');
  await expectShape(page, [145, 182, 118, 80]);
  await target.focus();
  await page.keyboard.press('Alt+Shift+ArrowUp');
  await expectShape(page, [145, 182, 118, 68]);

  // Moved and resized by typed lengths.
  await annotationBar(page).getByRole('button', { name: 'Move to…' }).click();
  const move = page.getByRole('dialog', { name: 'Move to' });
  await expect(move.getByLabel('Left (pt)')).toBeFocused();
  await move.getByLabel('Left (pt)').fill('700');
  await move.getByLabel('Left (pt)').press('Enter');
  await expect(move.getByRole('alert')).toContainText('Left must be between 0 and');
  await move.getByLabel('Left (pt)').fill('120');
  await move.getByLabel('Top (pt)').fill('160');
  await move.getByRole('button', { name: 'Move', exact: true }).click();
  await expect(move).toHaveCount(0);
  await expect(annotationBar(page).getByRole('button', { name: 'Move to…' })).toBeFocused();
  await expectShape(page, [120, 160, 118, 68]);
  await annotationBar(page).getByRole('button', { name: 'Resize…' }).click();
  const resize = page.getByRole('dialog', { name: 'Resize' });
  await expect(resize.getByLabel('Keep proportions')).not.toBeChecked();
  await resize.getByLabel('Width (pt)').fill('90');
  await resize.getByLabel('Height (pt)').fill('45');
  await resize.getByLabel('Height (pt)').press('Enter');
  await expect(resize).toHaveCount(0);
  await expectShape(page, [120, 160, 90, 45], 0.1);

  await annotationBar(page).getByRole('button', { name: 'Blue' }).click();
  await expect(marks(page, 'shape')).toHaveAttribute('stroke', 'rgb(30 100 230)');

  // Deleted: the focus stays in the pages; an undo brings it back, not selected.
  await target.focus();
  await page.keyboard.press('Delete');
  await expect(marks(page, 'shape')).toHaveCount(0);
  await expect(annotationBar(page)).toHaveCount(0);
  await expect(page.getByLabel('Pages', { exact: true })).toBeFocused();
  await page.keyboard.press('ControlOrMeta+Z');
  await expect(marks(page, 'shape')).toHaveCount(1);
  await expect(annotationBar(page)).toHaveCount(0);

  const { bytes } = await saveAsDownload(page);
  expectRect(annotationRects(bytes, 'Square')[0], [120, 792 - 205, 210, 792 - 160], 0.5);
  const [square] = subtypeDictionaries(bytes, 'Square');
  const color = /\/C\s*\[([^\]]*)\]/.exec(square!)![1]!.trim().split(/\s+/).map(Number);
  [30 / 255, 100 / 255, 230 / 255].forEach((channel, index) => expect(Math.abs(color[index]! - channel)).toBeLessThan(0.01));

  // Reopened, it is the file's: nothing to select.
  await openBytes(page, 'edited.pdf', bytes);
  await expect(page.locator('.annotation-target')).toHaveCount(0);
});

test('reaches an annotation from the keyboard, and leaves text and scrolling alone otherwise', async ({ page }) => {
  await loadApp(page);
  const spec: PageSpec = { ...LETTER, lines: ['some words to select'] };
  await openBytes(page, 'keys.pdf', buildPdf([spec, LETTER]));
  await drawShape(page, [300, 250], [400, 300]);

  // Tab goes from the pages to the annotation; Enter selects it, Escape lets go.
  const pagesArea = page.getByLabel('Pages', { exact: true });
  await pagesArea.focus();
  await page.keyboard.press('Tab');
  const target = button(page, 'Shape on page 1');
  await expect(target).toBeFocused();
  await expect(annotationBar(page)).toHaveCount(0);
  await page.keyboard.press('Enter');
  await expect(target).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Tab');
  await expect(button(page, 'Resize from the top left')).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expectShape(page, [299, 250, 101, 50]);
  await page.keyboard.press('Escape');
  await expect(annotationBar(page)).toHaveCount(0);
  await expect(target).toBeFocused();

  // Not selected: the arrows scroll.
  await page.keyboard.press('ArrowDown');
  await expect.poll(() => pagesArea.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
  await expectShape(page, [299, 250, 101, 50]);
  await pagesArea.evaluate((element) => element.scrollTo(0, 0));

  // A press elsewhere lets go and selects text, as before.
  await click(page, 350, 275);
  await expect(annotationBar(page)).toBeVisible();
  const y = spec.heightPt - lineBaseline(spec, 0) - 6;
  await drag(page, [74, y], [180, y]);
  await expect(annotationBar(page)).toHaveCount(0);
  await expect(pageSlot(page, 1).locator('.text-selection')).toHaveCount(1);
  // And selecting the annotation drops the text selection.
  await click(page, 350, 275);
  await expect(pageSlot(page, 1).locator('.text-selection')).toHaveCount(0);
});

test('retypes a text box, keeping the dialog open for a character it cannot show', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'text.pdf', buildPdf([LETTER]));
  await button(page, 'Free text').click();
  await drag(page, [100, 150], [300, 200]);
  const add = page.getByRole('dialog', { name: 'Add a text box' });
  await add.getByRole('textbox').fill('First draft');
  await add.getByRole('button', { name: 'Add text box' }).click();
  await expect(marks(page, 'freeText').locator('text')).toHaveText('First draft');
  await button(page, 'Select').click();

  await click(page, 200, 175);
  await annotationBar(page).getByRole('button', { name: 'Edit text…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Edit the text box' });
  const field = dialog.getByRole('textbox');
  await expect(field).toHaveValue('First draft');
  await field.fill('日本');
  await dialog.getByRole('button', { name: 'Save text' }).click();
  await expect(dialog.getByRole('alert')).toContainText("'日' cannot be shown in Helvetica (WinAnsi)");
  await field.fill('Final text');
  await expect(dialog.locator('.free-text-preview text')).toHaveText('Final text');
  await dialog.getByRole('button', { name: 'Save text' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(marks(page, 'freeText').locator('text')).toHaveText('Final text');
  await button(page, 'Undo').click();
  await expect(marks(page, 'freeText').locator('text')).toHaveText('First draft');
  await button(page, 'Redo').click();
  await expect(marks(page, 'freeText').locator('text')).toHaveText('Final text');

  const { bytes } = await saveAsDownload(page);
  expect(subtypeDictionaries(bytes, 'FreeText')[0]).toContain('Final text');
});

test('a stamp keeps its proportions and its image through a move, a resize, a delete and undo', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'stamp.pdf', buildPdf([LETTER]));
  const at = await onPage(page, 100, 150);
  // 2:1, stamped 144 × 72.
  await dropFileAt(page, { name: 'red.png', mimeType: 'image/png', buffer: buildPng(200, 100, [220, 30, 30]) }, at.x, at.y);
  const image = marks(page, 'stamp').locator('image');
  await expect(image).toHaveCount(1);

  await drag(page, [150, 180], [190, 200]);
  await expect(button(page, 'Image stamp on page 1')).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(async () => Math.round(((await box(image)).x - (await box(pageSlot(page, 1))).x) / SCALE)).toBe(140);
  // Pulled twice as wide and barely taller: in proportion, twice as tall too.
  await drag(page, [284, 242], [428, 246]);
  await expect.poll(async () => Math.round((await box(image)).width / SCALE)).toBe(288);
  await expect.poll(async () => Math.round((await box(image)).height / SCALE)).toBe(144);

  await annotationBar(page).getByRole('button', { name: 'Delete' }).click();
  await expect(image).toHaveCount(0);
  await button(page, 'Undo').click();
  await expect(image).toHaveAttribute('href', /^data:image\/png;base64,/);
  await expect(page.locator('.annotation-mark--stamp')).toHaveCount(0);

  const { bytes } = await saveAsDownload(page);
  expectRect(annotationRects(bytes, 'Stamp')[0], [140, 792 - 170 - 144, 428, 792 - 170]);
});

test('on a page turned 90° the file gets the box where it was left on screen', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'turned.pdf', buildPdf([{ ...LETTER, rotate: 90 }]));
  await drawShape(page, [100, 100], [200, 150]);

  await drag(page, [150, 125], [190, 145]);
  await expectShape(page, [140, 120, 100, 50]);
  await page.keyboard.press('Shift+ArrowDown');
  await expectShape(page, [140, 132, 100, 50]);
  await drag(page, [240, 182], [260, 212]);
  await expectShape(page, [140, 132, 120, 80]);

  const { bytes } = await saveAsDownload(page);
  // A quarter turn clockwise draws PDF (x, y) at (y, x).
  expectRect(annotationRects(bytes, 'Square')[0], [132, 140, 212, 260], 2);
});
