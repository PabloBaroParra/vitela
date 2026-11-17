// The engine behind the worker: pdfium (Emscripten) + core/pdf-wasm
// (wasm-bindgen). This file is the ONLY place that imports the Rust module;
// everything else talks to the `Engine` interface in core/types.ts.

import initEngine, * as core from '../generated/engine/pdf_wasm.js';
import type { Engine } from '../core/types.js';
import type { EngineAssetUrls } from './assets.js';
import { documentEngine } from './documentEngine.js';
import { instantiatePdfium } from './pdfium.js';

/** Loads both WASM modules, binds pdfium-render to pdfium, starts the core. */
export async function loadEngine(assets: EngineAssetUrls): Promise<Engine> {
  const [pdfium, engineExports] = await Promise.all([
    fetchText(assets.pdfiumGlue).then((glue) => instantiatePdfium(glue, assets.pdfiumWasm)),
    initEngine({ module_or_path: assets.engineWasm }),
  ]);
  if (!core.initialize_pdfium_render(pdfium, engineExports, false)) {
    throw new Error('initialize_pdfium_render refused the pdfium module');
  }
  return documentEngine(new core.Engine(), { placeRects: core.placeRects, pointToPdf: core.pointToPdf });
}

async function fetchText(url: string): Promise<string> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response.text();
}
