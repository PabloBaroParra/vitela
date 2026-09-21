//! Fixtures for Batch 20's AcroForm tests (T-144): a one-page document that
//! already carries an `/AcroForm`, built with lopdf.
//!
//! The companion to `tests/fixtures/forms/reportlab_acroform.pdf`, and
//! deliberately not a replacement for it. That file is committed, produced
//! once by a foreign encoder, and exists to catch the places where this
//! workspace's reader only agrees with this workspace's writer — it already
//! caught one (see `pdf-form`'s `external_acroform.rs`). This builder is the
//! opposite tool: a document whose starting state a test dictates exactly,
//! so a round-trip can assert on what changed without first having to
//! describe what reportlab happened to emit.
//!
//! Follows the `labeled_pdf` pattern `pdf-save::bridge`'s own test fixtures
//! use: raw dictionaries, no builder indirection, everything the test needs
//! to reason about visible in one function.

use lopdf::xref::XrefType;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

/// The `/T` of the text field [`build_acroform_document`] writes.
pub const TEXT_FIELD_NAME: &str = "applicant";
/// The `/T` of the checkbox [`build_acroform_document`] writes.
pub const CHECKBOX_FIELD_NAME: &str = "agrees";
/// The `/AP` `/N` state name the checkbox is on in — the value
/// `pdf-form`'s `CHECKBOX_ON_STATE` also uses.
pub const CHECKBOX_ON_STATE: &str = "Yes";

/// A single-page, unencrypted document with an `/AcroForm` holding one empty
/// single-line text field and one unchecked checkbox, both merged
/// field+widget dictionaries referenced from the page's `/Annots`.
///
/// Both start *unset* on purpose: a fill test that begins from a populated
/// field cannot tell "the value was written" from "the value was already
/// there".
pub fn build_acroform_document() -> Document {
    let mut doc = Document::with_version("1.5");
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));

    let text_id = doc.add_object(text_field_dictionary());
    // The checkbox's two `/AP` `/N` appearance states. Empty streams: what
    // the reader needs from them is the *names* — that is how it recovers
    // the on-state — and what a fill test needs is that they were replaced.
    let on_state = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let off_state = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let checkbox_id = doc.add_object(checkbox_dictionary(on_state, off_state));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Annots" => vec![Object::Reference(text_id), Object::Reference(checkbox_id)],
    });
    for field in [text_id, checkbox_id] {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(field) {
            dict.set("P", page_id);
        }
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );

    let helvetica_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let acroform_id = doc.add_object(dictionary! {
        "Fields" => vec![Object::Reference(text_id), Object::Reference(checkbox_id)],
        "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
        "DR" => dictionary! {
            "Font" => dictionary! { "Helv" => Object::Reference(helvetica_id) },
        },
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => Object::Reference(acroform_id),
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

fn text_field_dictionary() -> Dictionary {
    dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal(TEXT_FIELD_NAME),
        "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
        "Rect" => vec![72.into(), 700.into(), 312.into(), 722.into()],
        "V" => Object::string_literal(""),
        "F" => 4,
    }
}

fn checkbox_dictionary(on_state: ObjectId, off_state: ObjectId) -> Dictionary {
    let mut normal = Dictionary::new();
    normal.set(CHECKBOX_ON_STATE.as_bytes().to_vec(), on_state);
    normal.set("Off", off_state);
    let mut appearance = Dictionary::new();
    appearance.set("N", Object::Dictionary(normal));

    dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Btn",
        "T" => Object::string_literal(CHECKBOX_FIELD_NAME),
        "Rect" => vec![72.into(), 660.into(), 90.into(), 678.into()],
        "V" => Object::Name(b"Off".to_vec()),
        "AS" => Object::Name(b"Off".to_vec()),
        "AP" => Object::Dictionary(appearance),
        "F" => 4,
    }
}
