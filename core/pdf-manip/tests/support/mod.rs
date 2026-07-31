//! Shared test helpers for `pdf-manip` integration tests: build small,
//! self-contained, unencrypted lopdf documents with distinguishable page
//! content, without depending on any other workspace crate's fixture
//! generator (keeps this crate's test suite fully self-contained — the
//! `gen-fixtures` crate is reserved for the encrypted corpus consumed by
//! `tests/encrypted_open.rs`, T-025/T-026).

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::content::{Content, Operation};
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::xref::XrefType;
use lopdf::{
    dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
};

/// Builds a minimal, valid, unencrypted PDF with one page per label in
/// `labels`, each page's content stream containing a single `Tj` operation
/// with that label's text, so tests can verify page identity/order after a
/// manipulation op via [`page_label`].
pub fn build_pdf_with_pages(labels: &[&str]) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut kid_ids = Vec::with_capacity(labels.len());
    for label in labels {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 24.into()]),
                Operation::new("Td", vec![100.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal(*label)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            content.encode().expect("encode test page content"),
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        kid_ids.push(page_id);
    }

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => kid_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
        "Count" => kid_ids.len() as i64,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    doc
}

/// Builds an AES-128 encrypted single-page PDF whose `/P` is exactly
/// `permissions` — the shape of document a permission gate must refuse.
///
/// `user_password` is what the reader has to supply to open it; passing `""`
/// produces the very common "opens with no prompt at all, yet still restricts"
/// document, which is the case every permission gate gets wrong first.
pub fn restricted_pdf(
    user_password: &str,
    owner_password: &str,
    permissions: Permissions,
) -> Vec<u8> {
    let mut doc = build_pdf_with_pages(&["restricted"]);
    // Classic xref table, matching `gen-fixtures`: lopdf cannot re-hydrate
    // objects out of an encrypted cross-reference stream at load time.
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    // The standard security handler derives its key from the first element of
    // the trailer's /ID array (PDF 32000-1:2008 §7.6.3.3); without it lopdf
    // refuses to build the encryption state at all.
    let file_id = Object::string_literal("restricted-fixture-id");
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
        permissions,
    };
    let state = EncryptionState::try_from(version).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

/// Reads a page's decoded content-stream label back out, for asserting page
/// identity/order after a manipulation op. Assumes the page was built by
/// [`build_pdf_with_pages`] (single `Tj` operation with a literal string).
pub fn page_label(doc: &Document, page_id: lopdf::ObjectId) -> String {
    let content = doc
        .get_and_decode_page_content(page_id)
        .expect("decode test page content");
    for op in &content.operations {
        if op.operator == "Tj" {
            if let Some(Object::String(bytes, _)) = op.operands.first() {
                return String::from_utf8_lossy(bytes).to_string();
            }
        }
    }
    panic!("no Tj operation found in page {page_id:?} content");
}
