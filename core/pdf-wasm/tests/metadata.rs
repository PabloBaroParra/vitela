//! The `Engine`'s document-properties surface against the real pdfium, in a
//! headless browser (B26 T-255). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::EditCommand;
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

fn titled(title: &str, author: &str) -> EditCommand {
    EditCommand::set_document_properties(
        Some(title.into()),
        Some(author.into()),
        None,
        None,
        None,
        None,
    )
}

#[wasm_bindgen_test]
async fn a_change_is_read_back_undone_and_saved() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::described_document(), None).unwrap();
    let read = engine.document_properties(doc).unwrap();
    assert_eq!(read.title().as_deref(), Some("Quarterly report"));
    assert_eq!(read.author().as_deref(), Some("Ana"));
    assert!(engine.metadata_refusal(doc).unwrap().is_none());

    engine
        .apply_edit(doc, &titled("Annual report", "Luis"))
        .unwrap();
    let read = engine.document_properties(doc).unwrap();
    assert_eq!(read.title().as_deref(), Some("Annual report"));
    // Creator was not given: its key goes.
    assert_eq!(read.creator(), None);
    assert!(engine.can_undo(doc).unwrap());

    let saved = engine.save_to_bytes(doc, false).unwrap();
    let reopened = engine.open(saved, None).unwrap();
    let read = engine.document_properties(reopened).unwrap();
    assert_eq!(read.title().as_deref(), Some("Annual report"));
    assert_eq!(read.author().as_deref(), Some("Luis"));

    assert!(engine.undo(doc).unwrap());
    let read = engine.document_properties(doc).unwrap();
    assert_eq!(read.title().as_deref(), Some("Quarterly report"));
}

#[wasm_bindgen_test]
async fn a_document_that_withholds_modification_is_read_only() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::restricted_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    assert!(engine.metadata_refusal(doc).unwrap().is_some());
    assert!(engine.document_properties(doc).is_ok());
    let err = engine.apply_edit(doc, &titled("Mine", "Me")).err().unwrap();
    assert_eq!(error_name(err), "EditNotPermitted");
    assert!(!engine.can_undo(doc).unwrap());
}
