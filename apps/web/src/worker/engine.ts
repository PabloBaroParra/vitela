// The engine behind the worker: pdfium (Emscripten) + core/pdf-wasm
// (wasm-bindgen). This file is the ONLY place that imports the Rust module;
// everything else talks to the `Engine` interface in core/types.ts.

import initEngine, * as core from '../../generated/engine/pdf_wasm.js';
import engineWasmUrl from '../../generated/engine/pdf_wasm_bg.wasm?url';
import pdfiumGlueUrl from '../../generated/pdfium/pdfium.js?url';
import pdfiumWasmUrl from '../../generated/pdfium/pdfium.wasm?url';
import type { DocumentInfo, Engine, PageSize, RenderedPage } from '../core/types';
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
  return singleDocumentEngine(new core.Engine());
}

/**
 * The UI holds one document at a time: opening another closes the previous
 * one, so pdfium never keeps a document nobody can reach.
 */
function singleDocumentEngine(engine: core.Engine): Engine {
  let current: number | null = null;
  return {
    async open(bytes: Uint8Array, password?: string): Promise<DocumentInfo> {
      // A refused open throws here, before the previous document is closed.
      const doc = engine.open(bytes, password);
      try {
        const info = { pageCount: engine.pageCount(doc), pages: pageSizes(engine, doc) };
        if (current !== null) engine.close(current);
        current = doc;
        return info;
      } catch (error) {
        engine.close(doc);
        throw error;
      }
    },
    async renderPage(pageIndex: number, dpi: number): Promise<RenderedPage> {
      if (current === null) throw new Error('render: no document is open');
      const page = engine.renderPage(current, pageIndex, dpi, false);
      try {
        const rgba = page.takeRgba();
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

/** Copies the core's page sizes into plain objects and frees the wasm ones. */
function pageSizes(engine: core.Engine, doc: number): PageSize[] {
  return engine.pageDimensions(doc).map((dimensions) => {
    try {
      const { widthPt, heightPt, rotationDegrees } = dimensions;
      return { widthPt, heightPt, rotationDegrees };
    } finally {
      dimensions.free();
    }
  });
}

async function fetchText(url: string): Promise<string> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response.text();
}
