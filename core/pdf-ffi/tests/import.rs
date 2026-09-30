//! Integration tests for `import_pdf`: another PDF's pages added to an open
//! handle across the UniFFI boundary, and every path that later has to
//! materialize them — the save, an extraction, undo/redo, a content read.
//!
//! `pdf_save`'s own tests pin how imported pages are grafted on a save. None
//! of them proves the handle keeps the source the save needs, which is the
//! one thing this boundary adds, and what these tests are for.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::xref::XrefType;
use lopdf::{EncryptionState, EncryptionVersion, Object, Permissions};
use rand::{rngs::OsRng, RngCore};

use pdf_ffi::{
    extract_pages_to_pdf, import_pdf, open_from_bytes, redo, save_to_bytes, undo, FfiError,
    FfiSaveIntent, FfiSignatureAcknowledgement,
};

/// A `label_prefix page N` document, unencrypted, saved to bytes.
fn multi_page_pdf(pages: u32, label_prefix: &str) -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(pages, label_prefix);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture document");
    bytes
}

/// A two-page `label_prefix` document, AES-128 encrypted under freshly
/// generated passwords with `permissions`. Returns the bytes and the user
/// password.
fn encrypted_pdf(label_prefix: &str, permissions: Permissions) -> (Vec<u8>, String) {
    let mut rng = OsRng;
    let user_password = format!("user-{:016x}", rng.next_u64());
    let owner_password = format!("owner-{:016x}", rng.next_u64());

    let mut doc = gen_fixtures::build_multi_page_document(2, label_prefix);
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = Object::string_literal("import-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);

    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: &owner_password,
        user_password: &user_password,
        permissions,
    };
    let state = EncryptionState::try_from(version).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    (bytes, user_password)
}

fn signed_pdf() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/signed/rsa2048_sha256.pdf"
    ))
    .expect("read the signed fixture")
}

/// The literal string each page's lone `Tj` carries, in page order — how
/// these tests tell one fixture page from another after a round trip.
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

fn save(handle: &pdf_ffi::DocumentHandle) -> Vec<u8> {
    save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("the document saves")
}

#[test]
fn importing_at_the_end_appends_every_source_page_in_order() {
    let handle = open_from_bytes(multi_page_pdf(2, "base"), None).expect("base opens");

    let report =
        import_pdf(&handle, multi_page_pdf(3, "added"), None, 2).expect("the import succeeds");

    assert_eq!(report.page_count, 3);
    assert!(report.warnings.is_empty());
    assert_eq!(handle.page_count(), 5);
    assert_eq!(
        page_labels_in_order(&save(&handle)),
        vec![
            "base page 0",
            "base page 1",
            "added page 0",
            "added page 1",
            "added page 2"
        ]
    );
}

#[test]
fn importing_at_zero_puts_the_pages_first() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    import_pdf(&handle, multi_page_pdf(1, "added"), None, 0).expect("the import succeeds");

    assert_eq!(
        page_labels_in_order(&save(&handle)),
        vec!["added page 0", "base page 0"]
    );
}

#[test]
fn two_imports_keep_both_sources() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    import_pdf(&handle, multi_page_pdf(1, "first"), None, 1).expect("first import");
    import_pdf(&handle, multi_page_pdf(1, "second"), None, 2).expect("second import");

    assert_eq!(
        page_labels_in_order(&save(&handle)),
        vec!["base page 0", "first page 0", "second page 0"]
    );
}

#[test]
fn one_undo_takes_the_whole_import_back_and_a_redo_can_still_save_it() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    import_pdf(&handle, multi_page_pdf(2, "added"), None, 1).expect("the import succeeds");

    assert!(undo(&handle));
    assert_eq!(handle.page_count(), 1);
    assert_eq!(page_labels_in_order(&save(&handle)), vec!["base page 0"]);

    assert!(redo(&handle));
    assert_eq!(
        page_labels_in_order(&save(&handle)),
        vec!["base page 0", "added page 0", "added page 1"],
        "the handle must keep the source after the undo, or the redo cannot be saved"
    );
}

