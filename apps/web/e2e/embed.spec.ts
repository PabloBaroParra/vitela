// The editor embedded in a page that is not ours (e2e/host/): the host styles
// :root, body, a and button aggressively and sets a root font size of its own.
// The editor's stylesheet must stay inside its `.vitela` root, size the editor
// from its container, and keep its text and tokens whatever the host inherits
// down. What still reaches in from the host is pinned (T-245). Its events stay
// inside too: a file dropped on the host is the host's, and the editor's keys
// work only with focus in the editor (T-246).

import { expect, test, type Page } from '@playwright/test';
import { HOST_PORT } from './host/vite.config';
import { buildPdf } from './pdf';
import { buildPng } from './png';
import {
  dropFileAt,
  expectOpened,
  expectPagePainted,
  fixtureBytes,
  openBytes,
  openFixture,
  pageSlot,
} from './support';

type Box = { readonly x: number; readonly y: number; readonly width: number; readonly height: number };

const HOST = `http://127.0.0.1:${HOST_PORT}/`;
const RED_PNG = { name: 'red.png', mimeType: 'image/png', buffer: buildPng(200, 100, [220, 30, 30]) };

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

/**
 * Dispatches a file drag on the first element matching `selector`, one event
 * per type, as a file manager drag would. Returns, per event, whether the
 * editor took it (`defaultPrevented`) and whether the drop hint showed a
 * frame later.
 */
function dragFile(page: Page, selector: string, types: readonly string[]) {
  return page.locator(selector).first().evaluate(async (target, types) => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(['%PDF-1.4'], 'dropped.pdf', { type: 'application/pdf' }));
    const taken = types.map((type) => {
      const event = new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: transfer });
      target.dispatchEvent(event);
      return event.defaultPrevented;
    });
    // Two frames: React has rendered whatever the events asked for.
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return { taken, hint: !!document.querySelector('.drop-hint') };
  }, types);
}

test('takes a file dropped on the editor, and leaves one dropped on the host page alone', async ({ page }) => {
  await loadHost(page);

  // Dragged over the host's own elements: no hint, the browser default left to the host.
  for (const selector of ['#host-button', 'body']) {
    expect(await dragFile(page, selector, ['dragenter', 'dragover'])).toEqual({ taken: [false, false], hint: false });
    expect(await dragFile(page, selector, ['drop'])).toEqual({ taken: [false], hint: false });
  }
  await expect(page.locator('.workspace__bar')).toHaveCount(0);

  // Over the editor: the hint shows, and the drop opens the PDF in it.
  const pdf = buildPdf([{ widthPt: 612, heightPt: 792 }]);
  expect(await dragFile(page, '.vitela', ['dragenter', 'dragover'])).toEqual({ taken: [false, true], hint: true });
  await page.locator('.vitela').evaluate((root, bytes) => {
    const transfer = new DataTransfer();
    transfer.items.add(new File([new Uint8Array(bytes)], 'dropped.pdf', { type: 'application/pdf' }));
    root.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: transfer }));
  }, [...pdf]);
  await expectOpened(page, 'dropped.pdf');
  await expect(page.locator('.drop-hint')).toHaveCount(0);
});

test('undoes from the keyboard only with focus in the editor', async ({ page }) => {
  await loadHost(page);
  await openBytes(page, 'keys.pdf', buildPdf([{ widthPt: 612, heightPt: 792 }]));
  await expectPagePainted(page, 1);

  // One ink stroke on page 1.
  await page.getByRole('button', { name: 'Ink', exact: true }).click();
  const slot = (await pageSlot(page, 1).boundingBox())!;
  await page.mouse.move(slot.x + 100, slot.y + 100);
  await page.mouse.down();
  await page.mouse.move(slot.x + 200, slot.y + 160, { steps: 4 });
  await page.mouse.up();
  const ink = page.locator('.annotation-layer [data-kind="ink"]');
  await expect(ink).toHaveCount(1);

  // Focus on a host control: the key is the host's.
  await page.locator('#host-button').click();
  await page.keyboard.press('ControlOrMeta+Z');
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Ink', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(ink).toHaveCount(1);

  // A click on a part of the editor that takes no focus (its status line)
  // leaves focus on the page body; the keys are the editor's again.
  await page.locator('.workspace__bar').getByText('keys.pdf —').click();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Select', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('ControlOrMeta+Z');
  await expect(ink).toHaveCount(0);
});

test('undoes a stamp dropped on the editor without a click first', async ({ page }) => {
  await loadHost(page);
  // Nothing clicked: the drops alone put the user in the editor.
  await page.locator('.vitela').evaluate((root, bytes) => {
    const transfer = new DataTransfer();
    transfer.items.add(new File([new Uint8Array(bytes)], 'dropped.pdf', { type: 'application/pdf' }));
    for (const type of ['dragenter', 'dragover', 'drop']) {
      root.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: transfer }));
    }
  }, [...buildPdf([{ widthPt: 612, heightPt: 792 }])]);
  await expectOpened(page, 'dropped.pdf');
  const slot = (await pageSlot(page, 1).boundingBox())!;
  await dropFileAt(page, RED_PNG, slot.x + 100, slot.y + 150);
  const stamps = page.locator('.annotation-layer [data-kind="stamp"]');
  await expect(stamps).toHaveCount(1);
  await page.keyboard.press('ControlOrMeta+Z');
  await expect(stamps).toHaveCount(0);
});

