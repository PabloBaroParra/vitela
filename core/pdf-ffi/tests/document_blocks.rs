//! Integration tests for the Documents-view surface: `document_blocks`, and
//! the two block commands `RemovePages` and `RotatePages`.
//!
//! `pdf_document::derive_blocks` and the core commands have their own tests.
//! What these pin is what the boundary adds: blocks reported by position, an
//! imported block named by the id its import reported, and a whole block
//! deleted or turned as one undo step addressed by positions.

use std::sync::Arc;

use lopdf::{Object, Permissions};

use pdf_ffi::{
    apply_edit, document_blocks, import_pdf, open_from_bytes, redo, save_to_bytes, undo,
    FfiBlockSource, FfiDocumentBlock, FfiEditCommand, FfiError, FfiPageRotation, FfiSaveIntent,
    FfiSignatureAcknowledgement,
};

/// A `label_prefix page N` document, unencrypted, saved to bytes.
fn multi_page_pdf(pages: u32, label_prefix: &str) -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(pages, label_prefix);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture document");
    bytes
}

/// The literal string each page's lone `Tj` carries, in page order.
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

fn block(source: FfiBlockSource, part: Option<u32>, start: u32, count: u32) -> FfiDocumentBlock {
    FfiDocumentBlock {
        source,
        part,
        start,
        count,
    }
}

fn rotations(handle: &Arc<pdf_ffi::DocumentHandle>) -> Vec<FfiPageRotation> {
    handle
        .page_dimensions()
        .into_iter()
        .map(|page| page.rotation)
        .collect()
}

#[test]
fn a_freshly_opened_document_is_one_block() {
    let handle = open_from_bytes(multi_page_pdf(3, "base"), None).expect("base opens");

    assert_eq!(
        document_blocks(&handle),
        vec![block(FfiBlockSource::Base, None, 0, 3)]
    );
}

#[test]
fn an_import_in_the_middle_splits_the_base_into_numbered_parts() {
    let handle = open_from_bytes(multi_page_pdf(3, "base"), None).expect("base opens");

    let report =
        import_pdf(&handle, multi_page_pdf(2, "added"), None, 1).expect("the import succeeds");

    assert_eq!(
        document_blocks(&handle),
        vec![
            block(FfiBlockSource::Base, Some(1), 0, 1),
            block(
                FfiBlockSource::Imported {
                    id: report.source_id
                },
                None,
                1,
                2
            ),
            block(FfiBlockSource::Base, Some(2), 3, 2),
        ]
    );
}

#[test]
fn each_import_reports_its_own_source_id() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");

    let first = import_pdf(&handle, multi_page_pdf(1, "first"), None, 1).expect("first import");
    let second = import_pdf(&handle, multi_page_pdf(1, "second"), None, 2).expect("second import");

    assert_ne!(first.source_id, second.source_id);
    assert_eq!(
        document_blocks(&handle),
        vec![
            block(FfiBlockSource::Base, None, 0, 1),
            block(
                FfiBlockSource::Imported {
                    id: first.source_id
                },
                None,
                1,
                1
            ),
            block(
                FfiBlockSource::Imported {
                    id: second.source_id
                },
                None,
                2,
                1
            ),
        ]
    );
}

#[test]
fn a_blank_page_is_its_own_block() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 1,
            size: pdf_ffi::FfiPageSize::A4,
            orientation: pdf_ffi::FfiOrientation::Portrait,
        },
    )
    .expect("the blank page goes in");

    assert_eq!(
        document_blocks(&handle),
        vec![
            block(FfiBlockSource::Base, None, 0, 1),
            block(FfiBlockSource::Blank, None, 1, 1),
        ]
    );
}

#[test]
fn removing_a_block_is_one_undo_step() {
    let handle = open_from_bytes(multi_page_pdf(1, "base"), None).expect("base opens");
    import_pdf(&handle, multi_page_pdf(3, "added"), None, 1).expect("the import succeeds");

    apply_edit(&handle, FfiEditCommand::RemovePages { index: 1, count: 3 })
        .expect("the block is removed");

    assert_eq!(handle.page_count(), 1);
    assert_eq!(page_labels_in_order(&save(&handle)), vec!["base page 0"]);

    assert!(undo(&handle));
    assert_eq!(
        page_labels_in_order(&save(&handle)),
        vec![
            "base page 0",
            "added page 0",
            "added page 1",
            "added page 2"
        ]
    );
    assert!(redo(&handle));
    assert_eq!(handle.page_count(), 1);
}

