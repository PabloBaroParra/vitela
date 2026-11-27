// Print (T-257): what a save would write now, one image per page, in a
// hidden print frame inside the editor that prints itself through the
// browser's dialog. The dialog is stubbed (an init script replaces the
// frame's `print` and the page's own): each call records what the frame
// held — its images, in order, their pixel sizes and ink, and its page
// sizes — so a test reads exactly what would have gone to the printer.

import { expect, test, type Page } from '@playwright/test';
import { HOST_PORT } from './host/vite.config';
import { buildPdf, COPY_PROHIBITED, type PageSpec } from './pdf';
import { pageMediaBoxes } from './pdfObjects';
import { expectOpened, expectPagePainted, loadApp, openBytes, openTool } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792, lines: ['Printed page'] };
const HOST = `http://127.0.0.1:${HOST_PORT}/`;
/** `/P` granting everything but printing (bit 3) and copying (bit 5). */
const PRINT_PROHIBITED = COPY_PROHIBITED & ~(1 << 2);

interface PrintedImage {
  readonly alt: string;
  readonly width: number;
  readonly height: number;
  /** Pixels that are not white. */
  readonly inked: number;
  readonly sheet: string;
}

interface Printed {
  /** `true` when the page itself was printed, not the editor's frame. */
  readonly top: boolean;
  readonly title?: string;
  readonly css?: string;
  readonly images?: PrintedImage[];
  /** Whether the frame sits inside the editor's root. */
  readonly insideEditor?: boolean;
}

declare global {
  interface Window {
    __prints: Printed[];
  }
}

/** Stubs every print dialog on the page: the frame's and the page's own. */
async function stubPrinting(page: Page): Promise<void> {
  await page.addInitScript(() => {
    if (window !== window.top) return;
    window.__prints = [];
    window.print = () => void window.__prints.push({ top: true });
    const record = (frame: HTMLIFrameElement) => {
      const doc = frame.contentDocument!;
      const images = [...doc.querySelectorAll('img')].map((image) => {
        const canvas = document.createElement('canvas');
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const context = canvas.getContext('2d')!;
        context.drawImage(image, 0, 0);
        const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
        let inked = 0;
        for (let i = 0; i < data.length; i += 4) if (data[i]! < 200 || data[i + 1]! < 200 || data[i + 2]! < 200) inked++;
        return { alt: image.alt, width: image.naturalWidth, height: image.naturalHeight, inked, sheet: image.parentElement!.className };
      });
      window.__prints.push({
        top: false,
        title: doc.title,
        css: doc.querySelector('style')?.textContent ?? '',
        images,
        insideEditor: !!frame.closest('.vitela'),
      });
    };
    new MutationObserver((records) => {
      for (const { addedNodes } of records) {
        for (const node of addedNodes) {
          if (!(node instanceof HTMLIFrameElement) || !node.classList.contains('vitela-print-frame')) continue;
          // After the editor's own load listener: it prints only once its images are decoded.
          node.addEventListener('load', () => {
            node.contentWindow!.print = () => record(node);
          });
        }
      }
    }).observe(document, { childList: true, subtree: true });
  });
}

const prints = (page: Page) => page.evaluate(() => window.__prints);
const printFrame = (page: Page) => page.locator('iframe.vitela-print-frame');

/** Prints from the Tools menu and returns what the frame held. */
async function printFromMenu(page: Page): Promise<Printed> {
  const before = (await prints(page)).length;
  await openTool(page, 'Print');
  await expect.poll(async () => (await prints(page)).length, { timeout: 30_000 }).toBe(before + 1);
  return (await prints(page)).at(-1)!;
}

/** The dialog closed: what the browser reports after printing, printed or not. */
async function closeDialog(page: Page): Promise<void> {
  await printFrame(page).evaluate((frame) => (frame as HTMLIFrameElement).contentWindow!.dispatchEvent(new Event('afterprint')));
  await expect(printFrame(page)).toHaveCount(0);
}

test('prints every page as one image, in order, each on a sheet of its own size and orientation', async ({ page }) => {
  await stubPrinting(page);
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER, { ...LETTER, rotate: 90 }, LETTER]));

  const printed = await printFromMenu(page);
  expect(printed.top).toBe(false);
  expect(printed.insideEditor).toBe(true);
  expect(printed.title).toBe('report');
  // 150 DPI: Letter is 1275 x 1650 px, and the turned page is landscape.
  expect(printed.images!.map(({ alt, width, height }) => [alt, width, height])).toEqual([
    ['Page 1', 1275, 1650],
    ['Page 2', 1650, 1275],
    ['Page 3', 1275, 1650],
  ]);
  expect(printed.images!.map(({ sheet }) => sheet)).toEqual(['sheet sheet--0', 'sheet sheet--1', 'sheet sheet--0']);
  expect(printed.css).toContain('@page vitela-sheet-0 { size: 612pt 792pt; margin: 0; }');
  expect(printed.css).toContain('@page vitela-sheet-1 { size: 792pt 612pt; margin: 0; }');
  for (const image of printed.images!) expect(image.inked).toBeGreaterThan(0);
  // Nothing in the frame can be reached by keyboard or a screen reader.
  await expect(printFrame(page)).toHaveAttribute('aria-hidden', 'true');
  await expect(printFrame(page)).toHaveAttribute('tabindex', '-1');
  await expect(page.getByRole('status').filter({ hasText: 'print' })).toHaveCount(0);

  await closeDialog(page);
});

