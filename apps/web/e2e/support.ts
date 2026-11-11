// Shared E2E steps. Fixtures come from the repo's tests/fixtures and are
// handed to the file input directly — never served over HTTP.

import { basename, resolve } from 'node:path';
import { expect, type Page } from '@playwright/test';

export const FIXTURES = resolve(import.meta.dirname, '../../../tests/fixtures');

/** Loads the app and waits until both WASM modules are up. */
export async function loadApp(page: Page): Promise<void> {
  await page.goto('/');
  await expect(page.locator('main.app')).toHaveAttribute('data-engine', 'ready', { timeout: 30_000 });
}

/** Opens a fixture through the "Open PDF" picker and waits for page 1's pixels. */
export async function openFixture(page: Page, relativePath: string): Promise<void> {
  await page.getByLabel('Open PDF').setInputFiles(resolve(FIXTURES, relativePath));
  // Wait for THIS document first, so a previous document's canvas cannot
  // satisfy the painted check below.
  await expect(page.getByText(`${basename(relativePath)} —`)).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('canvas.page-canvas')).toHaveAttribute('data-painted', 'true', {
    timeout: 30_000,
  });
}

/** Counts the canvas pixels that are clearly not paper-white. */
export function inkedPixelCount(page: Page): Promise<{ width: number; height: number; inked: number }> {
  return page.locator('canvas.page-canvas').evaluate((node) => {
    const canvas = node as HTMLCanvasElement;
    const { data } = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height);
    let inked = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i]! < 200 || data[i + 1]! < 200 || data[i + 2]! < 200) inked++;
    }
    return { width: canvas.width, height: canvas.height, inked };
  });
}
