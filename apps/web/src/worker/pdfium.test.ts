import { describe, expect, it } from 'vitest';
import { applyInitAlias, instantiatePdfium } from './pdfium';

// A stand-in for bblanchon's Emscripten glue: non-MODULARIZE, it reuses a
// pre-seeded `Module` and calls back once the runtime is up.
const FAKE_GLUE = `
var Module = typeof Module != "undefined" ? Module : {};
Module.wasmPath = Module.locateFile("pdfium.wasm", "/ignored/");
Module._FPDF_InitLibrary = function () { return "initialised"; };
var wasmExports;
setTimeout(function () {
  wasmExports = { __indirect_function_table: new WebAssembly.Table({ initial: 1, element: "anyfunc" }) };
}, 0);
setTimeout(function () { Module.onRuntimeInitialized(); }, 0);
`;

describe('applyInitAlias', () => {
  it('aliases _PDFium_Init to _FPDF_InitLibrary', () => {
    const init = () => {};
    const module: Record<string, unknown> = { _FPDF_InitLibrary: init };
    applyInitAlias(module);
    expect(module._PDFium_Init).toBe(init);
  });

  it('refuses a pdfium build that does not export _FPDF_InitLibrary', () => {
    expect(() => applyInitAlias({})).toThrow(/_FPDF_InitLibrary/);
  });
});

describe('instantiatePdfium', () => {
  it('resolves the runtime with the init alias applied and the wasm URL wired', async () => {
    const module = await instantiatePdfium(FAKE_GLUE, '/assets/pdfium-abc123.wasm');
    expect(module.wasmPath).toBe('/assets/pdfium-abc123.wasm');
    expect(module._PDFium_Init).toBe(module._FPDF_InitLibrary);
  });

  it('does not leak the glue into the global scope', async () => {
    await instantiatePdfium(FAKE_GLUE, '/pdfium.wasm');
    expect('Module' in globalThis).toBe(false);
    expect('wasmExports' in globalThis).toBe(false);
  });

  it("exposes the glue's wasmExports, where pdfium-render finds the function table", async () => {
    const module = await instantiatePdfium(FAKE_GLUE, '/pdfium.wasm');
    const exports = module.wasmExports as { __indirect_function_table: unknown };
    expect(exports.__indirect_function_table).toBeInstanceOf(WebAssembly.Table);
  });

  it('rejects a glue whose function table pdfium-render could not find', async () => {
    const glue = FAKE_GLUE.replace('var wasmExports;', 'var renamedExports;').replace(
      'wasmExports = {',
      'renamedExports = {',
    );
    await expect(instantiatePdfium(glue, '/pdfium.wasm')).rejects.toThrow(
      /__indirect_function_table/,
    );
  });

  it('rejects when the runtime aborts', async () => {
    const glue = 'var Module = typeof Module != "undefined" ? Module : {}; Module.onAbort("no memory");';
    await expect(instantiatePdfium(glue, '/pdfium.wasm')).rejects.toThrow(
      'pdfium runtime aborted: no memory',
    );
  });

  it('rejects when the glue throws while evaluating', async () => {
    await expect(instantiatePdfium('throw new Error("bad glue")', '/pdfium.wasm')).rejects.toThrow(
      'bad glue',
    );
  });
});
