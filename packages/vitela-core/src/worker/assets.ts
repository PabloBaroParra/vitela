// Where the worker fetches the WASM modules from. By default the files ship
// next to the worker, and bundlers (Vite, webpack 5) follow these
// `new URL('…', import.meta.url)` literals and emit them as assets. A host
// that serves them from elsewhere (a CDN) names one base URL for all three.

/** The three files the engine loads, as absolute URLs. */
export interface EngineAssetUrls {
  /** core/pdf-wasm's module (wasm-bindgen output). */
  readonly engineWasm: string;
  /** pdfium's Emscripten glue, evaluated as text (see pdfium.ts). */
  readonly pdfiumGlue: string;
  readonly pdfiumWasm: string;
}

/** The file names expected under a custom asset base URL. */
export const ASSET_FILE_NAMES: EngineAssetUrls = {
  engineWasm: 'pdf_wasm_bg.wasm',
  pdfiumGlue: 'pdfium.js',
  pdfiumWasm: 'pdfium.wasm',
};

/** The bundled copies, or the files under `baseUrl` (an absolute URL) when given. */
export function resolveAssetUrls(baseUrl?: string): EngineAssetUrls {
  if (baseUrl === undefined) {
    return {
      engineWasm: new URL('../generated/engine/pdf_wasm_bg.wasm', import.meta.url).href,
      pdfiumGlue: new URL('../generated/pdfium/pdfium.js', import.meta.url).href,
      pdfiumWasm: new URL('../generated/pdfium/pdfium.wasm', import.meta.url).href,
    };
  }
  // A base without a trailing slash names a directory, not a file in its parent.
  const base = baseUrl.endsWith('/') ? baseUrl : `${baseUrl}/`;
  return {
    engineWasm: new URL(ASSET_FILE_NAMES.engineWasm, base).href,
    pdfiumGlue: new URL(ASSET_FILE_NAMES.pdfiumGlue, base).href,
    pdfiumWasm: new URL(ASSET_FILE_NAMES.pdfiumWasm, base).href,
  };
}
