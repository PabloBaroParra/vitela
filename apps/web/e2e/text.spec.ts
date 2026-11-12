// Text: search and moving between matches, selecting and copying, and a
// document that withholds its text. Every document is built in memory
// (pdf.ts) so each test knows exactly where its text is drawn.

import { expect, test, type Locator, type Page } from '@playwright/test';
import { COPY_PROHIBITED, LINE_FONT_PT, buildPdf, lineBaseline, type PageSpec } from './pdf';
import { expectOpened, expectPagePainted, loadApp, openBytes, pageSlot } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792 };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;

const findBox = (page: Page) => page.getByLabel('Find in document');
const results = (page: Page) => page.getByLabel('Search results');
const scroller = (page: Page) => page.getByLabel('Pages', { exact: true });

async function box(locator: Locator) {
  const rect = await locator.boundingBox();
  if (!rect) throw new Error('no box');
  return rect;
}

/** Whether `inner` lies inside the scroller's visible area. */
async function expectInView(page: Page, inner: Locator) {
  const view = await box(scroller(page));
  const rect = await box(inner);
  expect(rect.y).toBeGreaterThanOrEqual(view.y);
  expect(rect.y + rect.height).toBeLessThanOrEqual(view.y + view.height);
  expect(rect.x).toBeGreaterThanOrEqual(view.x);
  expect(rect.x + rect.width).toBeLessThanOrEqual(view.x + view.width);
}

/**
 * How many dark canvas pixels of page `number` lie under `region` (CSS px,
 * page coordinates): the highlight must sit on the text pdfium drew.
 */
function inkUnder(page: Page, number: number, region: { x: number; y: number; width: number; height: number }) {
  return pageSlot(page, number)
    .locator('canvas.page-canvas')
    .evaluate((node, region) => {
      const canvas = node as HTMLCanvasElement;
      const ratio = canvas.width / canvas.getBoundingClientRect().width;
      const [x, y, w, h] = [region.x, region.y, region.width, region.height].map((v) => Math.round(v * ratio));
      const { data } = canvas.getContext('2d')!.getImageData(x!, y!, Math.max(1, w!), Math.max(1, h!));
      let inked = 0;
      for (let i = 0; i < data.length; i += 4) if (data[i]! < 128) inked++;
      return inked;
    }, region);
}

/** A highlight's box relative to its page slot, in CSS px. */
async function onPage(page: Page, number: number, highlight: Locator) {
  const slot = await box(pageSlot(page, number));
  const rect = await box(highlight);
  return { x: rect.x - slot.x, y: rect.y - slot.y, width: rect.width, height: rect.height };
}

