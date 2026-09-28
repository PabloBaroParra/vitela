//! Integration tests for `extract_pages_to_pdf`: a real PDF pruned across the
//! UniFFI boundary and reopened, the two refusal gates, and proof that the
//! live handle is never mutated by an extraction.
//!
//! `pdf_document::prune`'s own tests (moved from the Linux shell, see that
//! crate's `prune` module) pin the cutting rules in isolation — which runs
//! are dropped, in what order, and that the log records them. None of that
//! proves the rules add up to a file `pdf-ffi`'s own boundary hands back
//! correctly, which is what these tests are for.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::xref::XrefType;
use lopdf::{EncryptionState, EncryptionVersion, Object, Permissions};
use rand::{rngs::OsRng, RngCore};

use pdf_ffi::{
    extract_pages_to_pdf, extract_source_is_signed, open_from_bytes,
    open_with_passwords_from_bytes, plan_split, FfiError,
};

/// A `label_prefix` page N` document, unencrypted, saved to bytes.
fn multi_page_pdf(pages: u32, label_prefix: &str) -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(pages, label_prefix);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture document");
    bytes
}

/// Same shape as [`multi_page_pdf`], but AES-128 encrypted with `/P` clearing
/// the copy bit — the document `text_extraction_allowed` must refuse.
/// `PRINTABLE` only, no `COPYABLE`: the same restricted shape
/// `pdf-manip`'s own `text_extraction_permission` integration tests build,
/// reconstructed here because that helper lives in a different crate's
/// private test module.
fn no_copy_pdf(user_password: &str, owner_password: &str) -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(2, "restricted");
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = Object::string_literal("extract-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);

    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password,
        user_password,
        permissions: Permissions::PRINTABLE,
    };
    let state = EncryptionState::try_from(version).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

/// Keep test credentials local to each generated fixture rather than
/// embedding password values in calls across the FFI boundary.
fn no_copy_fixture() -> (Vec<u8>, String, String) {
    let mut rng = OsRng;
    let user_password = format!("user-{:016x}", rng.next_u64());
    let owner_password = format!("owner-{:016x}", rng.next_u64());
    let bytes = no_copy_pdf(&user_password, &owner_password);
    (bytes, user_password, owner_password)
}

/// The literal string a page's lone `Tj` operation carries — how these tests
/// tell one fixture page from another after a round trip, the same trick
/// `pdf-manip`'s test support module uses for the same reason.
fn page_labels_in_order(bytes: &[u8]) -> Vec<String> {
    let doc = lopdf::Document::load_mem(bytes).expect("reopened bytes must parse as a PDF");
    doc.get_pages()
        .into_values()
        .map(|page_id| {
            let content = doc
                .get_and_decode_page_content(page_id)
                .expect("decode page content");
            content
                .operations
                .iter()
                .find(|op| op.operator == "Tj")
                .and_then(|op| op.operands.first())
                .and_then(|operand| match operand {
                    Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).to_string()),
                    _ => None,
                })
                .expect("every fixture page carries one Tj label")
        })
        .collect()
}

#[test]
fn extracting_two_pages_writes_a_two_page_pdf_in_document_order() {
    let bytes = multi_page_pdf(5, "sample");
    let handle = open_from_bytes(bytes, None).expect("unencrypted fixture must open");

    let extracted = extract_pages_to_pdf(&handle, vec![0, 3]).expect("extraction should succeed");

    let labels = page_labels_in_order(&extracted);
    assert_eq!(labels, vec!["sample page 0", "sample page 3"]);
}

#[test]
fn split_plan_and_extraction_write_every_page_to_exactly_one_part() {
    let handle = open_from_bytes(multi_page_pdf(5, "split"), None).expect("fixture opens");
    let parts =
        plan_split("2,4".into(), handle.page_count(), "split.pdf".into()).expect("valid cuts");

    let actual: Vec<_> = parts
        .into_iter()
        .map(|part| {
            let bytes = extract_pages_to_pdf(&handle, (part.first..=part.last).collect())
                .expect("part extraction succeeds");
            (part.file_name, page_labels_in_order(&bytes))
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (
                "split-part1.pdf".into(),
                vec!["split page 0".into(), "split page 1".into()]
            ),
            (
                "split-part2.pdf".into(),
                vec!["split page 2".into(), "split page 3".into()]
            ),
            ("split-part3.pdf".into(), vec!["split page 4".into()]),
        ]
    );
    assert_eq!(handle.page_count(), 5, "the source remains unchanged");
}

#[test]
fn extracting_a_scattered_selection_preserves_document_order() {
    let bytes = multi_page_pdf(6, "order");
    let handle = open_from_bytes(bytes, None).expect("unencrypted fixture must open");

    let extracted =
        extract_pages_to_pdf(&handle, vec![0, 2, 5]).expect("extraction should succeed");

    assert_eq!(
        page_labels_in_order(&extracted),
        vec!["order page 0", "order page 2", "order page 5"]
    );
}

