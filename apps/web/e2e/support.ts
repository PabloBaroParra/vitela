// Shared E2E steps. Fixtures come from the repo's tests/fixtures (or are
// built in memory by pdf.ts) and are handed to the file input or a drop
// event directly — never served over HTTP.

import { readFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { expect, type Locator, type Page } from '@playwright/test';

export const FIXTURES = resolve(import.meta.dirname, '../../../tests/fixtures');

/** Loads the app and waits until both WASM modules are up. */
export async function loadApp(page: Page): Promise<void> {
  await page.goto('/');
  await expect(page.locator('main.app')).toHaveAttribute('data-engine', 'ready', { timeout: 30_000 });
}

/** The slot of page `number` (1-based). */
export function pageSlot(page: Page, number: number): Locator {
  return page.locator(`.page-slot[data-page="${number}"]`);
}

/** Waits until page `number` shows a bitmap rendered at the current zoom. */
export async function expectPagePainted(page: Page, number: number): Promise<void> {
  const slot = pageSlot(page, number);
  await expect(slot).toHaveAttribute('data-state', 'rendered', { timeout: 30_000 });
  await expect(slot.locator('canvas.page-canvas')).toHaveAttribute('data-painted', 'true', { timeout: 30_000 });
}

/** Waits for `name` to be the open document and for its first page's pixels. */
export async function expectOpened(page: Page, name: string): Promise<void> {
  // Wait for THIS document first, so a previous document's canvas cannot
  // satisfy the painted check below.
  await expect(page.getByText(`${name} —`)).toBeVisible({ timeout: 30_000 });
  await expectPagePainted(page, 1);
}

/** Opens a fixture through the "Open PDF" picker and waits for page 1's pixels. */
export async function openFixture(page: Page, relativePath: string): Promise<void> {
  await page.getByLabel('Open PDF').setInputFiles(resolve(FIXTURES, relativePath));
  await expectOpened(page, basename(relativePath));
}

/** Opens in-memory bytes through the picker and waits for page 1's pixels. */
export async function openBytes(page: Page, name: string, buffer: Buffer): Promise<void> {
  await page.getByLabel('Open PDF').setInputFiles({ name, mimeType: 'application/pdf', buffer });
  await expectOpened(page, name);
}

export function fixtureBytes(relativePath: string): Buffer {
  return readFileSync(resolve(FIXTURES, relativePath));
}

/** Drops a file onto the window the way a file manager drag does. */
export async function dropFile(page: Page, name: string, buffer: Buffer): Promise<void> {
  await page.evaluate(
    ({ name, bytes }) => {
      const transfer = new DataTransfer();
      transfer.items.add(new File([new Uint8Array(bytes)], name, { type: 'application/pdf' }));
      const target = document.querySelector('main.app')!;
      for (const type of ['dragenter', 'dragover', 'drop']) {
        target.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: transfer }));
      }
    },
    { name, bytes: [...buffer] },
  );
}

/** Counts the canvas pixels of page `number` that are clearly not paper-white. */
export function inkedPixelCount(page: Page, number = 1): Promise<{ width: number; height: number; inked: number }> {
  return pageSlot(page, number)
    .locator('canvas.page-canvas')
    .evaluate((node) => {
      const canvas = node as HTMLCanvasElement;
      const { data } = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height);
      let inked = 0;
      for (let i = 0; i < data.length; i += 4) {
        if (data[i]! < 200 || data[i + 1]! < 200 || data[i + 2]! < 200) inked++;
      }
      return { width: canvas.width, height: canvas.height, inked };
    });
}