#[test]
fn removing_a_run_past_the_end_is_refused_and_changes_nothing() {
    let handle = open_from_bytes(multi_page_pdf(3, "base"), None).expect("base opens");

    assert!(matches!(
        apply_edit(&handle, FfiEditCommand::RemovePages { index: 2, count: 2 }),
        Err(FfiError::PageIndexOutOfBounds { index: 2 })
    ));
    assert_eq!(handle.page_count(), 3);
}

#[test]
fn rotating_a_block_turns_every_page_of_it_as_one_undo_step() {
    let handle = open_from_bytes(multi_page_pdf(4, "base"), None).expect("base opens");

    apply_edit(
        &handle,
        FfiEditCommand::RotatePages {
            from: 1,
            count: 2,
            delta_degrees: 90,
        },
    )
    .expect("the block turns");

    assert_eq!(
        rotations(&handle),
        vec![
            FfiPageRotation::None,
            FfiPageRotation::Clockwise90,
            FfiPageRotation::Clockwise90,
            FfiPageRotation::None,
        ]
    );
    assert!(undo(&handle));
    assert_eq!(rotations(&handle), vec![FfiPageRotation::None; 4]);
}

#[test]
fn rotating_a_run_past_the_end_is_refused_and_turns_nothing() {
    let handle = open_from_bytes(multi_page_pdf(2, "base"), None).expect("base opens");

    assert!(matches!(
        apply_edit(
            &handle,
            FfiEditCommand::RotatePages {
                from: 1,
                count: 2,
                delta_degrees: 90,
            },
        ),
        Err(FfiError::PageIndexOutOfBounds { index: 1 })
    ));
    assert_eq!(rotations(&handle), vec![FfiPageRotation::None; 2]);
}

#[test]
fn moving_a_block_next_to_its_twin_merges_the_parts() {
    let handle = open_from_bytes(multi_page_pdf(2, "base"), None).expect("base opens");
    let report = import_pdf(&handle, multi_page_pdf(1, "added"), None, 1).expect("import");

    // base part 1 (0), added (1), base part 2 (2): move the import to the end.
    apply_edit(
        &handle,
        FfiEditCommand::MovePages {
            from: 1,
            count: 1,
            to: 2,
        },
    )
    .expect("the block moves");

    assert_eq!(
        document_blocks(&handle),
        vec![
            block(FfiBlockSource::Base, None, 0, 2),
            block(
                FfiBlockSource::Imported {
                    id: report.source_id
                },
                None,
                2,
                1
            ),
        ]
    );
}

#[test]
fn a_document_that_forbids_assembly_refuses_both_block_commands() {
    // `document_assembly_is_allowed` accepts bit 4 *or* bit 11, so both go:
    // without bit 4 alone, a rotation would still be allowed.
    let (base, password) = common::encrypted_pdf(
        "no-assembly",
        Permissions::all() - Permissions::ASSEMBLABLE - Permissions::MODIFIABLE,
    );
    let handle = open_from_bytes(base, Some(password)).expect("base opens");

    for command in [
        FfiEditCommand::RemovePages { index: 0, count: 1 },
        FfiEditCommand::RotatePages {
            from: 0,
            count: 2,
            delta_degrees: 90,
        },
    ] {
        assert!(
            matches!(
                apply_edit(&handle, command.clone()),
                Err(FfiError::UnsupportedOperation { .. })
            ),
            "{command:?}"
        );
    }
    assert_eq!(handle.page_count(), 2);
    assert_eq!(rotations(&handle), vec![FfiPageRotation::None; 2]);
}

mod common {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
    use lopdf::xref::XrefType;
    use lopdf::{EncryptionState, EncryptionVersion, Object, Permissions};
    use rand::{rngs::OsRng, RngCore};

    /// A two-page `label_prefix` document, AES-128 encrypted under freshly
    /// generated passwords with `permissions`. Returns the bytes and the user
    /// password.
    pub(super) fn encrypted_pdf(label_prefix: &str, permissions: Permissions) -> (Vec<u8>, String) {
        let mut rng = OsRng;
        let user_password = format!("user-{:016x}", rng.next_u64());
        let owner_password = format!("owner-{:016x}", rng.next_u64());

        let mut doc = gen_fixtures::build_multi_page_document(2, label_prefix);
        doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        let file_id = Object::string_literal("blocks-fixture-id");
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
}
