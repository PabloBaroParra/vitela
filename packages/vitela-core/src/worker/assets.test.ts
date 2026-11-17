import { describe, expect, it } from 'vitest';
import { DEFAULT_LOAD_TIMEOUT_MS, planAssets, resolveAssetUrls } from './assets.js';

describe('resolveAssetUrls', () => {
  it('points at the copies bundled next to the worker by default', () => {
    const urls = resolveAssetUrls();
    expect(urls.engineWasm).toMatch(/\/generated\/engine\/pdf_wasm_bg\.wasm$/);
    expect(urls.pdfiumGlue).toMatch(/\/generated\/pdfium\/pdfium\.js$/);
    expect(urls.pdfiumWasm).toMatch(/\/generated\/pdfium\/pdfium\.wasm$/);
  });

  it('serves every file from a custom base URL', () => {
    expect(resolveAssetUrls('https://cdn.example/vitela/0.1.0/')).toEqual({
      engineWasm: 'https://cdn.example/vitela/0.1.0/pdf_wasm_bg.wasm',
      pdfiumGlue: 'https://cdn.example/vitela/0.1.0/pdfium.js',
      pdfiumWasm: 'https://cdn.example/vitela/0.1.0/pdfium.wasm',
    });
  });

  it('treats a base without a trailing slash as a directory', () => {
    expect(resolveAssetUrls('https://cdn.example/vitela').pdfiumWasm).toBe('https://cdn.example/vitela/pdfium.wasm');
  });
});

describe('planAssets', () => {
  const digests = { 'pdfium.wasm': 'ab'.repeat(32) };

  it('loads the bundled copies unchecked, with the default timeout', () => {
    expect(planAssets({ type: 'init' }, digests)).toEqual({
      urls: resolveAssetUrls(),
      timeoutMs: DEFAULT_LOAD_TIMEOUT_MS,
    });
  });

  it("checks files from a custom base against the package's digests", () => {
    const plan = planAssets({ type: 'init', assetBaseUrl: 'https://cdn.example/v/', loadTimeoutMs: 5 }, digests);
    expect(plan).toEqual({
      urls: resolveAssetUrls('https://cdn.example/v/'),
      timeoutMs: 5,
      integrity: digests,
    });
  });

  it('rejects a base that is not an absolute URL', () => {
    expect(() => planAssets({ type: 'init', assetBaseUrl: 'wasm/' }, digests)).toThrow(
      'invalid assetBaseUrl "wasm/": must be an absolute URL',
    );
  });

  it('rejects a timeout that is not a positive number', () => {
    expect(() => planAssets({ type: 'init', loadTimeoutMs: 0 }, digests)).toThrow('invalid loadTimeoutMs 0');
  });
});
