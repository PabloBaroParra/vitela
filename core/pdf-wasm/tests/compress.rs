//! The `Engine`'s compress surface against the real pdfium, in a headless
//! browser (B26 T-250). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{compress_presets, EditCommand};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
fn the_presets_are_the_cores() {
    assert_eq!(compress_presets(), ["lossless", "balanced", "small"]);
}

#[wasm_bindgen_test]
async fn a_scan_compresses_smaller_and_the_copy_opens() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::SCAN_200DPI.to_vec(), None).unwrap();
    assert!(engine.compression_refusal(doc).unwrap().is_none());
    assert!(!engine.compression_invalidates_signatures(doc).unwrap());

    let mut saved = engine
        .save_compressed_to_bytes(doc, "balanced".into(), false)
        .unwrap();
    assert!(saved.reduced());
    assert!(saved.refusals().is_empty());
    let bytes = saved.take_bytes();
    assert_eq!(saved.after_bytes(), bytes.len() as f64);
    assert_eq!(
        saved.saved_bytes(),
        saved.before_bytes() - saved.after_bytes()
    );
    assert!(bytes.len() < pdfs::SCAN_200DPI.len());

    let copy = engine.open(bytes, None).expect("the copy opens");
    assert_eq!(engine.page_count(copy).unwrap(), 1);
    let mut page = engine.render_page(copy, 0, 36, false).unwrap();
    assert!(!page.take_rgba().is_empty());
}

#[wasm_bindgen_test]
async fn the_copy_carries_the_pending_edits_and_the_history_is_untouched() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["one", "two", "three"]), None)
        .unwrap();
    engine
        .apply_edit(doc, &EditCommand::remove_pages(0, 1))
        .unwrap();

    let mut saved = engine
        .save_compressed_to_bytes(doc, "lossless".into(), false)
        .unwrap();
    let copy = engine.open(saved.take_bytes(), None).unwrap();
    assert_eq!(engine.page_count(copy).unwrap(), 2);
    assert!(engine.can_undo(doc).unwrap());
    assert!(!engine.can_redo(doc).unwrap());
}

#[wasm_bindgen_test]
async fn an_encrypted_document_is_refused_ahead() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::locked_document(&["one"]),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    let refusal = engine.compression_refusal(doc).unwrap().expect("refused");
    assert_eq!(refusal.kind(), "encryptedDocumentNotRewritable");

    let saved = engine
        .save_compressed_to_bytes(doc, "small".into(), false)
        .unwrap();
    assert!(!saved.reduced());
    assert_eq!(saved.refusals().len(), 1);
    assert_eq!(saved.refusals()[0].message(), refusal.message());
}

#[wasm_bindgen_test]
async fn a_signed_document_needs_the_users_yes() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::SIGNED.to_vec(), None).unwrap();
    assert!(engine.compression_invalidates_signatures(doc).unwrap());

    let err = engine
        .save_compressed_to_bytes(doc, "balanced".into(), false)
        .err()
        .unwrap();
    assert_eq!(error_name(err), "SignaturesWouldBeInvalidated");
    assert!(engine
        .save_compressed_to_bytes(doc, "balanced".into(), true)
        .is_ok());
}

#[wasm_bindgen_test]
async fn an_unknown_preset_or_document_is_refused() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::text_document(&["one"]), None).unwrap();
    let err = engine
        .save_compressed_to_bytes(doc, "tiny".into(), false)
        .err()
        .unwrap();
    assert_eq!(error_name(err), "UnsupportedOperation");
    let err = engine.compression_refusal(99).err().unwrap();
    assert_eq!(error_name(err), "DocumentNotFound");
}
