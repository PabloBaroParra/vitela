// The web equivalent of tools/zero-network-guard: once the app has loaded,
// it must never touch the network again. The user's PDF never leaves the
// machine, and nothing is fetched on its behalf.

import { expect, test, type Request } from '@playwright/test';
import { loadApp, openFixture } from './support';

test('makes no network request after the initial load', async ({ page, context, baseURL }) => {
  const duringLoad: string[] = [];
  const afterLoad: string[] = [];
  let loaded = false;
  // Context-level, so requests from the engine's Web Worker are seen too.
  context.on('request', (request: Request) => (loaded ? afterLoad : duringLoad).push(request.url()));

  await loadApp(page);
  loaded = true;

  // The initial load itself stays on the app's own origin.
  const origin = new URL(baseURL!).origin;
  // Proof the listener sees worker traffic in this browser: pdfium.wasm is
  // fetched from inside the engine worker. Without this the assertion below
  // could pass by being blind.
  expect(duringLoad.some((url) => /\/pdfium-[\w-]+\.wasm$/.test(url))).toBe(true);
  expect(duringLoad.filter((url) => new URL(url).origin !== origin)).toEqual([]);

  // Exercise the app: open and render a document, then give stragglers time.
  await openFixture(page, 'forms/reportlab_acroform.pdf');
  await openFixture(page, 'content-edit/reportlab_embedded_subset.pdf');
  await page.waitForTimeout(1_000);

  expect(afterLoad).toEqual([]);
});
