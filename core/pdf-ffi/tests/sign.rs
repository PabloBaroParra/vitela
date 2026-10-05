//! Integration tests for signing across the UniFFI boundary: unlocking a
//! `.pfx`, listing what it can sign as, and which bytes `sign_to_bytes`
//! signs.
//!
//! `pdf_sign`'s own tests pin the signature itself. What this boundary adds
//! is the choice of bytes — the file as opened, or the session as it would
//! be saved — and the gate, so that is what these tests are for.

use std::collections::BTreeMap;
use std::sync::Arc;

use gen_fixtures::signed::{self_signed_pfx, SignedAlgorithm, FIXTURE_PFX_PASSWORD};
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::xref::XrefType;
use lopdf::{EncryptionState, EncryptionVersion, Object, Permissions};

use pdf_ffi::{
    apply_edit, open_from_bytes, open_signing_certificate, save_to_bytes, sign_to_bytes,
    signing_refusal, undo, FfiColor, FfiEditCommand, FfiError, FfiRect, FfiSaveIntent,
    FfiSignatureAcknowledgement, SigningCertificate,
};

#[test]
fn encrypted_signing_reopens_with_both_credentials_and_preserves_protection() {
    let handle = open_from_bytes(two_page_pdf(), None).expect("open");
    let original_text = handle
        .read_page_content(0)
        .expect("source content")
        .text_runs;
    assert!(
        !original_text.is_empty(),
        "fixture must exercise encrypted text"
    );
    let protected = pdf_ffi::protect_to_bytes(
        &handle,
        "sign-user".into(),
        "sign-owner".into(),
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("protect");
    let handle = pdf_ffi::open_with_passwords_from_bytes(
        protected.clone(),
        "sign-user".into(),
        "sign-owner".into(),
    )
    .expect("dual-password open");
    let (certificate, id) = certificate();
    let signed = sign_to_bytes(&handle, certificate, id).expect("encrypted sign");
    assert!(signed.starts_with(&protected));
    assert!(matches!(
        open_from_bytes(signed.clone(), None),
        Err(FfiError::PasswordRequired)
    ));
    assert!(open_from_bytes(signed.clone(), Some("sign-user".into())).is_ok());
    let reopened =
        pdf_ffi::reopen_signed_document(&handle, signed).expect("reopen with session credentials");
    assert_eq!(reopened.page_count(), 2);
    assert!(pdf_ffi::extract_source_is_signed(&reopened));
    assert_eq!(
        reopened
            .read_page_content(0)
            .expect("signed content")
            .text_runs,
        original_text
    );
    // A structural rewrite still needs both credentials after signed reopen.
    apply_edit(
        &reopened,
        FfiEditCommand::InsertBlankPage {
            index: 2,
            size: pdf_ffi::FfiPageSize::A4,
            orientation: pdf_ffi::FfiOrientation::Portrait,
        },
    )
    .expect("structural edit");
    let rewritten = save_to_bytes(
        &reopened,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::ProceedAndInvalidate,
    )
    .expect("retain both password roles");
    assert!(pdf_ffi::open_with_passwords_from_bytes(
        rewritten,
        "sign-user".into(),
        "sign-owner".into()
    )
    .is_ok());
}

fn two_page_pdf() -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(2, "sign");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture document");
    bytes
}

fn certificate() -> (Arc<SigningCertificate>, String) {
    let pfx = self_signed_pfx(SignedAlgorithm::P256Sha256, "Test Signer").expect("generate pfx");
    let certificate =
        open_signing_certificate(pfx, FIXTURE_PFX_PASSWORD.to_string()).expect("unlock pfx");
    let id = certificate.identities()[0].id.clone();
    (certificate, id)
}

/// The `/T` of every signature field in `bytes`.
fn signature_field_names(bytes: &[u8]) -> Vec<String> {
    let document = lopdf::Document::load_mem(bytes).expect("signed bytes parse");
    let mut names: Vec<String> = document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| dict.get(b"FT").and_then(Object::as_name).ok() == Some(b"Sig".as_slice()))
        .filter_map(|dict| dict.get(b"T").and_then(Object::as_str).ok())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect();
    names.sort();
    names
}

