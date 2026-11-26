// The document bar on a phone (≤ 600 px): one row (every pixel it grows
// comes off the pages and the Comments panel), the document tools folded
// into one "More tools" menu button, and the tab order equal to the order on
// screen (WCAG 2.4.3) — the menu sits where the tools sit in the DOM.

import { expect, test, type Page } from '@playwright/test';
import { buildPdf } from './pdf';
import { loadApp, openBytes } from './support';

const LETTER = { widthPt: 612, heightPt: 792 };
const bar = (page: Page) => page.locator('.workspace__bar');
const more = (page: Page) => bar(page).getByRole('button', { name: 'More tools' });

async function openOnPhone(page: Page): Promise<void> {
  await page.setViewportSize({ width: 390, height: 600 });
  await loadApp(page);
  await openBytes(page, 'plain.pdf', buildPdf([LETTER, LETTER]));
}

test('at 390 px the bar is one row: Open, the name, the view switch, More tools and Save, in DOM order', async ({ page }) => {
  await openOnPhone(page);
  await expect(more(page)).toBeVisible();
  await expect(bar(page).getByRole('button', { name: 'Save', exact: true })).toBeVisible();
  await expect(bar(page).getByRole('button', { name: 'Compress', exact: true })).toBeHidden();
  await expect(bar(page).getByRole('button', { name: 'Export images', exact: true })).toBeHidden();

  // Every visible child of the bar, in DOM order.
  const boxes = await bar(page).evaluate((node) =>
    [...node.children]
      .map((child) => child.getBoundingClientRect())
      .filter((box) => box.width > 0 && box.height > 0)
      .map((box) => ({ left: box.left, right: box.right, top: box.top, bottom: box.bottom })),
  );
  expect(boxes.length).toBeGreaterThanOrEqual(5);
  // One row: all of them share a horizontal band.
  expect(Math.max(...boxes.map((box) => box.top))).toBeLessThan(Math.min(...boxes.map((box) => box.bottom)));
  // The order on screen is the DOM order, and nothing spills past the viewport.
  for (let i = 1; i < boxes.length; i++) expect(boxes[i]!.left).toBeGreaterThanOrEqual(boxes[i - 1]!.right - 1);
  expect(boxes.at(-1)!.right).toBeLessThanOrEqual(390);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);

  // Tab walks them in that order: Organize, More tools, Save.
  await bar(page).getByRole('button', { name: 'Organize' }).focus();
  await page.keyboard.press('Tab');
  await expect(more(page)).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(bar(page).getByRole('button', { name: 'Save', exact: true })).toBeFocused();
});

test('More tools works from the keyboard and gives the focus back after the dialog', async ({ page }) => {
  await openOnPhone(page);
  await more(page).focus();
  await page.keyboard.press('Enter');
  const menu = page.getByRole('menu', { name: 'More tools' });
  await expect(menu).toBeVisible();
  await expect(more(page)).toHaveAttribute('aria-expanded', 'true');
  await expect(menu.getByRole('menuitem')).toHaveText(['Compress', 'Export images']);
  await expect(menu.getByRole('menuitem', { name: 'Compress' })).toBeFocused();

  await page.keyboard.press('ArrowDown');
  await expect(menu.getByRole('menuitem', { name: 'Export images' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(menu).toHaveCount(0);
  const dialog = page.getByRole('dialog', { name: 'Export pages as images' });
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(more(page)).toBeFocused();

  // Escape closes the menu itself, back on its button; a tap opens Compress.
  await page.keyboard.press('ArrowDown');
  await expect(menu).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
  await expect(more(page)).toBeFocused();
  await more(page).click();
  await menu.getByRole('menuitem', { name: 'Compress' }).click();
  await expect(page.getByRole('dialog', { name: 'Compress PDF' })).toBeVisible({ timeout: 30_000 });
});
