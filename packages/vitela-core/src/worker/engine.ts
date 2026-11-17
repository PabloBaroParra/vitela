// The engine behind the worker: pdfium (Emscripten) + core/pdf-wasm
// (wasm-bindgen). This file is the ONLY place that imports the Rust module;
// everything else talks to the `Engine` interface in core/types.ts.

import initEngine, * as core from '../generated/engine/pdf_wasm.js';
import type { Engine } from '../core/types.js';
import { ASSET_FILE_NAMES, type AssetName, type AssetPlan } from './assets.js';
import { documentEngine } from './documentEngine.js';
import { fetchAsset } from './fetchAsset.js';
import { verifyAsset } from './integrity.js';
import { instantiatePdfium } from './pdfium.js';

/** Loads both WASM modules, binds pdfium-render to pdfium, starts the core. */
export async function loadEngine(plan: AssetPlan): Promise<Engine> {
  // Fetched here rather than by wasm-bindgen / Emscripten, so every file gets
  // the same deadline and error messages, and is verified before it runs.
  const [engineWasm, pdfiumGlue, pdfiumWasm] = await Promise.all([
    loadAsset(plan, 'engineWasm'),
    loadAsset(plan, 'pdfiumGlue'),
    loadAsset(plan, 'pdfiumWasm'),
  ]);
  const [pdfium, engineExports] = await Promise.all([
    instantiatePdfium(new TextDecoder().decode(pdfiumGlue), new Uint8Array(pdfiumWasm)),
    initEngine({ module_or_path: engineWasm }),
  ]);
  if (!core.initialize_pdfium_render(pdfium, engineExports, false)) {
    throw new Error('initialize_pdfium_render refused the pdfium module');
  }
  return documentEngine(new core.Engine(), { placeRects: core.placeRects, pointToPdf: core.pointToPdf });
}

async function loadAsset(plan: AssetPlan, name: AssetName): Promise<ArrayBuffer> {
  const bytes = await fetchAsset(plan.urls[name], plan.timeoutMs);
  if (plan.integrity) {
    const fileName = ASSET_FILE_NAMES[name];
    await verifyAsset(fileName, bytes, plan.integrity[fileName]);
  }
  return bytes;
}
