// The Content-Security-Policy (T-265): `vite preview` serves public/_headers
// (scripts/hostHeaders.ts), so the page and the engine worker run under the
// policies that ship, and every spec in the suite runs under them. This one
// walks a representative flow — open, render, annotate, the Edit content
// mode with a retype, save and print — and fails on any violation the
// browser reports, in the page or in a frame (the print frame), as a
// `securitypolicyviolation` event or a console error.

import { expect, test, type Page } from '@playwright/test';
import { saveAsDownload } from './organize';
import { LINE_FONT_PT, buildPdf, lineBaseline, type PageSpec } from './pdf';
import { expectOpened, expectPagePainted, loadApp, openBytes, openTool, pageSlot, toolsButton } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792, lines: ['Vitela content line', 'a line to highlight'] };
/** CSS px per point at 100%. */
const SCALE = 96 / 72;

declare global {
  interface Window {
    __cspViolations: string[];
  }
}

/** Records every CSP violation in the page and its frames, and every console line that reports one. */
async function watchViolations(page: Page): Promise<string[]> {
  const reported: string[] = [];
  page.on('console', (message) => {
    if (/content[- ]security[- ]policy|\bCSP\b/i.test(message.text())) reported.push(`console: ${message.text()}`);
  });
  page.on('pageerror', (error) => {
    if (/content[- ]security[- ]policy|\bCSP\b|unsafe-eval|unsafe-inline/i.test(error.message)) reported.push(`error: ${error.message}`);
  });
  await page.addInitScript(() => {
    const top = window.top as Window;
    try {
      top.__cspViolations ??= [];
    } catch {
      return;
    }
    document.addEventListener('securitypolicyviolation', (event) => {
      top.__cspViolations.push(`${event.effectiveDirective} blocked ${event.blockedURI || 'inline'} (${event.sourceFile}:${event.lineNumber})`);
    });
  });
  return reported;
}

async function expectNoViolations(page: Page, reported: readonly string[]): Promise<void> {
  const events = await page.evaluate(() => window.__cspViolations ?? []);
  expect([...events, ...reported]).toEqual([]);
}

/** Replaces the print frame's dialog with a counter, once the frame is in. */
async function stubPrinting(page: Page): Promise<void> {
  await page.addInitScript(() => {
    if (window !== window.top) return;
    (window as unknown as { __printed: number }).__printed = 0;
    new MutationObserver((records) => {
      for (const { addedNodes } of records) {
        for (const node of addedNodes) {
          if (!(node instanceof HTMLIFrameElement) || !node.classList.contains('vitela-print-frame')) continue;
          node.addEventListener('load', () => {
            node.contentWindow!.print = () => void (window as unknown as { __printed: number }).__printed++;
          });
        }
      }
    }).observe(document, { childList: true, subtree: true });
  });
}

/** (x, y) points on page 1 as drawn, in viewport coordinates. */
async function at(page: Page, x: number, y: number): Promise<{ x: number; y: number }> {
  const box = (await pageSlot(page, 1).boundingBox())!;
  return { x: box.x + x * SCALE, y: box.y + y * SCALE };
}

const lineMiddle = (index: number) => LETTER.heightPt - lineBaseline(LETTER, index) - LINE_FONT_PT / 2;

test('serves the page and the engine worker each under its own policy', async ({ request }) => {
  const page = (await request.get('/')).headers()['content-security-policy'];
  expect(page).toContain("script-src 'self';");
  expect(page).toContain("frame-ancestors 'none'");
  expect(page).not.toContain('unsafe');
  // A tool route is the same shell under the same policy.
  expect((await request.get('/edit')).headers()['content-security-policy']).toBe(page);

  const html = await (await request.get('/')).text();
  const script = html.match(/src="(\/assets\/index-[\w-]+\.js)"/)![1]!;
  const bundle = await (await request.get(script)).text();
  const worker = bundle.match(/\/assets\/worker-[\w-]+\.js/)![0];
  const workerPolicy = (await request.get(worker)).headers()['content-security-policy'];
  expect(workerPolicy).toContain("'wasm-unsafe-eval'");
  expect(workerPolicy).not.toContain(',');
});

test('opens, renders, annotates, edits content, saves and prints with no CSP violation', async ({ page }) => {
  const reported = await watchViolations(page);
  await stubPrinting(page);
  await loadApp(page);
  await openBytes(page, 'csp.pdf', buildPdf([LETTER, LETTER]));
  await expectOpened(page, 'csp.pdf');
  await expectPagePainted(page, 1);

  // Annotate: a highlight over a word of the second line.
  const word = await at(page, 100, LETTER.heightPt - lineBaseline(LETTER, 1) - 6);
  await page.mouse.dblclick(word.x, word.y);
  await expect(pageSlot(page, 1).locator('.text-selection')).toHaveCount(1);
  await page.getByRole('button', { name: 'Highlight', exact: true }).click();
  await expect(page.locator('.annotation-layer [data-kind="highlight"]')).toHaveCount(1);

  // Edit content: the first line retyped in the camouflaged editor (the PDF's own font).
  const mode = page.getByRole('group', { name: 'Edit content' });
  await toolsButton(page).click();
  const item = page.getByRole('menu', { name: 'Tools' }).getByRole('menuitemcheckbox', { name: 'Edit content' });
  await expect(item).not.toHaveAttribute('aria-disabled', 'true', { timeout: 30_000 });
  await item.click();
  await expect(pageSlot(page, 1).locator('.content-layer')).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
  const run = await at(page, 90, lineMiddle(0));
  await page.mouse.click(run.x, run.y);
  await expect(mode.getByRole('status')).toHaveText('Text “Vitela content line” selected on page 1.');
  await page.keyboard.press('Enter');
  const editor = page.getByRole('textbox', { name: 'Edit text “Vitela content line”' });
  await editor.fill('Vitela strict line');
  await editor.press('Enter');
  await expect(mode.getByRole('status')).toHaveText('Text “Vitela strict line” selected on page 1.', { timeout: 30_000 });
  await mode.getByRole('button', { name: 'Done' }).click();
  await expect(mode).toHaveCount(0);

  // Save: a download of the edited document.
  const { bytes } = await saveAsDownload(page);
  expect(bytes.subarray(0, 5).toString('latin1')).toBe('%PDF-');

  // Print: the hidden frame fills with every page and opens the (stubbed) dialog.
  await openTool(page, 'Print');
  await expect.poll(() => page.evaluate(() => (window as unknown as { __printed: number }).__printed), { timeout: 30_000 }).toBe(1);
  await expect(page.locator('iframe.vitela-print-frame').contentFrame().locator('img')).toHaveCount(2);

  await expectNoViolations(page, reported);
});