#[test]
fn an_extraction_can_take_an_imported_page() {
    let handle = open_from_bytes(multi_page_pdf(2, "base"), None).expect("base opens");
    import_pdf(&handle, multi_page_pdf(1, "added"), None, 2).expect("the import succeeds");

    let extracted = extract_pages_to_pdf(&handle, vec![1, 2]).expect("extraction succeeds");

    assert_eq!(
        page_labels_in_order(&extracted),
        vec!["base page 1", "added page 0"]
    );
}

#[test]
fn an_imported_page_reads_its_own_content() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    import_pdf(&handle, multi_page_pdf(1, "added"), None, 1).expect("the import succeeds");

    let content = handle
        .read_page_content(1)
        .expect("an imported page reads from its source");

    assert!(
        content
            .text_runs
            .iter()
            .any(|run| run.text == "added page 0"),
        "got {:?}",
        content.text_runs
    );
}

#[test]
fn an_imported_page_reports_its_dimensions() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    import_pdf(&handle, multi_page_pdf(1, "added"), None, 1).expect("the import succeeds");

    let dimensions = handle.page_dimensions();

    assert_eq!(dimensions.len(), 2);
    assert_eq!(
        dimensions[1], dimensions[0],
        "the fixture pages share a size"
    );
}

#[test]
fn an_encrypted_source_asks_for_its_password_and_then_imports() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    let (source, password) = encrypted_pdf("locked", Permissions::all());

    // Either variant means "ask for the password": which one a missing
    // password produces is `pdf_manip`'s business, not this boundary's.
    assert!(matches!(
        import_pdf(&handle, source.clone(), None, 1),
        Err(FfiError::PasswordRequired | FfiError::WrongPassword)
    ));
    assert!(matches!(
        import_pdf(&handle, source.clone(), Some("not-it".into()), 1),
        Err(FfiError::WrongPassword)
    ));
    assert_eq!(handle.page_count(), 1, "a refused import adds nothing");

    import_pdf(&handle, source, Some(password), 1).expect("the right password imports");
    assert_eq!(
        page_labels_in_order(&save(&handle)),
        vec!["base page 0", "locked page 0", "locked page 1"]
    );
}

#[test]
fn a_source_that_forbids_copying_is_refused() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    let (source, password) = encrypted_pdf("no-copy", Permissions::PRINTABLE);

    assert!(matches!(
        import_pdf(&handle, source, Some(password), 1),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert_eq!(handle.page_count(), 1);
}

#[test]
fn a_signed_source_is_refused_with_the_reason() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    match import_pdf(&handle, signed_pdf(), None, 1) {
        Err(FfiError::UnsupportedOperation { detail }) => {
            assert!(detail.contains("signature"), "got {detail:?}")
        }
        other => panic!("expected a signature refusal, got {other:?}"),
    }
    assert_eq!(handle.page_count(), 1);
}

#[test]
fn an_index_past_the_end_is_refused() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    assert!(matches!(
        import_pdf(&handle, multi_page_pdf(1, "added"), None, 2),
        Err(FfiError::PageIndexOutOfBounds { index: 2 })
    ));
}

#[test]
fn bytes_that_are_not_a_pdf_are_refused() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    assert!(import_pdf(&handle, b"not a pdf".to_vec(), None, 1).is_err());
    assert_eq!(handle.page_count(), 1);
}

#[test]
fn a_document_that_forbids_assembly_refuses_the_import() {
    let (base, password) =
        encrypted_pdf("no-assembly", Permissions::all() - Permissions::ASSEMBLABLE);
    let handle = open_from_bytes(base, Some(password)).expect("base opens");

    assert!(matches!(
        import_pdf(&handle, multi_page_pdf(1, "added"), None, 2),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert_eq!(handle.page_count(), 2);
}
