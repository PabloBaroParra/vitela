//! The `Engine`'s selection and clipboard surface against the real pdfium,
//! in a headless browser (B26 T-262). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{EditCommand, Engine};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// The ids of page `page`'s images, and of those a pending edit added.
fn images(engine: &Engine, doc: u32, page: u32) -> (Vec<f64>, Vec<f64>) {
    let mut content = engine.read_page_content(doc, page).unwrap();
    let images = content.take_images();
    (
        images.iter().map(|image| image.id()).collect(),
        images
            .iter()
            .filter(|image| image.pending())
            .map(|image| image.id())
            .collect(),
    )
}

#[wasm_bindgen_test]
async fn a_copy_pastes_onto_another_page_moves_as_one_step_and_saves() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::content_document_pages(2), None).unwrap();
    let (ids, _) = images(&engine, doc, 0);
    let mut clip = engine
        .copy_content_selection(doc, 0, vec![], ids, vec![], false)
        .unwrap();
    assert!(!engine.can_undo(doc).unwrap());
    let paste = EditCommand::paste_content(
        1,
        clip.take_pdf(),
        clip.bbox(),
        clip.came_from_encrypted(),
        300.0,
        200.0,
    )
    .unwrap();
    engine.apply_edit(doc, &paste).unwrap();
    assert!(engine.preview_stale(doc).unwrap());
    let (_, pasted) = images(&engine, doc, 1);
    assert_eq!(pasted.len(), 1);
    engine
        .apply_edit(
            doc,
            &EditCommand::move_content_selection(1, vec![], pasted, vec![], 10.0, 0.0).unwrap(),
        )
        .unwrap();

    let saved = engine.save_to_bytes(doc, false).unwrap();
    let reopened = engine.open(saved, None).unwrap();
    assert_eq!(images(&engine, reopened, 1).0.len(), 2);
    // The paste and its nudge are one step.
    assert!(engine.undo(doc).unwrap());
    assert!(!engine.can_undo(doc).unwrap());
}

#[wasm_bindgen_test]
async fn a_cut_is_refused_where_content_cannot_change() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::unmodifiable_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    let mut content = engine.read_page_content(doc, 0).unwrap();
    let run = content.take_runs()[0].id();
    let refused = engine
        .copy_content_selection(doc, 0, vec![run], vec![], vec![], true)
        .err()
        .unwrap();
    assert_eq!(error_name(refused), "EditNotPermitted");
    assert!(engine
        .copy_content_selection(doc, 0, vec![run], vec![], vec![], false)
        .is_ok());
}
