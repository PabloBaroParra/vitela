//! The browser-test harness shared by `tests/*.rs`: pdfium loaded the way
//! the worker loads it, bound once, and the JavaScript error's kind.
#![allow(dead_code)]

use std::cell::Cell;

use pdf_wasm::Engine;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

const PDFIUM_GLUE: &str = include_str!(concat!(env!("PDFIUM_WASM_DIR"), "/pdfium.js"));
const PDFIUM_WASM: &[u8] = include_bytes!(concat!(env!("PDFIUM_WASM_DIR"), "/pdfium.wasm"));

#[wasm_bindgen(inline_js = r#"
export function loadPdfium(glue, wasmBinary) {
  return new Promise((resolve, reject) => {
    const module = {
      wasmBinary,
      onRuntimeInitialized: () => {
        // The same alias the worker applies (pdfium.ts applyInitAlias).
        module._PDFium_Init = module._FPDF_InitLibrary;
        resolve(module);
      },
      onAbort: (what) => reject(new Error('pdfium runtime aborted: ' + what)),
    };
    new Function('Module', glue + `
;Object.defineProperty(Module, "wasmExports", {
  configurable: true,
  get: function () { return typeof wasmExports != "undefined" ? wasmExports : undefined; }
});`)(module);
  });
}

export function bindPdfium(pdfium, rustExports) {
  return rustExports.initialize_pdfium_render(pdfium, rustExports, false);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = loadPdfium)]
    fn load_pdfium(glue: &str, wasm_binary: &[u8]) -> js_sys::Promise;
    #[wasm_bindgen(js_name = bindPdfium)]
    fn bind_pdfium(pdfium: &JsValue, rust_exports: &JsValue) -> bool;
}

thread_local! {
    static BOUND: Cell<bool> = const { Cell::new(false) };
}

/// An engine over the real pdfium, binding it on first use.
pub async fn engine() -> Engine {
    if !BOUND.with(Cell::get) {
        let pdfium = JsFuture::from(load_pdfium(PDFIUM_GLUE, PDFIUM_WASM))
            .await
            .expect("pdfium loads");
        assert!(bind_pdfium(&pdfium, &wasm_bindgen::exports()));
        BOUND.with(|bound| bound.set(true));
    }
    Engine::new().expect("engine")
}

pub fn error_name(error: JsValue) -> String {
    js_sys::Error::from(error).name().into()
}
