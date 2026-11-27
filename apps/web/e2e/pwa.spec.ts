// The installable, offline app (T-264): the production build registers
// dist/sw.js, which precaches the app shell, the engine worker and both WASM
// modules, so after one online visit the app opens and renders a PDF with the
// network off. Every other spec runs with service workers blocked
// (playwright.config.ts), so the zero-network test still sees each request.

import { expect, test, type Page } from '@playwright/test';
import { expectPagePainted, loadApp, openFixture } from './support';

/** Waits until the service worker controls the page and its precache is complete. */
async function waitForOfflineReady(page: Page): Promise<string[]> {
  await page.evaluate(() => navigator.serviceWorker.ready);
  await expect.poll(() => page.evaluate(() => navigator.serviceWorker.controller !== null), { timeout: 30_000 }).toBe(true);
  return page.evaluate(async () => {
    const names = await caches.keys();
    const urls: string[] = [];
    for (const name of names) {
      for (const request of await (await caches.open(name)).keys()) urls.push(request.url);
    }
    return urls;
  });
}

test.describe('offline', () => {
  test.use({ serviceWorkers: 'allow' });

  test('opens and renders a PDF offline after one online visit', async ({ page, context, baseURL, browserName }) => {
    await loadApp(page);
    const cached = await waitForOfflineReady(page);
    const origin = new URL(baseURL!).origin;
    // The shell, both WASM modules and the engine worker are in the cache…
    expect(cached).toContain(`${origin}/`);
    expect(cached.some((url) => /\/assets\/pdfium-[\w-]+\.wasm$/.test(url))).toBe(true);
    expect(cached.some((url) => /\/assets\/pdf_wasm_bg-[\w-]+\.wasm$/.test(url))).toBe(true);
    expect(cached.some((url) => /\/assets\/worker-[\w-]+\.js$/.test(url))).toBe(true);
    // …and only the app's own files.
    expect(cached.filter((url) => new URL(url).origin !== origin)).toEqual([]);

    await context.setOffline(true);
    // Really offline, the worker included. A URL the worker does not handle
    // would go from the page itself, proving nothing about the worker; so a
    // precached file is taken out of the cache first: the worker answers it
    // (a cached file still loads) and, missing it, fetches it itself — the
    // worker's own request to the network, which must fail. Both asked for
    // with `cache: 'no-cache'`, which the worker's fetch keeps, so the HTTP
    // cache cannot answer in the network's place.
    //
    // Not in Firefox: there an offline context fails no-store and reload
    // requests before the worker sees them, and answers the others from the
    // HTTP cache, stale or not, so the worker's own network cannot be told
    // apart. The rest of the test still runs offline there.
    if (browserName !== 'firefox') {
      const probe = await page.evaluate(async () => {
        const name = (await caches.keys()).find((key) => key.startsWith('vitela-'))!;
        if (!(await (await caches.open(name)).delete('/manifest.webmanifest'))) return { served: false, missing: 'not precached' };
        const served = await fetch('/icons/vitela-512.png', { cache: 'no-cache' }).then(
          (response) => response.ok,
          () => false,
        );
        const missing = await fetch('/manifest.webmanifest', { cache: 'no-cache' }).then(
          () => 'reached the network',
          () => 'offline',
        );
        return { served, missing };
      });
      expect(probe).toEqual({ served: true, missing: 'offline' });
    }
    await page.reload();
    await expect(page.locator('main.app')).toHaveAttribute('data-engine', 'ready', { timeout: 30_000 });
    await openFixture(page, 'forms/reportlab_acroform.pdf');
    await expectPagePainted(page, 1);

    // A tool route is the same shell, offline too.
    await page.goto('/organize');
    await expect(page.locator('main.app')).toHaveAttribute('data-engine', 'ready', { timeout: 30_000 });

    // The opened PDF never went into the cache: files stay on the device.
    const after = await waitForOfflineReady(page);
    expect(after.filter((url) => !cached.includes(url))).toEqual([]);
    expect(after.some((url) => /\.pdf$/i.test(url))).toBe(false);

    // A path the app does not route is the host's (its 404), never the
    // shell: offline, it goes to the network and fails.
    await expect(page.goto('/no-such-page')).rejects.toThrow();
  });
});

test('serves a valid web app manifest, linked from the page, with icons that load', async ({ page, request }) => {
  await page.goto('/');
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute('href', '/manifest.webmanifest');
  await expect(page.locator('meta[name="theme-color"]')).toHaveAttribute('content', '#1b2a57');

  const response = await request.get('/manifest.webmanifest');
  expect(response.status()).toBe(200);
  const manifest = await response.json();
  expect(manifest).toMatchObject({
    name: 'Vitela',
    short_name: 'Vitela',
    start_url: '/',
    scope: '/',
    display: 'standalone',
    theme_color: '#1b2a57',
    background_color: '#ffffff',
  });
  // Installable: a PNG of at least 144 px, and a maskable one.
  expect(manifest.icons.some((icon: { sizes: string; type: string }) => icon.type === 'image/png' && icon.sizes === '512x512')).toBe(
    true,
  );
  expect(manifest.icons.some((icon: { purpose: string }) => icon.purpose === 'maskable')).toBe(true);
  for (const icon of manifest.icons as Array<{ src: string; type: string }>) {
    const file = await request.get(icon.src);
    expect(file.status(), icon.src).toBe(200);
    expect(file.headers()['content-type'], icon.src).toContain(icon.type);
  }

  // The service worker is served from the root, so its scope is the whole site.
  const worker = await request.get('/sw.js');
  expect(worker.status()).toBe(200);
  expect(worker.headers()['content-type']).toContain('javascript');
});