fn highlight() -> FfiEditCommand {
    FfiEditCommand::AddHighlight {
        page: 0,
        rect: FfiRect {
            x: 72.0,
            y: 700.0,
            width: 100.0,
            height: 12.0,
        },
        color: FfiColor {
            r: 255,
            g: 230,
            b: 0,
        },
    }
}

#[test]
fn a_wrong_password_does_not_unlock_the_certificate() {
    let pfx = self_signed_pfx(SignedAlgorithm::P256Sha256, "Test Signer").expect("generate pfx");
    let error = open_signing_certificate(pfx, "not the password".to_string())
        .expect_err("a wrong password must not unlock");
    assert!(matches!(error, FfiError::WrongPassword));
}

#[test]
fn an_unlocked_certificate_names_its_identity() {
    let (certificate, _) = certificate();
    let identities = certificate.identities();
    assert_eq!(identities.len(), 1);
    assert!(identities[0].display_name.contains("Test Signer"));
}

#[test]
fn an_unedited_document_is_signed_as_a_new_revision_of_its_own_bytes() {
    let original = two_page_pdf();
    let handle = open_from_bytes(original.clone(), None).expect("open");
    let (certificate, id) = certificate();

    let signed = sign_to_bytes(&handle, certificate, id).expect("sign");

    assert!(signed.starts_with(&original), "signing appends a revision");
    assert_eq!(signature_field_names(&signed), ["Signature_1"]);
    assert!(
        open_from_bytes(signed, None).is_ok(),
        "the signed file reopens"
    );
}

#[test]
fn a_signed_document_keeps_its_signature_and_gets_a_second() {
    let original = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/signed/p256_sha256.pdf"
    ))
    .expect("read signed fixture");
    let first = signature_field_names(&original);
    let handle = open_from_bytes(original.clone(), None).expect("open");
    let (certificate, id) = certificate();

    let signed = sign_to_bytes(&handle, certificate, id).expect("sign");

    assert!(
        signed.starts_with(&original),
        "the first signature's bytes are untouched"
    );
    let names = signature_field_names(&signed);
    assert_eq!(names.len(), first.len() + 1);
    assert!(first.iter().all(|name| names.contains(name)));
}

#[test]
fn an_applied_edit_is_signed_as_the_session_would_save() {
    let handle = open_from_bytes(two_page_pdf(), None).expect("open");
    apply_edit(&handle, highlight()).expect("highlight");
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");
    let (certificate, id) = certificate();

    let signed = sign_to_bytes(&handle, certificate, id).expect("sign");

    assert!(
        signed.starts_with(&saved),
        "what is signed is what Save wrote"
    );
}

#[test]
fn an_undone_edit_signs_the_file_as_opened() {
    let original = two_page_pdf();
    let handle = open_from_bytes(original.clone(), None).expect("open");
    apply_edit(&handle, highlight()).expect("highlight");
    assert!(undo(&handle), "undo");
    let (certificate, id) = certificate();

    let signed = sign_to_bytes(&handle, certificate, id).expect("sign");

    assert!(signed.starts_with(&original));
}

#[test]
fn a_document_that_forbids_annotations_is_not_signed() {
    let mut doc = gen_fixtures::build_multi_page_document(1, "restricted");
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = Object::string_literal("sign-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: "sign-owner",
        user_password: "sign-user",
        permissions: Permissions::PRINTABLE | Permissions::MODIFIABLE | Permissions::COPYABLE,
    };
    let state = EncryptionState::try_from(version).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    let handle =
        open_from_bytes(bytes, Some("sign-user".to_string())).expect("open with the user password");
    let (certificate, id) = certificate();

    assert!(signing_refusal(&handle).is_some());
    let error = sign_to_bytes(&handle, certificate, id).expect_err("refused");
    assert!(matches!(error, FfiError::UnsupportedOperation { .. }));
}

#[test]
fn an_identity_the_certificate_does_not_hold_is_refused() {
    let handle = open_from_bytes(two_page_pdf(), None).expect("open");
    let (certificate, _) = certificate();

    let error = sign_to_bytes(&handle, certificate, "someone else".to_string())
        .expect_err("unknown identity");

    assert!(
        matches!(error, FfiError::UnsupportedOperation { detail } if detail.contains("certificate file"))
    );
}
