// The engine behind the worker: pdfium (Emscripten) + core/pdf-wasm
// (wasm-bindgen). This file is the ONLY place that imports the Rust module;
// everything else talks to the `Engine` interface in core/types.ts.

import initEngine, * as core from '../../generated/engine/pdf_wasm.js';
import engineWasmUrl from '../../generated/engine/pdf_wasm_bg.wasm?url';
import pdfiumGlueUrl from '../../generated/pdfium/pdfium.js?url';
import pdfiumWasmUrl from '../../generated/pdfium/pdfium.wasm?url';
import type { Engine } from '../core/types';
import { documentEngine } from './documentEngine';
import { instantiatePdfium } from './pdfium';

/** Loads both WASM modules, binds pdfium-render to pdfium, starts the core. */
export async function createEngine(): Promise<Engine> {
  const [pdfium, engineExports] = await Promise.all([
    fetchText(pdfiumGlueUrl).then((glue) => instantiatePdfium(glue, pdfiumWasmUrl)),
    initEngine({ module_or_path: engineWasmUrl }),
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
