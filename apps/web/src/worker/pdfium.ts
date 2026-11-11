// Loads bblanchon's Emscripten build of pdfium (chromium/7763) into the
// worker, ready for pdfium-render's `initialize_pdfium_render`.

/** The Emscripten module object: exported functions are `_`-prefixed members. */
export type PdfiumModule = Record<string, unknown>;

/**
 * pdfium-render 0.9.5's WASM `FPDF_InitLibrary()` (wasm_bindings.rs:1652-1668)
 * calls `PDF_InitLibrary` or `PDFium_Init` — the names other pdfium WASM
 * builds export. bblanchon's build exports only the real `_FPDF_InitLibrary`,
 * so without this alias the first pdfium call panics with "func is not a
 * function". Must run before `initialize_pdfium_render`.
 */
export function applyInitAlias(module: PdfiumModule): void {
  const init = module._FPDF_InitLibrary;
  if (typeof init !== 'function') {
    throw new Error('pdfium build does not export _FPDF_InitLibrary; cannot alias _PDFium_Init');
  }
  module._PDFium_Init = init;
}

/**
 * Evaluates the pdfium glue (`pdfium.js` source) and resolves once its
 * runtime is initialised, with the init alias applied.
 *
 * The glue is non-MODULARIZE: it starts with
 * `var Module = typeof Module != "undefined" ? Module : {}` and fills that
 * object. Neither `importScripts` (unavailable in module workers) nor an ES
 * import (its `var` would shadow any pre-seeded config) can run it, so it is
 * evaluated as a sloppy-mode function body whose `Module` parameter carries
 * the config in and the runtime out, without touching the global scope.
 *
 * Keeping the glue out of the global scope hides its top-level `wasmTable`,
 * which pdfium-render looks up as a global first. Its documented fallback is
 * `Module.wasmExports.__indirect_function_table` (pdfium-render issue #128),
 * so the glue's own `wasmExports` is exposed on the module instead.
 */
export function instantiatePdfium(glueSource: string, wasmUrl: string): Promise<PdfiumModule> {
  return new Promise((resolve, reject) => {
    const module: PdfiumModule = {
      locateFile: (path: string) => (path.endsWith('.wasm') ? wasmUrl : path),
      onRuntimeInitialized: () => {
        try {
          applyInitAlias(module);
          requireFunctionTable(module);
          resolve(module);
        } catch (error) {
          reject(error);
        }
      },
      // Without this a failed runtime (404 on pdfium.wasm, bad module, OOM)
      // never fires onRuntimeInitialized and the worker waits forever.
      onAbort: (what: unknown) => reject(new Error(`pdfium runtime aborted: ${String(what)}`)),
    };
    try {
      new Function('Module', glueSource + EXPOSE_WASM_EXPORTS)(module);
    } catch (error) {
      reject(error);
    }
  });
}

// Appended to the glue body, so `wasmExports` resolves to the glue's own
// top-level variable (assigned once the wasm is instantiated, hence a getter).
const EXPOSE_WASM_EXPORTS = `
;Object.defineProperty(Module, "wasmExports", {
  configurable: true,
  get: function () { return typeof wasmExports != "undefined" ? wasmExports : undefined; }
});`;

function requireFunctionTable(module: PdfiumModule): void {
  const exports = module.wasmExports as Record<string, unknown> | undefined;
  if (!(exports?.__indirect_function_table instanceof WebAssembly.Table)) {
    throw new Error('pdfium glue exposes no wasmExports.__indirect_function_table');
  }
}
