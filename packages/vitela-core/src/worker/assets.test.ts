import { describe, expect, it } from 'vitest';
import { resolveAssetUrls } from './assets.js';

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