test('the print frame lays out one paper sheet per page, landscape for the turned one', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'Only Chromium can print a page to PDF in Playwright.');
  await stubPrinting(page);
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER, { ...LETTER, rotate: 90 }]));
  await printFromMenu(page);

  // The frame's own document, its images inlined, printed on its own: what the dialog would lay out.
  const html = await printFrame(page).evaluate(async (frame) => {
    const doc = (frame as HTMLIFrameElement).contentDocument!;
    const copy = doc.documentElement.cloneNode(true) as HTMLElement;
    const sources = [...doc.querySelectorAll('img')].map((image) => image.src);
    const inlined = await Promise.all(
      sources.map(async (src) => {
        const blob = await (await fetch(src)).blob();
        return new Promise<string>((resolve) => {
          const reader = new FileReader();
          reader.onload = () => resolve(reader.result as string);
          reader.readAsDataURL(blob);
        });
      }),
    );
    copy.querySelectorAll('img').forEach((image, i) => image.setAttribute('src', inlined[i]!));
    return `<!doctype html>${copy.outerHTML}`;
  });
  const sheetPage = await page.context().newPage();
  await sheetPage.setContent(html);
  const pdf = await sheetPage.pdf({ preferCSSPageSize: true });
  await sheetPage.close();
  const boxes = pageMediaBoxes(pdf).map((box) => [Math.round(box[2]! - box[0]!), Math.round(box[3]! - box[1]!)]);
  expect(boxes).toEqual([
    [612, 792],
    [792, 612],
  ]);
});

test('prints what a save would write now: a pending watermark is on the paper, and the document stays unsaved', async ({ page }) => {
  await stubPrinting(page);
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER]));
  const plain = (await printFromMenu(page)).images![0]!.inked;
  await closeDialog(page);

  await openTool(page, 'Watermark');
  const dialog = page.getByRole('dialog', { name: 'Add watermark' });
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  await dialog.getByRole('slider', { name: 'Opacity' }).fill('100');
  await dialog.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByText('Watermark added to 1 page.')).toBeVisible();

  const stamped = (await printFromMenu(page)).images![0]!.inked;
  expect(stamped).toBeGreaterThan(plain);
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  await closeDialog(page);
});

test('Ctrl+P with focus in the editor prints its pages, not the app around them', async ({ page }) => {
  await stubPrinting(page);
  await loadApp(page);
  await openBytes(page, 'keys.pdf', buildPdf([LETTER, LETTER]));
  await page.getByRole('button', { name: 'Select', exact: true }).focus();
  await page.keyboard.press('ControlOrMeta+P');
  await expect.poll(async () => (await prints(page)).length, { timeout: 30_000 }).toBe(1);
  const [printed] = await prints(page);
  expect(printed!.top).toBe(false);
  expect(printed!.images).toHaveLength(2);
  await closeDialog(page);
});

test('a document whose permissions withhold printing still prints, as on every native shell', async ({ page }) => {
  // The core has no rule for the print bit, and no native shell asks one before printing.
  const locked = buildPdf([LETTER], { encrypt: { permissions: PRINT_PROHIBITED, userPassword: 'user-e2e' } });
  await stubPrinting(page);
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: locked });
  const prompt = page.getByRole('dialog', { name: 'Password required' });
  await expect(prompt).toBeVisible({ timeout: 30_000 });
  await prompt.getByLabel('Password').fill('user-e2e');
  await prompt.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');

  const printed = await printFromMenu(page);
  expect(printed.images!.map(({ width, height }) => [width, height])).toEqual([[1275, 1650]]);
  await closeDialog(page);
});

test.describe('embedded in a host page', () => {
  test('Ctrl+P prints only the editor pages, and only with the key in the editor', async ({ page }) => {
    await stubPrinting(page);
    await page.goto(HOST);
    await expect(page.getByLabel('Open PDF')).toBeAttached({ timeout: 30_000 });
    await openBytes(page, 'embedded.pdf', buildPdf([LETTER]));
    await expectPagePainted(page, 1);

    // Focus on a host control: the key is the host's (the browser's own Print), the editor prints nothing.
    await page.locator('#host-button').focus();
    await page.keyboard.press('ControlOrMeta+P');
    await expect(printFrame(page)).toHaveCount(0);
    expect((await prints(page)).filter(({ top }) => !top)).toEqual([]);

    // A click in the editor's bar: the key is the editor's.
    await page.locator('.workspace__bar').getByText('embedded.pdf —').click();
    await page.keyboard.press('ControlOrMeta+P');
    await expect.poll(async () => (await prints(page)).filter(({ top }) => !top).length, { timeout: 30_000 }).toBe(1);
    const printed = (await prints(page)).find(({ top }) => !top)!;
    expect(printed.insideEditor).toBe(true);
    expect(printed.images).toHaveLength(1);
    // The host's page was never printed by the editor, and its styles never reached the frame.
    expect((await prints(page)).filter(({ top }) => top)).toEqual([]);
    await expect(page.locator('#host-frame iframe.vitela-print-frame')).toHaveCount(1);
    const frameStyles = await printFrame(page).evaluate((frame) => {
      const doc = (frame as HTMLIFrameElement).contentDocument!;
      return { sheets: doc.styleSheets.length, links: doc.querySelectorAll('link').length, bodyMargin: getComputedStyle(doc.body).margin };
    });
    expect(frameStyles).toEqual({ sheets: 1, links: 0, bodyMargin: '0px' });
    await closeDialog(page);
  });
});
