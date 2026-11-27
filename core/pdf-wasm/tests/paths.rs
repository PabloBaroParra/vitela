//! The `Engine`'s vector-path editing surface against the real pdfium, in a
//! headless browser (B26 T-263). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{EditCommand, Engine};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// The id, box and paint of every path on page 0.
fn paths(engine: &Engine, doc: u32) -> Vec<(f64, Vec<f64>, String)> {
    let mut content = engine.read_page_content(doc, 0).unwrap();
    content
        .take_paths()
        .iter()
        .map(|path| (path.id(), path.bbox(), path.paint()))
        .collect()
}

/// The filled box [`pdfs::content_document`] paints: 100 x 50 at (300, 300).
fn the_box(engine: &Engine, doc: u32) -> (f64, Vec<f64>) {
    paths(engine, doc)
        .into_iter()
        .find(|(_, _, paint)| paint == "fill")
        .map(|(id, bbox, _)| (id, bbox))
        .expect("the box")
}

fn near(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.01)
}

#[wasm_bindgen_test]
async fn move_resize_and_remove_are_steps_a_save_writes() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::content_document(), None).unwrap();

    let (id, _) = the_box(&engine, doc);
    engine
        .apply_edit(doc, &EditCommand::move_path(0, id, 10.0, 0.0).unwrap())
        .unwrap();
    assert!(engine.preview_stale(doc).unwrap());
    engine.refresh_preview(doc).unwrap();
    let (id, _) = the_box(&engine, doc);
    engine
        .apply_edit(
            doc,
            &EditCommand::resize_path(0, id, 310.0, 250.0, 200.0, 100.0).unwrap(),
        )
        .unwrap();

    let saved = engine.save_to_bytes(doc, false).unwrap();
    let reopened = engine.open(saved, None).unwrap();
    let (_, bbox) = the_box(&engine, reopened);
    assert!(near(&bbox, &[310.0, 250.0, 200.0, 100.0]), "{bbox:?}");

    let (id, _) = the_box(&engine, doc);
    engine
        .apply_edit(doc, &EditCommand::remove_path(0, id).unwrap())
        .unwrap();
    assert_eq!(paths(&engine, doc).len(), 1);
    // The resize and the removal fold into the move of the same path: one step.
    assert!(engine.undo(doc).unwrap());
    assert!(!engine.can_undo(doc).unwrap());
    assert_eq!(paths(&engine, doc).len(), 2);
}

#[wasm_bindgen_test]
async fn a_document_that_forbids_content_edits_refuses_path_edits() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::unmodifiable_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    let refused = engine
        .apply_edit(doc, &EditCommand::remove_path(0, 0.0).unwrap())
        .unwrap_err();
    assert_eq!(error_name(refused), "EditNotPermitted");
    assert!(!engine.can_undo(doc).unwrap());
}
