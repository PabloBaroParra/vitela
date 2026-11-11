// The engine behind the worker: pdfium (Emscripten) + the Rust render module
// (wasm-bindgen). This file is the ONLY place that knows which WASM module
// implements the engine — today the temporary render-wasm crate, later
// core/pdf-wasm (B26 T-219). Swapping it must not touch anything outside
// src/worker/.

import initRender, * as render from '../../generated/render/vitela_web_render.js';
import renderWasmUrl from '../../generated/render/vitela_web_render_bg.wasm?url';
import pdfiumGlueUrl from '../../generated/pdfium/pdfium.js?url';
import pdfiumWasmUrl from '../../generated/pdfium/pdfium.wasm?url';
import type { DocumentInfo, Engine, RenderedPage } from '../core/types';
import { instantiatePdfium } from './pdfium';

/** Loads both WASM modules and binds pdfium-render to pdfium. */
export async function createEngine(): Promise<Engine> {
  const [pdfium, renderExports] = await Promise.all([
    fetchText(pdfiumGlueUrl).then((glue) => instantiatePdfium(glue, pdfiumWasmUrl)),
    initRender({ module_or_path: renderWasmUrl }),
  ]);
  if (!render.initialize_pdfium_render(pdfium, renderExports, false)) {
    throw new Error('initialize_pdfium_render refused the pdfium module');
  }
  return {
    async open(bytes: Uint8Array): Promise<DocumentInfo> {
      return { pageCount: render.open_document(bytes) };
    },
    async renderPage(pageIndex: number, targetWidth: number): Promise<RenderedPage> {
      const page = render.render_page(pageIndex, targetWidth);
      try {
        const rgba = page.take_rgba();
        return {
          width: page.width,
          height: page.height,
          rgba: new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength),
        };
      } finally {
        page.free();
      }
    },
  };
}

async function fetchText(url: string): Promise<string> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response.text();
}
