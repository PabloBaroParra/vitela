//! The `Engine` against the real pdfium, in a headless browser.
//!
//! Runs in a dedicated worker — where `apps/web` runs the engine — with
//! pdfium's glue evaluated as a function body exactly as the worker does it
//! (`apps/web/src/worker/pdfium.ts`). pdfium itself is compiled in from
//! `PDFIUM_WASM_DIR` (fetched by `scripts/fetch-pdfium.sh`), so the test
//! server needs to serve nothing but the test module.
//!
//! ```sh
//! core/pdf-wasm/scripts/fetch-pdfium.sh target/pdfium-wasm
//! PDFIUM_WASM_DIR=$PWD/target/pdfium-wasm \
//! CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//!   cargo test --target wasm32-unknown-unknown -p pdf-wasm
//! ```
#![cfg(target_arch = "wasm32")]

use std::cell::Cell;

use pdf_wasm::Engine;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

#[path = "support/pdfs.rs"]
mod pdfs;

wasm_bindgen_test_configure!(run_in_dedicated_worker);

const PDFIUM_GLUE: &str = include_str!(concat!(env!("PDFIUM_WASM_DIR"), "/pdfium.js"));
const PDFIUM_WASM: &[u8] = include_bytes!(concat!(env!("PDFIUM_WASM_DIR"), "/pdfium.wasm"));
const LETTER_FORM: &[u8] = include_bytes!("../../../tests/fixtures/forms/reportlab_acroform.pdf");

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
async fn engine() -> Engine {
    if !BOUND.with(Cell::get) {
        let pdfium = JsFuture::from(load_pdfium(PDFIUM_GLUE, PDFIUM_WASM))
            .await
            .expect("pdfium loads");
        assert!(bind_pdfium(&pdfium, &wasm_bindgen::exports()));
        BOUND.with(|bound| bound.set(true));
    }
    Engine::new().expect("engine")
}

fn error_name(error: JsValue) -> String {
    js_sys::Error::from(error).name().into()
}

fn inked_pixels(rgba: &[u8]) -> usize {
    rgba.chunks_exact(4)
        .filter(|pixel| pixel[..3].iter().any(|&channel| channel < 128))
        .count()
}

#[wasm_bindgen_test]
async fn renders_a_page_to_tight_rgba() {
    let mut engine = engine().await;
    let doc = engine.open(LETTER_FORM.to_vec(), None).unwrap();
    assert_eq!(engine.page_count(doc).unwrap(), 1);

    let mut page = engine.render_page(doc, 0, 72, false).unwrap();
    assert_eq!((page.width(), page.height()), (612, 792));
    let rgba = page.take_rgba();
    assert_eq!(rgba.len(), 612 * 792 * 4);
    assert!(inked_pixels(&rgba) > 1_000, "the page has visible content");

    let mut doubled = engine.render_page(doc, 0, 144, false).unwrap();
    assert_eq!((doubled.width(), doubled.height()), (1224, 1584));
    assert_eq!(doubled.take_rgba().len(), 1224 * 1584 * 4);
}

#[wasm_bindgen_test]
async fn dimensions_follow_the_page_rotation() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::rotated_document(90), None).unwrap();

    let dimensions = engine.page_dimensions(doc).unwrap();
    assert_eq!(dimensions.len(), 1);
    assert_eq!(
        (dimensions[0].width_pt, dimensions[0].height_pt),
        (792.0, 612.0)
    );
    assert_eq!(dimensions[0].rotation_degrees, 90);

    let page = engine.render_page(doc, 0, 72, false).unwrap();
    assert_eq!((page.width(), page.height()), (792, 612));
}

