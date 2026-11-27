//! The `Engine`'s image editing surface against the real pdfium, in a
//! headless browser (B26 T-260). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{EditCommand, Engine};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// The id and box of every image on page 0.
fn images(engine: &Engine, doc: u32) -> Vec<(f64, Vec<f64>)> {
    let mut content = engine.read_page_content(doc, 0).unwrap();
    content
        .take_images()
        .iter()
        .map(|image| (image.id(), image.bbox()))
        .collect()
}

#[wasm_bindgen_test]
async fn move_resize_replace_insert_and_remove_are_steps_a_save_writes() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::content_document(), None).unwrap();

    let (id, _) = images(&engine, doc)[0].clone();
    engine
        .apply_edit(doc, &EditCommand::move_image(0, id, 10.0, 0.0).unwrap())
        .unwrap();
    assert!(engine.preview_stale(doc).unwrap());
    engine.refresh_preview(doc).unwrap();
    let (id, _) = images(&engine, doc)[0].clone();
    engine
        .apply_edit(
            doc,
            &EditCommand::resize_image(0, id, 82.0, 450.0, 200.0, 100.0).unwrap(),
        )
        .unwrap();
    let (id, _) = images(&engine, doc)[0].clone();
    engine.check_image_replacement(doc, 0, id).unwrap();
    engine
        .apply_edit(
            doc,
            &EditCommand::replace_image(0, id, pdfs::PNG_1X1.to_vec()).unwrap(),
        )
        .unwrap();
    engine
        .apply_edit(
            doc,
            &EditCommand::insert_image(0, 300.0, 700.0, pdfs::PNG_1X1.to_vec()),
        )
        .unwrap();
    assert_eq!(images(&engine, doc).len(), 2);

    let saved = engine.save_to_bytes(doc, false).unwrap();
    let reopened = engine.open(saved, None).unwrap();
    let shown = images(&engine, reopened);
    assert_eq!(shown.len(), 2);
    assert!(shown
        .iter()
        .any(|(_, bbox)| (bbox[0] - 82.0).abs() < 0.01 && (bbox[2] - 200.0).abs() < 0.01));

    let (id, _) = images(&engine, doc)[0].clone();
    engine
        .apply_edit(doc, &EditCommand::remove_image(0, id).unwrap())
        .unwrap();
    assert_eq!(images(&engine, doc).len(), 1);
    for _ in 0..5 {
        assert!(engine.undo(doc).unwrap());
    }
    assert!(!engine.can_undo(doc).unwrap());
    assert_eq!(images(&engine, doc).len(), 1);
}

#[wasm_bindgen_test]
async fn a_document_that_forbids_content_edits_refuses_image_edits() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::unmodifiable_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    let refused = engine
        .apply_edit(
            doc,
            &EditCommand::insert_image(0, 10.0, 10.0, pdfs::PNG_1X1.to_vec()),
        )
        .unwrap_err();
    assert_eq!(error_name(refused), "EditNotPermitted");
    assert_eq!(
        error_name(engine.check_image_replacement(doc, 0, 0.0).unwrap_err()),
        "EditNotPermitted"
    );
    assert!(!engine.can_undo(doc).unwrap());
}
