// The editor embedded in a page that is not ours (e2e/host/): the host styles
// :root, body, a and button aggressively and sets a root font size of its own.
// The editor's stylesheet must stay inside its `.vitela` root, size the editor
// from its container, and keep its text and tokens whatever the host inherits
// down. What still reaches in from the host is pinned at the end (T-245).

import { expect, test, type Page } from '@playwright/test';
import { HOST_PORT } from './host/vite.config';
import { expectPagePainted, fixtureBytes, openFixture } from './support';

type Box = { readonly x: number; readonly y: number; readonly width: number; readonly height: number };

const HOST = `http://127.0.0.1:${HOST_PORT}/`;

/** Loads the host page and waits until the editor can open documents. */
async function loadHost(page: Page): Promise<void> {
  await page.goto(HOST);
  await expect(page.getByLabel('Open PDF')).toBeAttached({ timeout: 30_000 });
}

/** The computed values of `properties` on the first element matching `selector`. */
function styleOf(page: Page, selector: string, properties: readonly string[]): Promise<Record<string, string>> {
  return page
    .locator(selector)
    .first()
    .evaluate(
      (element, properties) => {
        const style = getComputedStyle(element);
        return Object.fromEntries(properties.map((name) => [name, style.getPropertyValue(name).trim()]));
      },
      [...properties],
    );
}

/** Layout boxes match up to sub-pixel rounding (Firefox reports 640.00002). */
function expectSameBox(actual: Box, expected: Box): void {
  for (const key of ['x', 'y', 'width', 'height'] as const) expect(actual[key]).toBeCloseTo(expected[key], 1);
}

const EDITOR_TEXT = 'rgb(22, 24, 29)'; // --text, light theme
const EDITOR_SURFACE = 'rgb(255, 255, 255)'; // --surface, light theme

test('keeps the editor styles off the host page root, body, links and buttons', async ({ page }) => {
  await loadHost(page);

  // No token, colour scheme or font of ours on the document root or body.
  expect(await styleOf(page, 'html', ['--surface', '--viewer-bg', 'color-scheme', 'font-size'])).toEqual({
    '--surface': '',
    '--viewer-bg': '',
    'color-scheme': 'normal',
    'font-size': '20px',
  });
  const body = await styleOf(page, 'body', ['--surface', 'margin-top', 'background-color', 'color', 'font-family']);
  expect(body).toMatchObject({
    '--surface': '',
    'margin-top': '24px',
    'background-color': 'rgb(10, 20, 30)',
    color: 'rgb(250, 240, 0)',
  });
  expect(body['font-family']).toContain('Courier New');

  // The host's own controls, before and after the editor, keep the host look.
  for (const button of ['#host-button', '#host-footer-button']) {
    const style = await styleOf(page, button, [
      'background-color',
      'color',
      'border-top-width',
      'border-top-color',
      'border-top-left-radius',
      'padding-top',
      'padding-left',
      'font-family',
    ]);
    expect(style).toMatchObject({
      'background-color': 'rgb(200, 0, 0)',
      color: 'rgb(255, 255, 255)',
      'border-top-width': '3px',
      'border-top-color': 'rgb(0, 0, 255)',
      'border-top-left-radius': '0px',
      'padding-top': '12px',
      'padding-left': '24px',
    });
    expect(style['font-family']).toContain('Courier New');
  }
  expect(await styleOf(page, '#host-link', ['color'])).toEqual({ color: 'rgb(0, 200, 0)' });
});

test('fills its container instead of the viewport, also with a document open', async ({ page }) => {
  await loadHost(page);
  await expect(page.locator('.vitela')).toHaveCount(1);
  const root = page.locator('#host-frame > .vitela');
  const frame = (await page.locator('#host-frame').boundingBox())!;
  expect(frame.height).toBeCloseTo(640, 1);
  expectSameBox((await root.boundingBox())!, frame);

  await openFixture(page, 'compress/vector_only.pdf');
  expectSameBox((await root.boundingBox())!, frame);
  const scroller = (await page.locator('.viewer__scroller').boundingBox())!;
  expect(scroller.y + scroller.height).toBeLessThanOrEqual(frame.y + frame.height + 0.5);
});

