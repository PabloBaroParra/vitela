//! The `Engine`'s protect surface against the real pdfium, in a headless
//! browser (B26 T-251). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::EditCommand;
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

const OPEN: &str = "open-pw";
const PERMISSIONS: &str = "permissions-pw";

#[wasm_bindgen_test]
async fn the_copy_opens_with_either_password_and_carries_the_pending_edits() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["one", "two", "three"]), None)
        .unwrap();
    assert!(engine.protection_refusal(doc).unwrap().is_none());
    assert!(!engine.protection_invalidates_signatures(doc).unwrap());
    engine
        .apply_edit(doc, &EditCommand::remove_pages(0, 1))
        .unwrap();

    let bytes = engine
        .protect_to_bytes(doc, OPEN.into(), PERMISSIONS.into(), false)
        .unwrap();
    assert_eq!(
        error_name(engine.open(bytes.clone(), None).err().unwrap()),
        "PasswordRequired"
    );
    let by_open = engine.open(bytes.clone(), Some(OPEN.into())).unwrap();
    assert_eq!(engine.page_count(by_open).unwrap(), 2);
    let mut page = engine.render_page(by_open, 0, 36, false).unwrap();
    assert!(!page.take_rgba().is_empty());
    let by_permissions = engine.open(bytes, Some(PERMISSIONS.into())).unwrap();
    assert_eq!(engine.page_count(by_permissions).unwrap(), 2);

    // Not an edit: the session still has its one step to undo.
    assert!(engine.can_undo(doc).unwrap());
    assert!(!engine.can_redo(doc).unwrap());
}

#[wasm_bindgen_test]
async fn missing_or_equal_passwords_are_refused() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::text_document(&["one"]), None).unwrap();
    for (open, permissions) in [("", PERMISSIONS), (OPEN, ""), (OPEN, OPEN)] {
        let err = engine
            .protect_to_bytes(doc, open.into(), permissions.into(), false)
            .err()
            .unwrap();
        assert_eq!(error_name(err), "InvalidPasswords");
    }
}

#[wasm_bindgen_test]
async fn a_signed_document_needs_the_users_yes() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::SIGNED.to_vec(), None).unwrap();
    assert!(engine.protection_invalidates_signatures(doc).unwrap());
    let err = engine
        .protect_to_bytes(doc, OPEN.into(), PERMISSIONS.into(), false)
        .err()
        .unwrap();
    assert_eq!(error_name(err), "SignaturesWouldBeInvalidated");
    assert!(engine
        .protect_to_bytes(doc, OPEN.into(), PERMISSIONS.into(), true)
        .is_ok());
}

#[wasm_bindgen_test]
async fn a_document_that_withholds_modification_is_refused_ahead() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::restricted_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    let refusal = engine.protection_refusal(doc).unwrap().expect("refused");
    assert!(refusal.contains("protection changes"), "{refusal}");
    let err = engine
        .protect_to_bytes(doc, OPEN.into(), PERMISSIONS.into(), false)
        .err()
        .unwrap();
    assert_eq!(error_name(err), "EditNotPermitted");
    let err = engine.protection_refusal(99).err().unwrap();
    assert_eq!(error_name(err), "DocumentNotFound");
}
