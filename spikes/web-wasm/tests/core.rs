//! Step 1: the lopdf-backed core opens a real fixture on wasm32.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/forms/reportlab_acroform.pdf");

#[wasm_bindgen_test]
fn opens_fixture_and_counts_pages() {
    let (doc, security) = pdf_manip::open_document_from_bytes(FIXTURE, None).unwrap();
    assert!(security.is_none());
    assert!(doc.page_count() >= 1);
}

#[wasm_bindgen_test]
fn encrypted_fixture_needs_password() {
    let bytes = include_bytes!("../../../tests/fixtures/encrypted/aes_128_user_and_owner.pdf");
    assert!(pdf_manip::open_document_from_bytes(bytes, None).is_err());
}

#[wasm_bindgen_test]
fn encrypted_fixture_opens_with_user_password() {
    let bytes = include_bytes!("../../../tests/fixtures/encrypted/aes_128_user_and_owner.pdf");
    let (doc, security) = pdf_manip::open_document_from_bytes(bytes, Some("user-aes-pass")).unwrap();
    assert!(security.is_some());
    assert!(doc.page_count() >= 1);
}

/// already_packed.pdf stores objects in an /ObjStm, which lopdf decodes
/// through rayon's par_iter. On wasm32-unknown-unknown rayon cannot spawn
/// threads; this proves it degrades to the calling thread instead of panicking.
#[wasm_bindgen_test]
fn object_stream_fixture_opens_through_rayon_path() {
    let bytes = include_bytes!("../../../tests/fixtures/compress/already_packed.pdf");
    let (doc, _) = pdf_manip::open_document_from_bytes(bytes, None).unwrap();
    assert!(doc.page_count() >= 1);
}

#[wasm_bindgen_test]
fn save_round_trips_in_memory() {
    let (mut doc, _) = pdf_manip::open_document_from_bytes(FIXTURE, None).unwrap();
    let pages = doc.page_count();
    let mut out = Vec::new();
    doc.as_lopdf_mut().save_to(&mut out).unwrap();
    let (reopened, _) = pdf_manip::open_document_from_bytes(&out, None).unwrap();
    assert_eq!(reopened.page_count(), pages);
}
