//! Saving a document whose user password is **empty** — it opens with no
//! prompt but its `/P` still restricts it.
//!
//! `pdf_manip`'s open path now reports that document's `SecurityContext`
//! (user role, empty user password) instead of `None`. These tests pin what
//! that means for each writer:
//!
//! - the incremental writer keeps re-encrypting from lopdf's retained state;
//! - a full rewrite opened with only the (empty) user password is refused,
//!   exactly like any single-password open — it used to write plaintext;
//! - with both passwords the rewrite re-applies the same protection;
//! - a compressed save, which is always a rewrite, is refused up front.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::content::{Content, Operation};
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::xref::XrefType;
use lopdf::{dictionary, EncryptionState, EncryptionVersion, Object, Permissions, Stream};
use pdf_document::{Annotation, AnnotationId, AnnotationKind, Color, Command, Credential, Rect};
use pdf_save::{save_document, SaveInput, SaveIntent, SignatureAcknowledgement};

const OWNER_PASSWORD: &str = "owner-promptless";

/// Two pages, AES-128, empty user password, printable only.
fn promptless_two_page_pdf() -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kid_ids = Vec::new();
    for label in ["one", "two"] {
        let content = Content {
            operations: vec![Operation::new("Tj", vec![Object::string_literal(label)])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        kid_ids.push(doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kid_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
            "Count" => kid_ids.len() as i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);

    // Classic xref table: lopdf cannot re-hydrate objects out of an encrypted
    // cross-reference stream at load time. The handler keys off `/ID`.
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = Object::string_literal("promptless-save-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);

    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: OWNER_PASSWORD,
        user_password: "",
        permissions: Permissions::PRINTABLE,
    })
    .expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

fn apply_command(document: &mut pdf_document::Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

fn highlight(page: u32) -> Command {
    Command::AddAnnotation(Annotation {
        id: AnnotationId(1),
        page: pdf_document::PageId(page),
        kind: AnnotationKind::Highlight {
            rect: Rect {
                x: 1.0,
                y: 2.0,
                width: 30.0,
                height: 10.0,
            },
            color: Color { r: 250, g: 0, b: 0 },
        },
    })
}

fn input<'a>(
    document: &'a pdf_document::Document,
    base: &'a pdf_manip::LopdfDocument,
    original_bytes: &'a [u8],
) -> SaveInput<'a> {
    SaveInput {
        document,
        base,
        original_bytes: Some(original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::none(),
    }
}

/// The saved bytes still open without a prompt, still restricted, and the
/// owner password still lifts the restriction.
fn assert_same_protection(saved: &[u8]) {
    let before = pdf_manip::read_security_context_from_bytes(&promptless_two_page_pdf(), None)
        .unwrap()
        .expect("fixture is encrypted");
    let (_, after) =
        pdf_manip::open_document_from_bytes(saved, None).expect("still opens with no prompt");
    let after = after.expect("the saved file must still be encrypted");

    assert_eq!(after.handler, before.handler);
    assert_eq!(after.permissions, before.permissions);
    assert!(!pdf_manip::text_extraction_is_allowed(Some(&after)));

    let (_, as_owner) = pdf_manip::open_document_from_bytes(saved, Some(OWNER_PASSWORD))
        .expect("the owner password must survive the save");
    assert_eq!(as_owner.unwrap().credential, Credential::Owner);
}

#[test]
fn an_incremental_save_keeps_the_protection() {
    let original = promptless_two_page_pdf();
    let (base, security) = pdf_manip::open_document_from_bytes(&original, None).unwrap();
    assert!(security.is_some(), "the open must report the restrictions");
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let page0 = document.pages[0].id;
    apply_command(&mut document, highlight(page0.0));

    let saved = save_document(input(&document, &base, &original)).expect("incremental save");

    assert_same_protection(&saved);
}

#[test]
fn a_full_rewrite_with_only_the_empty_user_password_is_refused_not_stripped() {
    let original = promptless_two_page_pdf();
    let (base, security) = pdf_manip::open_document_from_bytes(&original, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();
    apply_command(&mut document, Command::MovePage { from: 0, to: 1 });

    assert_eq!(
        pdf_save::full_rewrite_blocker(document.security.as_ref()),
        Some(pdf_save::RewriteBlocker::IncompleteCredentials),
        "the owner password is unknown, so the rewrite cannot re-apply it"
    );
    let result = save_document(input(&document, &base, &original));
    assert!(
        matches!(result, Err(pdf_save::SaveError::InvalidSaveRequest(_))),
        "a page move must never come out as an unprotected file, got: {result:?}"
    );
}

#[test]
fn a_full_rewrite_with_both_passwords_reapplies_the_protection() {
    let original = promptless_two_page_pdf();
    let (base, security) =
        pdf_manip::open_document_with_passwords_from_bytes(&original, "", OWNER_PASSWORD).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();
    apply_command(&mut document, Command::MovePage { from: 0, to: 1 });

    let saved = save_document(input(&document, &base, &original)).expect("full rewrite");

    assert_same_protection(&saved);
    let (reloaded, _) = pdf_manip::open_document_from_bytes(&saved, None).unwrap();
    assert_eq!(reloaded.as_lopdf().get_pages().len(), 2);
}

#[test]
fn a_compressed_save_is_refused_rather_than_written_in_plaintext() {
    let original = promptless_two_page_pdf();
    let (base, security) = pdf_manip::open_document_from_bytes(&original, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();

    assert!(pdf_save::compression_blocker(input(&document, &base, &original)).is_some());
}
