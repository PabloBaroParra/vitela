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

use pdf_wasm::{EditCommand, Engine};
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
    rgba.as_chunks::<4>()
        .0
        .iter()
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
async fn text_on_a_turned_page_is_placed_where_it_is_drawn() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::rotated_document(90), None).unwrap();
    let size = engine.page_dimensions(doc).unwrap()[0];
    let (width, height, turn) = (
        size.width_pt as f32,
        size.height_pt as f32,
        size.rotation_degrees,
    );

    let found = engine.search(doc, "rotated".into()).unwrap();
    let placed = pdf_wasm::place_rects(found[0].lines(), width, height, turn);
    let (left, top, line_width, line_height) = (placed[0], placed[1], placed[2], placed[3]);
    // Drawn at PDF (72, 700) on an upright page; a clockwise quarter turn
    // takes PDF y to the drawn left and PDF x to the drawn top, and the
    // line now runs down the page rather than across it.
    assert!((680.0..730.0).contains(&left), "left {left}");
    assert!((60.0..80.0).contains(&top), "top {top}");
    assert!(line_height > line_width, "a turned line runs downwards");

    // The middle of that drawn line is back on the word's characters.
    let middle = pdf_wasm::point_to_pdf(
        f64::from(left + line_width / 2.0),
        f64::from(top + line_height / 2.0),
        width,
        height,
        turn,
    );
    let characters = engine.page_characters(doc, 0).unwrap();
    assert_eq!(characters.word_at(middle[0], middle[1]), Some(vec![0, 7]));
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
        assert_eq!(hit.lines().len(), 4, "one line rect for a one-line match");
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
async fn a_promptless_document_renders_but_withholds_text() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::promptless_restricted_document(), None)
        .expect("an empty user password opens without a prompt");

    let mut page = engine.render_page(doc, 0, 72, false).unwrap();
    assert!(inked_pixels(&page.take_rgba()) > 0);
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

/// Each page's text in page order, read through the engine's own search of
/// the page's single word.
fn page_words(engine: &mut Engine, doc: u32, words: &[&str]) -> Vec<String> {
    let count = engine.page_count(doc).unwrap() as usize;
    let mut order = vec![String::new(); count];
    for word in words {
        for hit in engine.search(doc, (*word).into()).unwrap() {
            order[hit.page_index() as usize] = (*word).into();
        }
    }
    order
}

#[wasm_bindgen_test]
async fn edits_save_and_reopen_with_their_order_turn_and_count() {
    let words = ["one", "two", "three", "four"];
    let mut engine = engine().await;
    let doc = engine.open(pdfs::text_document(&words), None).unwrap();

    engine
        .apply_edit(doc, &EditCommand::move_pages(0, 1, 3))
        .unwrap();
    engine
        .apply_edit(doc, &EditCommand::rotate_pages(0, 1, 90))
        .unwrap();
    engine
        .apply_edit(doc, &EditCommand::remove_pages(1, 1))
        .unwrap();
    assert_eq!(engine.page_count(doc).unwrap(), 3);

    let saved = engine.save_to_bytes(doc, false).unwrap();
    let reopened = engine.open(saved, None).unwrap();
    assert_eq!(engine.page_count(reopened).unwrap(), 3);
    assert_eq!(
        page_words(&mut engine, reopened, &words),
        ["two", "four", "one"]
    );
    let turns: Vec<u32> = engine
        .page_dimensions(reopened)
        .unwrap()
        .iter()
        .map(|page| page.rotation_degrees)
        .collect();
    assert_eq!(turns, [90, 0, 0]);
}

#[wasm_bindgen_test]
async fn undo_and_redo_step_the_edits() {
    let words = ["one", "two"];
    let mut engine = engine().await;
    let doc = engine.open(pdfs::text_document(&words), None).unwrap();
    assert!(!engine.can_undo(doc).unwrap());
    assert!(!engine.undo(doc).unwrap(), "nothing to undo");

    engine
        .apply_edit(doc, &EditCommand::remove_pages(0, 1))
        .unwrap();
    assert!(engine.undo(doc).unwrap());
    assert_eq!(engine.page_count(doc).unwrap(), 2);
    assert!(engine.can_redo(doc).unwrap());
    assert!(engine.redo(doc).unwrap());
    assert!(!engine.can_redo(doc).unwrap());

    let reopened = engine.open(engine.save_to_bytes(doc, false).unwrap(), None);
    let reopened = reopened.unwrap();
    assert_eq!(page_words(&mut engine, reopened, &words), ["two"]);
}

#[wasm_bindgen_test]
async fn refresh_preview_renders_the_edited_pages() {
    let words = ["one", "two"];
    let mut engine = engine().await;
    let doc = engine.open(pdfs::text_document(&words), None).unwrap();

    engine
        .apply_edit(doc, &EditCommand::rotate_pages(1, 1, 90))
        .unwrap();
    engine
        .apply_edit(doc, &EditCommand::move_pages(1, 1, 0))
        .unwrap();
    engine.refresh_preview(doc).unwrap();

    let page = engine.render_page(doc, 0, 72, false).unwrap();
    assert_eq!(
        (page.width(), page.height()),
        (792, 612),
        "turned page first"
    );
    assert_eq!(page_words(&mut engine, doc, &words), ["two", "one"]);

    assert!(engine.undo(doc).unwrap());
    assert!(engine.undo(doc).unwrap());
    engine.refresh_preview(doc).unwrap();
    let page = engine.render_page(doc, 0, 72, false).unwrap();
    assert_eq!((page.width(), page.height()), (612, 792));
    assert_eq!(page_words(&mut engine, doc, &words), ["one", "two"]);
}

#[wasm_bindgen_test]
async fn a_refresh_reopens_an_encrypted_document_with_its_password() {
    let mut engine = engine().await;
    let doc = engine
        .open_with_passwords(
            pdfs::restricted_document(),
            pdfs::USER_PASSWORD.into(),
            pdfs::OWNER_PASSWORD.into(),
        )
        .unwrap();
    engine
        .apply_edit(doc, &EditCommand::rotate_pages(0, 1, 90))
        .unwrap();
    engine.refresh_preview(doc).unwrap();
    let page = engine.render_page(doc, 0, 72, false).unwrap();
    assert_eq!((page.width(), page.height()), (792, 612));
}

#[wasm_bindgen_test]
async fn page_edits_are_refused_by_kind() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["one", "two"]), None)
        .unwrap();
    for edit in [
        EditCommand::move_pages(0, 1, 2),
        EditCommand::rotate_pages(2, 1, 90),
        EditCommand::remove_pages(5, 1),
    ] {
        let err = engine.apply_edit(doc, &edit).unwrap_err();
        assert_eq!(error_name(err), "PageIndexOutOfBounds");
    }

    let restricted = engine
        .open(
            pdfs::restricted_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    for edit in [
        EditCommand::move_pages(0, 1, 0),
        EditCommand::rotate_pages(0, 1, 90),
        EditCommand::remove_pages(0, 1),
        EditCommand::remove_pages(5, 1),
    ] {
        let err = engine.apply_edit(restricted, &edit).unwrap_err();
        assert_eq!(error_name(err), "EditNotPermitted");
    }
    assert!(!engine.can_undo(restricted).unwrap());

    let unknown = engine
        .apply_edit(99, &EditCommand::remove_pages(0, 1))
        .unwrap_err();
    assert_eq!(error_name(unknown), "DocumentNotFound");
}