#[wasm_bindgen_test]
async fn reads_text_runs_and_page_characters() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["Hello Vitela"]), None)
        .unwrap();
    assert!(engine.text_extraction_allowed(doc).unwrap());

    let runs = engine.text_runs(doc, 0).unwrap();
    let text: String = runs.iter().map(|run| run.text()).collect();
    assert_eq!(text, "Hello Vitela");
    for run in &runs {
        assert_eq!(run.bounds().len(), run.text().chars().count() * 4);
        assert_eq!(run.font_size_pt().round(), pdfs::FONT_SIZE_PT);
    }

    let characters = engine.page_characters(doc, 0).unwrap();
    assert_eq!(characters.length(), 12);
    assert_eq!(characters.text_in(0, 5), "Hello");
    assert_eq!(characters.text_in(5, 0), "Hello", "carets in either order");
    let y = pdfs::TEXT_Y_PT + pdfs::FONT_SIZE_PT / 3.0;
    assert_eq!(characters.caret_at(pdfs::TEXT_X_PT - 1.0, y), Some(0));
    assert_eq!(
        characters.word_at(pdfs::TEXT_X_PT + 5.0, y),
        Some(vec![0, 5])
    );
    assert_eq!(characters.rects_in(0, 5).len(), 4, "one rect for one line");
}

#[wasm_bindgen_test]
async fn search_finds_every_page_and_ignores_an_empty_query() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["alpha beta", "beta"]), None)
        .unwrap();

    let found = engine.search(doc, "beta".into()).unwrap();
    let pages: Vec<u32> = found.iter().map(|hit| hit.page_index()).collect();
    assert_eq!(pages, vec![0, 1]);
    for hit in &found {
        assert_eq!(hit.text(), "beta");
        assert_eq!(hit.bounds().len(), 4 * 4);
    }
    assert!(engine.search(doc, "Beta".into()).unwrap().is_empty());
    assert!(engine.search(doc, String::new()).unwrap().is_empty());
}

#[wasm_bindgen_test]
async fn an_encrypted_document_names_what_it_needs() {
    let mut engine = engine().await;
    let bytes = pdfs::restricted_document();

    let missing = engine.open(bytes.clone(), None).unwrap_err();
    assert_eq!(error_name(missing), "PasswordRequired");
    let wrong = engine.open(bytes.clone(), Some("nope".into())).unwrap_err();
    assert_eq!(error_name(wrong), "WrongPassword");
    let wrong_owner = engine
        .open_with_passwords(bytes, pdfs::USER_PASSWORD.into(), "nope".into())
        .unwrap_err();
    assert_eq!(error_name(wrong_owner), "WrongPassword");
}

#[wasm_bindgen_test]
async fn the_user_password_renders_but_withholds_text() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::restricted_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();

    let mut page = engine.render_page(doc, 0, 72, false).unwrap();
    assert!(
        inked_pixels(&page.take_rgba()) > 0,
        "renders under the user password"
    );
    assert!(!engine.text_extraction_allowed(doc).unwrap());
    let refusals = [
        engine.text_runs(doc, 0).err(),
        engine.page_characters(doc, 0).err(),
        engine.search(doc, "Hello".into()).err(),
    ];
    for refusal in refusals {
        assert_eq!(
            error_name(refusal.expect("refused")),
            "TextExtractionDenied"
        );
    }
}

#[wasm_bindgen_test]
async fn both_passwords_open_as_owner_with_text() {
    let mut engine = engine().await;
    let doc = engine
        .open_with_passwords(
            pdfs::restricted_document(),
            pdfs::USER_PASSWORD.into(),
            pdfs::OWNER_PASSWORD.into(),
        )
        .unwrap();
    assert!(engine.text_extraction_allowed(doc).unwrap());
    assert_eq!(engine.search(doc, "Vitela".into()).unwrap().len(), 1);
}

#[wasm_bindgen_test]
async fn bad_addresses_are_refused_by_kind() {
    let mut engine = engine().await;
    let doc = engine.open(LETTER_FORM.to_vec(), None).unwrap();

    let past_end = engine.render_page(doc, 1, 72, false).err().unwrap();
    assert_eq!(error_name(past_end), "PageIndexOutOfBounds");
    let past_end = engine.text_runs(doc, 1).err().unwrap();
    assert_eq!(error_name(past_end), "PageIndexOutOfBounds");

    let not_a_pdf = engine
        .open(b"this is not a PDF".to_vec(), None)
        .unwrap_err();
    assert_eq!(error_name(not_a_pdf), "InvalidDocument");

    assert!(engine.close(doc));
    assert!(!engine.close(doc), "closing twice is a no-op");
    let closed = engine.page_count(doc).unwrap_err();
    assert_eq!(error_name(closed), "DocumentNotFound");
}
