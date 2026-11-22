// The editor's keys in the app, where the whole page is the editor
// (`eventScope="window"`): Ctrl/⌘+Z works wherever focus is and whatever was
// clicked last, as it did before the editor learned to share a page (T-246).
// The embedded editor's narrower rules are in embed.spec.ts.

import { expect, test, type Page } from '@playwright/test';
import { buildPdf } from './pdf';
import { buildPng } from './png';
import { dropFile, dropFileAt, expectOpened, loadApp, pageSlot } from './support';

const RED_PNG = { name: 'red.png', mimeType: 'image/png', buffer: buildPng(200, 100, [220, 30, 30]) };
const stamps = (page: Page) => page.locator('.annotation-layer [data-kind="stamp"]');

/** Drops a one-page PDF on the window, then a PNG on its page: one stamp, nothing clicked. */
async function dropPdfAndStamp(page: Page): Promise<void> {
  await loadApp(page);
  await dropFile(page, 'keys.pdf', buildPdf([{ widthPt: 612, heightPt: 792 }]));
  await expectOpened(page, 'keys.pdf');
  const slot = (await pageSlot(page, 1).boundingBox())!;
  await dropFileAt(page, RED_PNG, slot.x + 100, slot.y + 150);
  await expect(stamps(page)).toHaveCount(1);
}

test('undoes a dropped stamp from the keyboard without a click first', async ({ page }) => {
  await dropPdfAndStamp(page);
  await page.keyboard.press('ControlOrMeta+Z');
  await expect(stamps(page)).toHaveCount(0);
});

test('undoes from the keyboard after a click on the site header', async ({ page }) => {
  await dropPdfAndStamp(page);
  // The header is outside the editor's root; the page is still all editor.
  await page.locator('.site-header').click({ position: { x: 4, y: 4 } });
  await page.keyboard.press('ControlOrMeta+Z');
  await expect(stamps(page)).toHaveCount(0);
});
