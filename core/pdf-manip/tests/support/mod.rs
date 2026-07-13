//! Shared test helpers for `pdf-manip` integration tests: build small,
//! self-contained, unencrypted lopdf documents with distinguishable page
//! content, without depending on any other workspace crate's fixture
//! generator (keeps this crate's test suite fully self-contained — the
//! `gen-fixtures` crate is reserved for the encrypted corpus consumed by
//! `tests/encrypted_open.rs`, T-025/T-026).

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

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
