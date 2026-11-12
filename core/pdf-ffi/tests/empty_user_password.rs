//! A PDF whose user password is **empty** opens with no prompt on every
//! shell, yet its `/P` can still forbid copying, annotating and assembly.
//! The handle must report and enforce those restrictions exactly as it does
//! for a document opened with a typed user password.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::xref::XrefType;
use pdf_ffi::{
    apply_edit, open_from_bytes, open_with_passwords_from_bytes, save_to_bytes, FfiEditCommand,
    FfiError, FfiSaveIntent, FfiSignatureAcknowledgement,
};

const OWNER_PASSWORD: &str = "owner-promptless";

/// One line of Standard-14 text, AES-128, empty user password, printable only.
fn promptless_pdf() -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
    // Classic xref table: lopdf cannot re-hydrate objects out of an encrypted
    // cross-reference stream at load time. The handler keys off `/ID`.
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = lopdf::Object::string_literal("promptless-ffi-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);

    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: OWNER_PASSWORD,
        user_password: "",
        permissions: lopdf::Permissions::PRINTABLE,
    })
    .expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

#[test]
fn a_promptless_open_reports_every_restriction() {
    let handle = open_from_bytes(promptless_pdf(), None).expect("opens with no password");

    assert!(!handle.text_extraction_allowed());
    assert!(!handle.annotation_editing_allowed());
    assert!(!handle.content_editing_allowed());
    assert!(!handle.form_field_editing_allowed());
    assert!(
        !handle.full_rewrite_allowed(),
        "the owner password is unknown, so a rewrite cannot re-apply it"
    );
}

#[test]
fn a_promptless_open_withholds_text() {
    let handle = open_from_bytes(promptless_pdf(), None).unwrap();

    assert!(matches!(
        handle.text_runs(0),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert!(matches!(
        handle.search("Hello".to_string()),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert!(matches!(
        handle.read_page_content(0),
        Err(FfiError::UnsupportedOperation { .. })
    ));
}

#[test]
fn a_promptless_open_refuses_assembly_and_records_nothing() {
    let handle = open_from_bytes(promptless_pdf(), None).unwrap();

    assert!(matches!(
        apply_edit(
            &handle,
            FfiEditCommand::RotatePage {
                page: 0,
                delta_degrees: 90,
            },
        ),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert!(!handle.can_undo());
}

#[test]
fn the_owner_password_lifts_the_restrictions_and_a_save_keeps_them() {
    let handle = open_from_bytes(promptless_pdf(), Some(OWNER_PASSWORD.to_string()))
        .expect("the owner password opens");
    assert!(handle.text_extraction_allowed());

    apply_edit(
        &handle,
        FfiEditCommand::RotatePage {
            page: 0,
            delta_degrees: 90,
        },
    )
    .expect("the owner may rotate");
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("an incremental save re-encrypts from lopdf's retained state");

    let reopened = open_from_bytes(saved, None).expect("still opens with no prompt");
    assert!(
        !reopened.text_extraction_allowed(),
        "the saved file must keep its restrictions"
    );
}

#[test]
fn both_passwords_allow_a_rewrite_that_keeps_the_protection() {
    let handle =
        open_with_passwords_from_bytes(promptless_pdf(), String::new(), OWNER_PASSWORD.into())
            .expect("both passwords open");
    assert!(handle.full_rewrite_allowed());

    // A page insert is structural: it forces the full-rewrite writer.
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: pdf_ffi::FfiPageSize::A4,
            orientation: pdf_ffi::FfiOrientation::Portrait,
        },
    )
    .expect("a structural edit is allowed with both passwords");
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("the rewrite re-applies both passwords");

    let reopened = open_from_bytes(saved, None).expect("still opens with no prompt");
    assert!(!reopened.text_extraction_allowed());
    assert_eq!(reopened.page_count(), 2);
}
