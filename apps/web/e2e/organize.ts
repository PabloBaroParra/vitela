// Shared Organize steps for the E2E specs (organize.spec.ts, import.spec.ts).
//
// Pages are told apart by their shape: the specs build documents whose pages
// are all `SHAPE_HEIGHT` pt tall and differ in width, the grid draws each
// thumbnail at its page's aspect ratio, and a turned page swaps it.

import { readFileSync } from 'node:fs';
import { expect, type Download, type Locator, type Page } from '@playwright/test';

/** The height every page of a shapes document has, before turning. */
export const SHAPE_HEIGHT = 700;

export const cards = (page: Page) => page.locator('.page-card');
export const grid = (page: Page) => page.getByRole('listbox', { name: /^(Pages of|Documents in) / });
export const toolbar = (page: Page) => page.getByRole('toolbar', { name: 'Organize' });
export const tool = (page: Page, name: string) => toolbar(page).getByRole('button', { name, exact: true });

export async function openOrganize(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Organize' }).click();
  await expect(grid(page)).toBeVisible();
  await expectThumbnailsRendered(page);
}

export async function expectThumbnailsRendered(page: Page): Promise<void> {
  await expect(cards(page).first()).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.page-card:not([data-state="rendered"])')).toHaveCount(0, { timeout: 30_000 });
}

/**
 * Each card's page as `width` pt, or `width r` when it is turned a quarter:
 * the width read back from the thumbnail's drawn aspect ratio.
 */
export async function shapes(page: Page): Promise<string[]> {
  const boxes = await page.locator('.page-card').evaluateAll((nodes) =>
    nodes.map((node) => {
      const box = node.querySelector<HTMLElement>('.page-card__page')!.getBoundingClientRect();
      return { ratio: box.width / box.height, rotation: Number((node as HTMLElement).dataset.rotation) };
    }),
  );
  return boxes.map(({ ratio, rotation }) => {
    const turned = rotation % 180 !== 0;
    // A turned page is drawn SHAPE_HEIGHT wide: its own width is now its height.
    const width = Math.round((turned ? SHAPE_HEIGHT / ratio : SHAPE_HEIGHT * ratio) / 100) * 100;
    return turned ? `${width} r` : `${width}`;
  });
}

export const saveButton = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });

export async function saveAsDownload(page: Page): Promise<{ download: Download; bytes: Buffer }> {
  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  return { download, bytes: readFileSync(await download.path()) };
}

/** Drags `from` so it drops `dx` px right of the middle of `onto`. */
export async function drag(page: Page, from: Locator, onto: Locator, dx: number): Promise<void> {
  const start = (await from.boundingBox())!;
  const end = (await onto.boundingBox())!;
  await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2);
  await page.mouse.down();
  await page.mouse.move(start.x + start.width / 2 + 20, start.y + start.height / 2, { steps: 4 });
  await page.mouse.move(end.x + end.width / 2 + dx, end.y + end.height / 2, { steps: 8 });
  await page.mouse.up();
}