test('searches the document and moves between matches, scrolling each into view', async ({ page }) => {
  await loadApp(page);
  const pages: PageSpec[] = Array.from({ length: 60 }, () => LETTER);
  pages[4] = { ...LETTER, lines: ['the first needle'] };
  pages[41] = { ...LETTER, lines: ['a needle far away'] };
  // Exact and case-sensitive, like the core: this one is not a match.
  pages[57] = { ...LETTER, lines: ['Needle'] };
  await openBytes(page, 'needles.pdf', buildPdf(pages));

  await findBox(page).fill('needle');
  await findBox(page).press('Enter');
  await expect(results(page)).toHaveText('1 of 2');
  const current = page.locator('.search-hit--current');
  await expect(pageSlot(page, 5).locator('.search-hit--current')).toHaveCount(1);
  await expectInView(page, current);

  // Next: page 42 was never mounted; the viewer scrolls there and shows it.
  await page.getByRole('button', { name: 'Next match' }).click();
  await expect(results(page)).toHaveText('2 of 2');
  await expect(pageSlot(page, 42).locator('.search-hit--current')).toHaveCount(1);
  await expectInView(page, current);
  await expectPagePainted(page, 42);
  // On the drawn word: about 160 pt from the top of the page, 72 pt in.
  const hit = await onPage(page, 42, current);
  expect(hit.x).toBeGreaterThan(72 * SCALE);
  expect(Math.abs(hit.y + hit.height / 2 - (792 - lineBaseline(LETTER, 0) - 6) * SCALE)).toBeLessThan(10 * SCALE);
  expect(await inkUnder(page, 42, hit)).toBeGreaterThan(50);

  // Enter on the same query moves on and wraps; Shift+Enter goes back.
  await findBox(page).press('Enter');
  await expect(results(page)).toHaveText('1 of 2');
  await expect(pageSlot(page, 5).locator('.search-hit--current')).toHaveCount(1);
  await expectInView(page, current);
  await findBox(page).press('Shift+Enter');
  await expect(results(page)).toHaveText('2 of 2');
  await expectInView(page, current);

  await findBox(page).fill('nowhere');
  await findBox(page).press('Enter');
  await expect(results(page)).toHaveText('No matches');
  await expect(page.locator('.search-hit')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Next match' })).toBeDisabled();
});

test('places a match on a rotated page over the text pdfium drew there', async ({ page }) => {
  await loadApp(page);
  const turned: PageSpec = { ...LETTER, rotate: 90, lines: ['turned words'] };
  await openBytes(page, 'turned.pdf', buildPdf([LETTER, turned]));

  await findBox(page).fill('turned');
  await findBox(page).press('Enter');
  await expect(results(page)).toHaveText('1 of 1');
  await expectPagePainted(page, 2);
  const hit = await onPage(page, 2, pageSlot(page, 2).locator('.search-hit--current'));
  // A quarter turn clockwise: the line now runs down the page, its PDF
  // baseline height becomes the distance from the left edge.
  expect(hit.height).toBeGreaterThan(hit.width);
  expect(hit.x).toBeGreaterThan((lineBaseline(turned, 0) - LINE_FONT_PT) * SCALE);
  expect(hit.x).toBeLessThan((lineBaseline(turned, 0) + LINE_FONT_PT) * SCALE);
  expect(await inkUnder(page, 2, hit)).toBeGreaterThan(50);
});

test('selects text by dragging and by double click, and copies it', async ({ page, context, browserName }) => {
  // Headless Chromium refuses clipboard access unless granted.
  if (browserName === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await recordClipboardWrites(page);
  await loadApp(page);
  const spec: PageSpec = { ...LETTER, lines: ['Hello Vitela copy test'] };
  await openBytes(page, 'copy.pdf', buildPdf([spec]));
  const copy = page.getByRole('button', { name: 'Copy' });
  await expect(copy).toBeDisabled();

  // The line's middle, in viewport coordinates.
  const slot = await box(pageSlot(page, 1));
  const y = slot.y + (792 - lineBaseline(spec, 0) - 6) * SCALE;
  const at = (pt: number) => slot.x + pt * SCALE;

  // Drag across "Hello Vitela" (Helvetica 18 pt: 72 pt to about 163 pt).
  await page.mouse.move(at(70), y);
  await page.mouse.down();
  await page.mouse.move(at(120), y, { steps: 4 });
  await page.mouse.move(at(164), y, { steps: 4 });
  await page.mouse.up();
  const selection = pageSlot(page, 1).locator('.text-selection');
  await expect(selection).toHaveCount(1);
  expect(await inkUnder(page, 1, await onPage(page, 1, selection))).toBeGreaterThan(50);
  await expect(copy).toBeEnabled();

  await page.keyboard.press('ControlOrMeta+C');
  await expect(page.getByText('Copied', { exact: true })).toBeVisible();
  await expectCopied(page, browserName, 'Hello Vitela');

  // A double click selects the word under it; the button copies it.
  await page.mouse.dblclick(at(140), y);
  await expect.poll(async () => (await box(selection)).width).toBeLessThan(60 * SCALE);
  await copy.click();
  await expect(page.getByText('Copied', { exact: true })).toBeVisible();
  await expectCopied(page, browserName, 'Vitela');

  // A click on blank paper drops the selection.
  await page.mouse.click(at(450), y);
  await expect(selection).toHaveCount(0);
  await expect(copy).toBeDisabled();
});

test('a PDF that withholds its text opens, but search and selection are off and say why', async ({ page }) => {
  await loadApp(page);
  const spec: PageSpec = { ...LETTER, lines: ['Hello Vitela'] };
  const bytes = buildPdf([spec], { encrypt: { permissions: COPY_PROHIBITED, userPassword: 'reader' } });
  await page.getByLabel('Open PDF').setInputFiles({ name: 'no-copy.pdf', mimeType: 'application/pdf', buffer: bytes });
  // The user password opens it with the document's restrictions in force.
  const dialog = page.getByRole('dialog', { name: 'Password required' });
  await dialog.getByLabel('Password').fill('reader');
  await dialog.getByLabel('Password').press('Enter');
  await expectOpened(page, 'no-copy.pdf');

  await expect(page.getByRole('note')).toHaveText(
    'This PDF does not allow copying its text, so search and text selection are turned off.',
  );
  await expect(findBox(page)).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Copy' })).toBeDisabled();

  // Dragging over the text selects nothing, and there is nothing to copy.
  const slot = await box(pageSlot(page, 1));
  const y = slot.y + (792 - lineBaseline(spec, 0) - 6) * SCALE;
  await page.mouse.move(slot.x + 70 * SCALE, y);
  await page.mouse.down();
  await page.mouse.move(slot.x + 160 * SCALE, y, { steps: 4 });
  await page.mouse.up();
  await page.mouse.dblclick(slot.x + 100 * SCALE, y);
  await expect(pageSlot(page, 1).locator('.text-layer')).toHaveAttribute('data-selectable', 'false');
  await expect(page.locator('.text-selection')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Copy' })).toBeDisabled();
});

/** Notes every text the page writes to the clipboard, in `window.copied`. */
function recordClipboardWrites(page: Page) {
  return page.addInitScript(() => {
    const scope = window as unknown as { copied: string[] };
    scope.copied = [];
    const write = Clipboard.prototype.writeText;
    Clipboard.prototype.writeText = function (text: string) {
      scope.copied.push(text);
      return write.call(this, text);
    };
  });
}

/**
 * The app wrote `text` to the clipboard, and — where the browser lets a test
 * paste — the system clipboard really holds it: pasted into the search box
 * (a real paste, no clipboard-read permission), which is emptied again.
 * Playwright's WebKit neither pastes nor reads the clipboard, so there the
 * write itself, which resolved ("Copied"), is the evidence.
 */
async function expectCopied(page: Page, browserName: string, text: string) {
  expect(await page.evaluate(() => (window as unknown as { copied: string[] }).copied.at(-1))).toBe(text);
  if (browserName === 'webkit') return;
  const field = findBox(page);
  await field.fill('');
  await field.focus();
  await page.keyboard.press('ControlOrMeta+V');
  await expect(field).toHaveValue(text);
  await field.fill('');
}
