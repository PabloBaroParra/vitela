// The read-only "Edit content" mode (B26 T-258b): turned on from the Tools
// menu (a menuitemcheckbox), it outlines the text run, image or shape under
// the pointer, a press selects one and the status line names it, Escape lets
// it go and Done turns it off. A document whose permissions withhold
// modification keeps the item off, described by the core's sentence.

import { expect, test, type Page } from '@playwright/test';
import { LINE_FONT_PT, MODIFY_PROHIBITED, buildPdf, lineBaseline, type PageSpec } from './pdf';
import { expectOpened, expectPagePainted, loadApp, openBytes, openFixture, pageSlot, toolsButton } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792, lines: ['Vitela content line'] };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;

const item = (page: Page) => page.getByRole('menu', { name: 'Tools' }).getByRole('menuitemcheckbox', { name: 'Edit content' });
const mode = (page: Page) => page.getByRole('group', { name: 'Edit content' });
const status = (page: Page) => mode(page).getByRole('status');
const layer = (page: Page, number = 1) => pageSlot(page, number).locator('.content-layer');

/** Moves the pointer to (x, y) points on page `number` as drawn. */
async function pointAt(page: Page, number: number, x: number, y: number): Promise<{ x: number; y: number }> {
  const box = (await pageSlot(page, number).boundingBox())!;
  const point = { x: box.x + x * SCALE, y: box.y + y * SCALE };
  await page.mouse.move(point.x, point.y);
  return point;
}

async function enterMode(page: Page): Promise<void> {
  await toolsButton(page).click();
  await expect(item(page)).toHaveAttribute('aria-checked', 'false');
  await expect(item(page)).not.toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  await item(page).click();
  await expect(mode(page)).toBeVisible();
  await expect(layer(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
}

test('a run under the pointer is outlined, a press selects it and the status names its text', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'content.pdf', buildPdf([LETTER, LETTER, LETTER]));
  await expect(mode(page)).toHaveCount(0);
  await enterMode(page);
  await expect(status(page)).toHaveText('Point at text, an image or a shape on a page, and click to select it.');

  // The middle of the line's first letters, in points from the page's top.
  const lineTop = LETTER.heightPt - lineBaseline(LETTER, 0) - LINE_FONT_PT / 2;
  const point = await pointAt(page, 1, 90, lineTop);
  const hovered = layer(page).locator('.content-outline[data-state="hover"]');
  await expect(hovered).toHaveAttribute('data-kind', 'run');
  const outline = (await hovered.boundingBox())!;
  expect(outline.x).toBeLessThanOrEqual(point.x);
  expect(outline.x + outline.width).toBeGreaterThanOrEqual(point.x);
  expect(outline.y).toBeLessThanOrEqual(point.y);
  expect(outline.y + outline.height).toBeGreaterThanOrEqual(point.y);

  await page.mouse.down();
  await page.mouse.up();
  await expect(status(page)).toHaveText('Text “Vitela content line” selected on page 1.');
  await expect(layer(page).locator('.content-outline[data-state="selected"]')).toHaveAttribute('data-kind', 'run');
  // Nothing was selected as text: the press was the mode's.
  expect(await page.evaluate(() => window.getSelection()?.toString() ?? '')).toBe('');

  // Off the run, nothing is outlined; Escape lets the selection go.
  await pointAt(page, 1, 500, 700);
  await expect(layer(page).locator('.content-outline[data-state="hover"]')).toHaveCount(0);
  await page.getByLabel('Pages', { exact: true }).focus();
  await page.keyboard.press('Escape');
  await expect(layer(page).locator('.content-outline')).toHaveCount(0);
  await expect(status(page)).toHaveText('Point at text, an image or a shape on a page, and click to select it.');

  // The menu shows it on; Done turns it off, back on Tools, with no layer left.
  await toolsButton(page).click();
  await expect(item(page)).toHaveAttribute('aria-checked', 'true');
  await page.keyboard.press('Escape');
  await mode(page).getByRole('button', { name: 'Done' }).click();
  await expect(mode(page)).toHaveCount(0);
  await expect(page.locator('.content-layer')).toHaveCount(0);
  await expect(toolsButton(page)).toBeFocused();
});

test('only the pages in view and one on each side are parsed', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'long.pdf', buildPdf(Array.from({ length: 30 }, () => LETTER)));
  // Zoomed all the way out, several pages are in view and more are mounted
  // (one viewport of overscan on each side) than are parsed.
  const zoomOut = page.getByRole('button', { name: 'Zoom out' });
  while (await zoomOut.isEnabled()) await zoomOut.click();
  await enterMode(page);

  const view = (await page.getByLabel('Pages', { exact: true }).boundingBox())!;
  const slots = await page.locator('.page-slot').evaluateAll((nodes) =>
    nodes.map((node) => {
      const box = node.getBoundingClientRect();
      const layer = node.querySelector('.content-layer');
      return { number: Number(node.getAttribute('data-page')), top: box.top, bottom: box.bottom, state: layer?.getAttribute('data-state') };
    }),
  );
  const inView = slots.filter((slot) => slot.bottom > view.y && slot.top < view.y + view.height).map((slot) => slot.number);
  const first = Math.min(...inView);
  const last = Math.max(...inView);
  // Every page in view and the one after the last get parsed…
  for (const number of [...inView, last + 1]) {
    await expect(layer(page, number)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  }
  // …and no mounted page further away is.
  const states = await page.locator('.page-slot').evaluateAll((nodes) =>
    nodes.map((node) => [Number(node.getAttribute('data-page')), node.querySelector('.content-layer')?.getAttribute('data-state')] as const),
  );
  const far = states.filter(([number]) => number < first - 1 || number > last + 1);
  expect(far.length).toBeGreaterThan(0);
  for (const [, state] of far) expect(state).toBe('waiting');
  expect(slots.length).toBeLessThan(30);
});

test('an image is outlined as an image and selected as one', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, 'compress/scan_200dpi.pdf');
  await enterMode(page);
  // Inside the scan and inside the window: the page is taller than the view.
  const box = (await pageSlot(page, 1).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + 150);
  const hovered = layer(page).locator('.content-outline[data-state="hover"]');
  await expect(hovered).toHaveAttribute('data-kind', 'image');
  await expect(hovered).toHaveClass(/content-outline--image/);
  await page.mouse.down();
  await page.mouse.up();
  await expect(status(page)).toHaveText('Image selected on page 1.');
});

test("a document that forbids modification keeps the toggle off, described by the core's sentence", async ({ page }) => {
  const locked = buildPdf([LETTER], { encrypt: { permissions: MODIFY_PROHIBITED, userPassword: 'user-e2e' } });
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: locked });
  const prompt = page.getByRole('dialog', { name: 'Password required' });
  await expect(prompt).toBeVisible({ timeout: 30_000 });
  await prompt.getByLabel('Password').fill('user-e2e');
  await prompt.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');

  await toolsButton(page).click();
  await expect(item(page)).toHaveAccessibleDescription('Content editing is not permitted.', { timeout: 30_000 });
  await expect(item(page)).toHaveAttribute('aria-disabled', 'true');
  await expect(item(page)).toHaveAttribute('aria-checked', 'false');
  // A press does nothing (`force`: Playwright waits for an aria-disabled
  // item to turn enabled otherwise): the menu stays, the mode stays off.
  await item(page).click({ force: true });
  await expect(item(page)).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(mode(page)).toHaveCount(0);
  await expect(page.locator('.content-layer')).toHaveCount(0);
  await expectPagePainted(page, 1);
});
