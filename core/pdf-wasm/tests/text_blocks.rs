//! The `Engine`'s text-block editing surface against the real pdfium, in a
//! headless browser (B26 T-261). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{EditCommand, Engine};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// The run ids and text of the paragraph block on page 0 starting `start`.
fn block(engine: &Engine, doc: u32, start: &str) -> Option<(Vec<f64>, String)> {
    engine
        .read_paragraph_blocks(doc, 0)
        .unwrap()
        .iter()
        .find(|block| block.text().starts_with(start))
        .map(|block| (block.run_ids(), block.text()))
}

fn texts(engine: &Engine, doc: u32) -> Vec<String> {
    let mut content = engine.read_page_content(doc, 0).unwrap();
    content.take_runs().iter().map(|run| run.text()).collect()
}

#[wasm_bindgen_test]
async fn a_paragraph_retype_and_an_insertion_are_steps_a_save_writes() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::content_document(), None).unwrap();

    let (runs, _) = block(&engine, doc, "First line").unwrap();
    engine
        .apply_edit(
            doc,
            &EditCommand::retype_text_block(0, runs, "Fresh start\nSecond line".into()).unwrap(),
        )
        .unwrap();
    engine
        .apply_edit(
            doc,
            &EditCommand::insert_text(0, 100.0, 200.0, "Added note".into(), 14.0),
        )
        .unwrap();
    assert!(engine.preview_stale(doc).unwrap());
    engine.refresh_preview(doc).unwrap();

    let saved = engine.save_to_bytes(doc, false).unwrap();
    let reopened = engine.open(saved, None).unwrap();
    let (_, text) = block(&engine, reopened, "Fresh").expect("retyped");
    assert_eq!(text, "Fresh start\nSecond line");
    assert!(texts(&engine, reopened).contains(&"Added note".to_string()));

    for _ in 0..2 {
        assert!(engine.undo(doc).unwrap());
    }
    assert!(!engine.can_undo(doc).unwrap());
    assert!(block(&engine, doc, "First line").is_some());
}

#[wasm_bindgen_test]
async fn a_document_that_forbids_content_edits_refuses_an_insertion() {
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
            &EditCommand::insert_text(0, 100.0, 200.0, "x".into(), 14.0),
        )
        .unwrap_err();
    assert_eq!(error_name(refused), "EditNotPermitted");
    assert_eq!(
        error_name(
            EditCommand::retype_text_block(0, vec![0.5], "x".into())
                .err()
                .unwrap()
        ),
        "UnsupportedOperation"
    );
}
