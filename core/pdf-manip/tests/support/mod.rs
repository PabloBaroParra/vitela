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
use pdf_manip::LopdfDocument;

pub mod structures;

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

/// A source whose pages carry nothing of their own: `/MediaBox`,
/// `/Resources` and `/Rotate` all live on the page-tree root and reach the
/// pages by inheritance (PDF 32000-1:2008 section 7.7.3.4). Grafting has to
/// materialize them, because the root they inherit from is not coming along.
pub fn pdf_with_inherited_attributes(labels: &[&str]) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F9" => font_id },
    });

    let mut kid_ids = Vec::with_capacity(labels.len());
    for label in labels {
        let content = Content {
            operations: vec![Operation::new("Tj", vec![Object::string_literal(*label)])],
        };
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            content.encode().expect("encode page content"),
        ));
        kid_ids.push(doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        }));
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kid_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
            "Count" => kid_ids.len() as i64,
            "MediaBox" => vec![0.into(), 0.into(), 300.into(), 400.into()],
            "Resources" => resources_id,
            "Rotate" => 90,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

/// A two-page source whose first page carries a link annotation pointing at
/// the *second* page — the reference a graft of page one alone must not
/// follow, or it would drag an unselected page in with it.
pub fn pdf_with_a_link_to_its_second_page() -> Document {
    let mut doc = build_pdf_with_pages(&["Linked", "Target"]);
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    let (first, second) = (pages[0], pages[1]);
    let annotation_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        "Dest" => vec![Object::Reference(second), "Fit".into()],
    });
    doc.get_dictionary_mut(first)
        .expect("first page")
        .set("Annots", vec![Object::Reference(annotation_id)]);
    doc
}

/// A two-page source whose first page carries a `/Subtype /Widget`
/// annotation — the visible half of an AcroForm field. The second page has
/// none, so a graft that selects only it must still succeed.
pub fn pdf_with_a_widget_on_first_page() -> Document {
    let mut doc = build_pdf_with_pages(&["Form", "Plain"]);
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    let first = pages[0];
    let widget_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal("Name"),
        "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
    });
    doc.get_dictionary_mut(first)
        .expect("first page")
        .set("Annots", vec![Object::Reference(widget_id)]);
    doc
}

/// A one-page source whose resources nest several levels deep: the page's
/// `/XObject` names an image, that image names an `/SMask` image and an
/// indexed `/ColorSpace` whose lookup table is itself an indirect object.
///
/// Exists because a graft that only copies a page's immediate references
/// still produces a page that renders blank — the interesting failures all
/// live two or three hops in, inside a *stream's* dictionary rather than a
/// plain one.
pub fn pdf_with_nested_page_resources(label: &str) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let lookup_id = doc.add_object(Object::string_literal("   ÿÿÿ"));
    let colorspace_id = doc.add_object(Object::Array(vec![
        "Indexed".into(),
        "DeviceRGB".into(),
        1.into(),
        Object::Reference(lookup_id),
    ]));
    let smask_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 1,
            "Height" => 1,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
        },
        vec![0xff],
    ));
    let image_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 1,
            "Height" => 1,
            "BitsPerComponent" => 1,
            "ColorSpace" => Object::Reference(colorspace_id),
            "SMask" => Object::Reference(smask_id),
        },
        vec![0x00],
    ));
    let resources_id = doc.add_object(dictionary! {
        "XObject" => dictionary! { "Im1" => image_id },
    });

    let content = Content {
        operations: vec![
            Operation::new("Tj", vec![Object::string_literal(label)]),
            Operation::new("Do", vec!["Im1".into()]),
        ],
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        content.encode().expect("encode page content"),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

pub fn labels(document: &LopdfDocument) -> Vec<String> {
    document
        .as_lopdf()
        .get_pages()
        .values()
        .map(|&id| page_label(document.as_lopdf(), id))
        .collect()
}

/// Every object in `document` that is itself a page, whether or not the page
/// tree still lists it. `get_pages` only walks `/Kids`, so on its own it
/// cannot tell a page that was copied in by accident from one that was not.
pub fn page_object_count(document: &LopdfDocument) -> usize {
    document
        .as_lopdf()
        .objects
        .values()
        .filter(|object| object.type_name().unwrap_or_default() == b"Page")
        .count()
}