#[test]
fn extracting_does_not_change_the_live_handle_s_page_count() {
    let bytes = multi_page_pdf(4, "untouched");
    let handle = open_from_bytes(bytes, None).expect("unencrypted fixture must open");

    let before = handle.page_count();
    let _ = extract_pages_to_pdf(&handle, vec![0]).expect("extraction should succeed");

    assert_eq!(
        handle.page_count(),
        before,
        "an extraction must not mutate the document it was extracted from"
    );
}

#[test]
fn an_empty_selection_is_refused() {
    let bytes = multi_page_pdf(2, "empty");
    let handle = open_from_bytes(bytes, None).expect("unencrypted fixture must open");

    assert!(matches!(
        extract_pages_to_pdf(&handle, Vec::new()),
        Err(FfiError::InvalidPageSelection { .. })
    ));
}

#[test]
fn an_unsorted_selection_is_refused_instead_of_silently_dropping_pages() {
    let bytes = multi_page_pdf(3, "order");
    let handle = open_from_bytes(bytes, None).expect("unencrypted fixture must open");

    assert!(matches!(
        extract_pages_to_pdf(&handle, vec![2, 0]),
        Err(FfiError::InvalidPageSelection { .. })
    ));
}

#[test]
fn duplicate_pages_are_refused() {
    let bytes = multi_page_pdf(3, "duplicate");
    let handle = open_from_bytes(bytes, None).expect("unencrypted fixture must open");

    assert!(matches!(
        extract_pages_to_pdf(&handle, vec![0, 0]),
        Err(FfiError::InvalidPageSelection { .. })
    ));
}

#[test]
fn a_page_past_the_end_is_refused() {
    let bytes = multi_page_pdf(2, "bounds");
    let handle = open_from_bytes(bytes, None).expect("unencrypted fixture must open");

    assert!(matches!(
        extract_pages_to_pdf(&handle, vec![5]),
        Err(FfiError::PageIndexOutOfBounds { index: 5 })
    ));
}

/// The gate this whole surface exists to enforce: `/P` withholding the copy
/// bit refuses the extraction before anything is pruned.
#[test]
fn a_document_that_forbids_copying_refuses_extraction() {
    let (bytes, user_password, _) = no_copy_fixture();
    let handle = open_from_bytes(bytes, Some(user_password)).expect("fixture must open");

    assert!(matches!(
        extract_pages_to_pdf(&handle, vec![0]),
        Err(FfiError::UnsupportedOperation { .. })
    ));
}

/// The owner credential bypasses the permission bitmask, the same rule
/// `pdf_manip::text_extraction_is_allowed` pins directly. Opened with
/// *both* passwords, not just the owner one: a full rewrite (which any
/// extraction needs) has its own, unrelated gate that requires both — see
/// `an_incomplete_credentials_open_is_refused_even_when_copying_is_allowed`
/// for that gate pinned on its own.
#[test]
fn the_owner_of_a_no_copy_document_may_still_extract() {
    let (bytes, user_password, owner_password) = no_copy_fixture();
    let handle = open_with_passwords_from_bytes(bytes, user_password, owner_password)
        .expect("fixture must open with both passwords");

    assert!(extract_pages_to_pdf(&handle, vec![0]).is_ok());
}

/// The second gate, isolated: even a document that grants copying cannot be
/// extracted from when opened with only one of its two passwords, because
/// the extraction a rewrite produces could never reproduce its encryption.
#[test]
fn an_incomplete_credentials_open_is_refused_even_when_copying_is_allowed() {
    let (bytes, _, owner_password) = no_copy_fixture();
    // Opened with the owner password alone: copying is allowed (the owner
    // credential bypasses `/P`), but only one of the two passwords is known.
    let handle = open_from_bytes(bytes, Some(owner_password)).expect("fixture must open");

    assert!(matches!(
        extract_pages_to_pdf(&handle, vec![0]),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    // A shell asks `full_rewrite_allowed` up front, before offering the
    // Extract dialog at all — pin that it agrees with the refusal above.
    assert!(!handle.full_rewrite_allowed());
}

#[test]
fn a_document_open_with_both_passwords_may_be_fully_rewritten() {
    let (bytes, user_password, owner_password) = no_copy_fixture();
    let handle = open_with_passwords_from_bytes(bytes, user_password, owner_password)
        .expect("fixture must open with both passwords");

    assert!(handle.full_rewrite_allowed());
}

#[test]
fn an_unsigned_source_reports_no_signature() {
    let bytes = multi_page_pdf(1, "unsigned");
    let handle = open_from_bytes(bytes, None).expect("unencrypted fixture must open");

    assert!(!extract_source_is_signed(&handle));
}
