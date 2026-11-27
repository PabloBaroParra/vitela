//! The `Engine`'s text-run editing surface against the real pdfium, in a
//! headless browser (B26 T-259). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{EditCommand, Engine};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// The id and `x` of the run showing `text` on page 0.
fn run(engine: &Engine, doc: u32, text: &str) -> Option<(f64, f64)> {
    let mut content = engine.read_page_content(doc, 0).unwrap();
    content
        .take_runs()
        .iter()
        .find(|run| run.text() == text)
        .map(|run| (run.id(), run.bbox()[0]))
}

#[wasm_bindgen_test]
async fn retype_move_restyle_and_remove_are_steps_a_save_writes() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::content_document(), None).unwrap();

    let (hello, x) = run(&engine, doc, "Hello").unwrap();
    engine
        .apply_edit(
            doc,
            &EditCommand::retype_text_run(0, hello, "Howdy".into()).unwrap(),
        )
        .unwrap();
    assert!(engine.preview_stale(doc).unwrap());
    assert_ne!(engine.paint_revision(doc).unwrap(), 0.0);
    engine.refresh_preview(doc).unwrap();

    let (howdy, _) = run(&engine, doc, "Howdy").unwrap();
    engine
        .apply_edit(
            doc,
            &EditCommand::move_text_run(0, howdy, 40.0, 0.0).unwrap(),
        )
        .unwrap();
    let (howdy, moved) = run(&engine, doc, "Howdy").unwrap();
    assert!((moved - (x + 40.0)).abs() < 0.5, "{moved}");

    engine
        .apply_edit(
            doc,
            &EditCommand::restyle_text_runs(0, vec![howdy], true, false, false, false).unwrap(),
        )
        .unwrap();
    let (howdy, _) = run(&engine, doc, "Howdy").unwrap();
    assert!(engine.text_run_look(doc, 0, howdy).unwrap().unwrap().bold());
    assert!(engine
        .text_run_font_program(doc, 0, howdy)
        .unwrap()
        .is_none());

    let (world, _) = run(&engine, doc, " world").unwrap();
    engine
        .apply_edit(doc, &EditCommand::remove_text_run(0, world).unwrap())
        .unwrap();
    engine.refresh_preview(doc).unwrap();

    let saved = engine.save_to_bytes(doc, false).unwrap();
    let reopened = engine.open(saved, None).unwrap();
    let (_, saved_x) = run(&engine, reopened, "Howdy").expect("retyped");
    assert!((saved_x - (x + 40.0)).abs() < 0.5);
    assert!(run(&engine, reopened, " world").is_none(), "removed");

    // Four steps, each undone and redone once, across the move's checkpoint.
    for _ in 0..4 {
        assert!(engine.undo(doc).unwrap());
    }
    assert!(!engine.can_undo(doc).unwrap());
    assert!(run(&engine, doc, "Hello").is_some());
    for _ in 0..4 {
        assert!(engine.redo(doc).unwrap());
    }
    assert!(run(&engine, doc, " world").is_none());
}

#[wasm_bindgen_test]
async fn a_document_that_forbids_content_edits_refuses_them() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::unmodifiable_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    let mut content = engine.read_page_content(doc, 0).unwrap();
    let id = content.take_runs()[0].id();
    let refused = engine
        .apply_edit(doc, &EditCommand::remove_text_run(0, id).unwrap())
        .unwrap_err();
    assert_eq!(error_name(refused), "EditNotPermitted");
    assert!(!engine.can_undo(doc).unwrap());
    assert_eq!(
        error_name(EditCommand::remove_text_run(0, 0.5).err().unwrap()),
        "UnsupportedOperation"
    );
}
