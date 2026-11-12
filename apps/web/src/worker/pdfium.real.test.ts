// @vitest-environment node
//
// The init alias against the REAL pinned pdfium and the real core module,
// run under Node (Emscripten's glue supports it). Needs `npm run build:wasm`
// first; CI runs it before Vitest.

import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { instantiatePdfium, type PdfiumModule } from './pdfium';
import * as core from '../../generated/engine/pdf_wasm.js';

const here = dirname(fileURLToPath(import.meta.url));
const generated = resolve(here, '../../generated');
const fixture = resolve(here, '../../../../tests/fixtures/forms/reportlab_acroform.pdf');

describe('real pdfium chromium/7763 + core/pdf-wasm', () => {
  let pdfium: PdfiumModule;
  let coreExports: unknown;
  const glueSource = readFileSync(resolve(generated, 'pdfium/pdfium.js'), 'utf8');

  beforeAll(async () => {
    // Under Node, Emscripten's glue reads the .wasm with require('fs').
    const scope = globalThis as Record<string, unknown>;
    scope.require = createRequire(import.meta.url);
    scope.__dirname = resolve(generated, 'pdfium');
    pdfium = await instantiatePdfium(glueSource, resolve(generated, 'pdfium/pdfium.wasm'));
    coreExports = core.initSync({ module: readFileSync(resolve(generated, 'engine/pdf_wasm_bg.wasm')) });
  });

  it('exports only _FPDF_InitLibrary, none of the names pdfium-render looks up', () => {
    // pdfium-render 0.9.5 calls `PDF_InitLibrary` when `FPDF_InitLibrary`
    // exists, else `PDFium_Init`. If this build ever starts exporting one of
    // those, the alias in pdfium.ts is obsolete (or shadowing something).
    expect(typeof pdfium._FPDF_InitLibrary).toBe('function');
    expect(pdfium.FPDF_InitLibrary).toBeUndefined();
    expect(pdfium._PDF_InitLibrary).toBeUndefined();
    expect(glueSource).not.toMatch(/\b_?(PDFium_Init|PDF_InitLibrary)\b/);
    // instantiatePdfium applied the alias.
    expect(pdfium._PDFium_Init).toBe(pdfium._FPDF_InitLibrary);
  });

  it('opens a repo fixture and renders non-blank pixels through the alias', () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = new core.Engine();
    const doc = engine.open(readFileSync(fixture));
    expect(engine.pageCount(doc)).toBe(1);
    // US Letter at 72 dpi: one pixel per point.
    const page = engine.renderPage(doc, 0, 72, false);
    const rgba = page.takeRgba();
    expect(rgba.length).toBe(page.width * page.height * 4);
    expect([page.width, page.height]).toEqual([612, 792]);
    let inked = 0;
    for (let i = 0; i < rgba.length; i += 4) {
      if (rgba[i]! < 128 && rgba[i + 1]! < 128 && rgba[i + 2]! < 128) inked++;
    }
    expect(inked).toBeGreaterThan(100);
    page.free();
    expect(engine.close(doc)).toBe(true);
    engine.free();
  });
});
