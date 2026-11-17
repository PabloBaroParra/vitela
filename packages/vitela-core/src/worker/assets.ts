// Where the worker fetches the WASM modules from, and how. By default the
// files ship next to the worker, and bundlers (Vite, webpack 5) follow these
// `new URL('…', import.meta.url)` literals and emit them as assets. A host
// that serves them from elsewhere (a CDN) names one base URL for all three;
// those copies are then checked against this package's own digests.

import type { InitMessage } from '../core/protocol.js';

/** The three files the engine loads. */
export type AssetName = 'engineWasm' | 'pdfiumGlue' | 'pdfiumWasm';

/** The three files the engine loads, as absolute URLs. */
export interface EngineAssetUrls {
  /** core/pdf-wasm's module (wasm-bindgen output). */
  readonly engineWasm: string;
  /** pdfium's Emscripten glue, evaluated as text (see pdfium.ts). */
  readonly pdfiumGlue: string;
  readonly pdfiumWasm: string;
}

/** Bare file names (not URLs), one per asset. */
export type AssetFileNames = Readonly<Record<AssetName, string>>;

/** Lower-case hex sha256 per file name, as written by scripts/write-integrity.mjs. */
export type AssetDigests = Readonly<Record<string, string>>;

/** The file names expected under a custom asset base URL. */
export const ASSET_FILE_NAMES: AssetFileNames = {
  engineWasm: 'pdf_wasm_bg.wasm',
  pdfiumGlue: 'pdfium.js',
  pdfiumWasm: 'pdfium.wasm',
};

/** Used when the init message carries no timeout. */
export const DEFAULT_LOAD_TIMEOUT_MS = 60_000;

/** Everything the loader needs: where, how long, and what to check. */
export interface AssetPlan {
  readonly urls: EngineAssetUrls;
  /** Per-file fetch budget, in milliseconds. */
  readonly timeoutMs: number;
  /** Expected sha256 per file name; absent for the bundled copies. */
  readonly integrity?: AssetDigests;
}

/**
 * Turns the client's init message into a load plan. Files from a custom base
 * must match `digests` (the ones this package version ships); the bundled
 * copies came with the code itself and are not checked.
 */
export function planAssets(init: InitMessage, digests: AssetDigests): AssetPlan {
  const timeoutMs = init.loadTimeoutMs ?? DEFAULT_LOAD_TIMEOUT_MS;
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
    throw new Error(`invalid loadTimeoutMs ${String(timeoutMs)}: must be a positive number`);
  }
  if (init.assetBaseUrl === undefined) return { urls: resolveAssetUrls(), timeoutMs };
  return { urls: resolveAssetUrls(init.assetBaseUrl), timeoutMs, integrity: digests };
}

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
  const absolute = absoluteBase(baseUrl);
  const base = absolute.endsWith('/') ? absolute : `${absolute}/`;
  return {
    engineWasm: new URL(ASSET_FILE_NAMES.engineWasm, base).href,
    pdfiumGlue: new URL(ASSET_FILE_NAMES.pdfiumGlue, base).href,
    pdfiumWasm: new URL(ASSET_FILE_NAMES.pdfiumWasm, base).href,
  };
}

function absoluteBase(baseUrl: string): string {
  try {
    return new URL(baseUrl).href;
  } catch {
    throw new Error(`invalid assetBaseUrl "${baseUrl}": must be an absolute URL`);
  }
}