test('shows the drop hint over the editor, not over the whole viewport', async ({ page }) => {
  await loadHost(page);
  await page.locator('.vitela').evaluate((root) => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(['%PDF-'], 'dragged.pdf', { type: 'application/pdf' }));
    root.dispatchEvent(new DragEvent('dragenter', { bubbles: true, cancelable: true, dataTransfer: transfer }));
  });
  const hint = page.locator('.drop-hint');
  await expect(hint).toBeVisible();

  const root = (await page.locator('.vitela').boundingBox())!;
  const box = (await hint.boundingBox())!;
  expect(box.x).toBeCloseTo(root.x + 12, 0);
  expect(box.y).toBeCloseTo(root.y + 12, 0);
  expect(box.width).toBeCloseTo(root.width - 24, 0);
  expect(box.height).toBeCloseTo(root.height - 24, 0);
  // Sized from the editor's 15px, not the host's 20px root (1.3rem would be 26px).
  expect(await styleOf(page, '.drop-hint', ['font-size'])).toEqual({ 'font-size': '19.5px' });
});

test('keeps its own text, colours and tokens whatever the host page inherits down', async ({ page }) => {
  await loadHost(page);
  const root = await styleOf(page, '.vitela', [
    '--accent',
    '--text',
    'color',
    'background-color',
    'font-family',
    'font-size',
    'line-height',
    'color-scheme',
  ]);
  expect(root).toMatchObject({
    // The host's :root defines --accent and --text too; the editor's win inside.
    '--accent': '#1b2a57',
    '--text': '#16181d',
    color: EDITOR_TEXT,
    'background-color': EDITOR_SURFACE,
    'font-size': '15px',
    'line-height': '22.5px',
    'color-scheme': 'light dark',
  });
  expect(root['font-family']).not.toContain('Courier');

  // And it still works: a document opens and paints.
  await openFixture(page, 'compress/vector_only.pdf');
  await expectPagePainted(page, 1);
});

test('styles the password dialog in the top layer with the editor tokens', async ({ page }) => {
  await loadHost(page);
  await page.getByLabel('Open PDF').setInputFiles({
    name: 'aes_128_user_and_owner.pdf',
    mimeType: 'application/pdf',
    buffer: fixtureBytes('encrypted/aes_128_user_and_owner.pdf'),
  });
  const dialog = page.getByRole('dialog', { name: 'Password required' });
  await expect(dialog).toBeVisible({ timeout: 30_000 });

  const style = await styleOf(page, '.password-dialog', ['background-color', 'color', 'font-family', 'border-top-color']);
  expect(style).toMatchObject({
    'background-color': EDITOR_SURFACE,
    color: EDITOR_TEXT,
    'border-top-color': 'rgb(227, 228, 232)', // --border
  });
  expect(style['font-family']).not.toContain('Courier');
  // 1.15 × the editor's 15px, not × the host's 20px root.
  expect(await styleOf(page, '.password-dialog__title', ['font-size'])).toEqual({ 'font-size': '17.25px' });
});

// Known leak, kept measurable on purpose (T-245 decision: no Shadow DOM). A
// host type selector still reaches the editor's own elements: properties the
// editor never sets come through as they are, and ties on specificity go to
// whichever stylesheet loads later (here, the host's).
test('still lets host type selectors style the editor buttons (known leak)', async ({ page }) => {
  await loadHost(page);
  await openFixture(page, 'compress/vector_only.pdf');
  const style = await styleOf(page, '.view-switch button', ['text-transform', 'letter-spacing', 'font-family']);
  expect(style).toMatchObject({ 'text-transform': 'uppercase', 'letter-spacing': '3px' });
  expect(style['font-family']).toContain('Courier New');
});
