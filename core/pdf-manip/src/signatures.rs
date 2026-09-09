//! What a signature means to an import (checklist "Seguridad y firmas",
//! `docs/batch-pdf-assembly.md` section 5).
//!
//! Two questions, two scopes, and they must not be confused:
//!
//! - **Does this file carry a signature at all?**
//!   [`document_has_signatures`]. A signature's byte range covers the whole
//!   file, so this is a document-level fact — it is what tells a destination
//!   that saving will break something, and what tells a source that its
//!   pages travel without any of their signing.
//! - **Does *this page* carry a signature's widget?**
//!   [`page_has_signature_widget`]. This is the page-level fact a graft acts
//!   on, because a widget in a page's `/Annots` is the only part of a
//!   signature a copied page can drag along.
//!
//! ## Why a signature widget is refused rather than reported
//!
//! [`crate::report`] sorts everything a graft leaves behind into *refused*
//! (the imported page would come out **wrong**) and *reported* (the page is
//! intact, something about it did not travel). A signature widget is the
//! clearest case of the first kind, and worse than the AcroForm widget it is
//! a special case of.
//!
//! The widget carries the appearance stream — "Digitally signed by …", the
//! name, the date, the seal. Its field, its `/V` signature dictionary and the
//! `/ByteRange` that dictionary covers all stay behind in the source. Copy
//! the widget alone and the destination shows a signature block that attests
//! to a document it has never seen, with nothing in the file for a reader to
//! check it against. That is not a poorer import. It is a forgery-shaped one,
//! and it is exactly what this module exists to make impossible.
//!
//! This crate owns the detection for both scopes so `pdf-save` (the
//! destination side, at save time) and [`crate::graft_report`] (the source
//! side, at selection time) can never disagree about what counts as a
//! signature.

use lopdf::{Dictionary, Document as LopdfRawDocument, Object};

use crate::document::LopdfDocument;

/// How far a `/Parent` chain is followed before giving up. A field tree is a
/// handful of levels deep in any real file; the bound is here so a malformed
/// one that points at itself cannot spin.
const MAX_FIELD_DEPTH: usize = 32;

/// Whether the file already contains a signature.
///
/// Looks for the two shapes a signed PDF takes: an `/AcroForm` that declares
/// `/SigFlags`, and any object that is a signature dictionary or a signature
/// form field. Scanning objects rather than only walking `/AcroForm /Fields`
/// is deliberate — a file whose form tree is damaged can still carry a
/// signature, and under-reporting here means a user is not warned before
/// their signature stops verifying.
pub fn document_has_signatures(document: &LopdfDocument) -> bool {
    raw_document_has_signatures(&document.0)
}

pub(crate) fn raw_document_has_signatures(document: &LopdfRawDocument) -> bool {
    if acroform_declares_signatures(document) {
        return true;
    }

    document
        .objects
        .values()
        .any(|object| object.as_dict().ok().is_some_and(is_signature_dict))
}

fn acroform_declares_signatures(document: &LopdfRawDocument) -> bool {
    let Ok(root) = document.trailer.get(b"Root") else {
        return false;
    };
    let Some(catalog) = dereferenced_dict(document, root) else {
        return false;
    };
    let Ok(form) = catalog.get(b"AcroForm") else {
        return false;
    };

    dereferenced_dict(document, form)
        .and_then(|form| form.get(b"SigFlags").ok().and_then(|f| f.as_i64().ok()))
        // Bit 1 of /SigFlags is SignaturesExist.
        .is_some_and(|flags| flags & 1 != 0)
}

fn is_signature_dict(dict: &Dictionary) -> bool {
    name_is(dict, b"Type", b"Sig") || name_is(dict, b"FT", b"Sig")
}

/// Whether `page`'s `/Annots` holds the widget of a **signature** field.
///
/// Narrower than "has a widget" on purpose: an ordinary text field and a
/// signature are both `/Subtype /Widget`, and telling a user their page "has
/// form fields" when what it really has is somebody's signature hides the
/// only fact that mattered. [`crate::report`] asks this one first so the more
/// specific refusal wins.
///
/// A field's `/FT` may sit on the widget itself (the common merged
/// field-and-widget shape) or on an ancestor reached through `/Parent`, so
/// the chain is walked. A `/V` resolving to a `/Type /Sig` dictionary counts
/// too: it is the signature itself hanging off the field, and a producer that
/// omits `/FT` somewhere up the chain must not slip a real signature past.
pub(crate) fn page_has_signature_widget(donor: &LopdfRawDocument, page: &Dictionary) -> bool {
    let Ok(annots) = page.get(b"Annots").and_then(|value| value.as_array()) else {
        return false;
    };
    annots.iter().any(|annot| {
        dereferenced_dict(donor, annot).is_some_and(|dict| {
            name_is(dict, b"Subtype", b"Widget") && field_is_a_signature(donor, dict)
        })
    })
}

fn field_is_a_signature(donor: &LopdfRawDocument, widget: &Dictionary) -> bool {
    let mut field = widget;
    for _ in 0..MAX_FIELD_DEPTH {
        if name_is(field, b"FT", b"Sig") || value_is_a_signature(donor, field) {
            return true;
        }
        let Ok(parent) = field.get(b"Parent") else {
            return false;
        };
        let Some(next) = dereferenced_dict(donor, parent) else {
            return false;
        };
        field = next;
    }
    false
}

fn value_is_a_signature(donor: &LopdfRawDocument, field: &Dictionary) -> bool {
    field
        .get(b"V")
        .ok()
        .and_then(|value| dereferenced_dict(donor, value))
        .is_some_and(|value| name_is(value, b"Type", b"Sig"))
}

fn name_is(dict: &Dictionary, key: &[u8], expected: &[u8]) -> bool {
    dict.get(key)
        .ok()
        .and_then(|value| value.as_name().ok())
        .is_some_and(|name| name == expected)
}

fn dereferenced_dict<'a>(
    document: &'a LopdfRawDocument,
    value: &'a Object,
) -> Option<&'a Dictionary> {
    match value {
        Object::Dictionary(dict) => Some(dict),
        Object::Reference(id) => document.get_dictionary(*id).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    /// A minimal document with a catalog, which is all any check here needs.
    fn document() -> LopdfRawDocument {
        let mut doc = LopdfRawDocument::with_version("1.5");
        let pages_id = doc.new_object_id();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Vec::<Object>::new(),
                "Count" => 0,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn set_acroform(doc: &mut LopdfRawDocument, form: Dictionary) {
        let Ok(Object::Reference(catalog_id)) = doc.trailer.get(b"Root") else {
            panic!("the fixture has a catalog");
        };
        let catalog_id = *catalog_id;
        doc.get_dictionary_mut(catalog_id)
            .expect("catalog")
            .set("AcroForm", form);
    }

    /// A page whose `/Annots` holds `annot`, as an indirect object.
    fn page_with_annotation(doc: &mut LopdfRawDocument, annot: Dictionary) -> Dictionary {
        let annot_id = doc.add_object(annot);
        dictionary! { "Annots" => vec![Object::Reference(annot_id)] }
    }

    #[test]
    fn an_unsigned_document_reports_no_signatures() {
        assert!(!raw_document_has_signatures(&document()));
    }

    #[test]
    fn a_signature_dictionary_is_detected() {
        let mut doc = document();
        doc.add_object(dictionary! { "Type" => "Sig", "Filter" => "Adobe.PPKLite" });

        assert!(raw_document_has_signatures(&doc));
    }

    #[test]
    fn a_signature_form_field_is_detected() {
        let mut doc = document();
        doc.add_object(dictionary! { "FT" => "Sig", "T" => "Signature1" });

        assert!(raw_document_has_signatures(&doc));
    }

    /// A file whose form tree says signatures exist counts even when the
    /// field objects themselves cannot be reached.
    #[test]
    fn an_acroform_declaring_sigflags_is_detected() {
        let mut doc = document();
        set_acroform(&mut doc, dictionary! { "SigFlags" => 3 });

        assert!(raw_document_has_signatures(&doc));
    }

    #[test]
    fn an_acroform_without_the_signatures_exist_bit_is_not_a_signature() {
        let mut doc = document();
        set_acroform(&mut doc, dictionary! { "SigFlags" => 0 });

        assert!(!raw_document_has_signatures(&doc));
    }

    #[test]
    fn the_public_wrapper_answers_for_a_handle() {
        let mut doc = document();
        doc.add_object(dictionary! { "Type" => "Sig" });

        assert!(document_has_signatures(&LopdfDocument::from_lopdf(doc)));
    }

    #[test]
    fn a_merged_signature_widget_is_a_signature_widget() {
        let mut doc = document();
        let page = page_with_annotation(
            &mut doc,
            dictionary! { "Subtype" => "Widget", "FT" => "Sig" },
        );

        assert!(page_has_signature_widget(&doc, &page));
    }

    /// The `/FT` sits on the parent field and the widget is only its
    /// appearance — the shape a field with several widgets takes.
    #[test]
    fn a_signature_reached_through_the_parent_chain_is_found() {
        let mut doc = document();
        let parent_id = doc.add_object(dictionary! { "FT" => "Sig", "T" => "Signature1" });
        let page = page_with_annotation(
            &mut doc,
            dictionary! { "Subtype" => "Widget", "Parent" => Object::Reference(parent_id) },
        );

        assert!(page_has_signature_widget(&doc, &page));
    }

    /// No `/FT` anywhere, but the field's value *is* the signature. Detected
    /// on purpose: missing one here would let a real signature appearance
    /// travel into another file.
    #[test]
    fn a_field_whose_value_is_a_signature_dictionary_is_found() {
        let mut doc = document();
        let sig_id = doc.add_object(dictionary! { "Type" => "Sig", "Filter" => "Adobe.PPKLite" });
        let page = page_with_annotation(
            &mut doc,
            dictionary! { "Subtype" => "Widget", "V" => Object::Reference(sig_id) },
        );

        assert!(page_has_signature_widget(&doc, &page));
    }

    /// An ordinary text field is a widget and is **not** a signature. The two
    /// are refused for different reasons and must stay distinguishable.
    #[test]
    fn a_text_field_widget_is_not_a_signature_widget() {
        let mut doc = document();
        let page = page_with_annotation(
            &mut doc,
            dictionary! { "Subtype" => "Widget", "FT" => "Tx", "T" => "Name" },
        );

        assert!(!page_has_signature_widget(&doc, &page));
    }

    /// A signature annotation that is not a widget is not a field's visible
    /// half, and a page with no `/Annots` has nothing to check.
    #[test]
    fn a_non_widget_annotation_and_a_bare_page_are_not_signature_widgets() {
        let mut doc = document();
        let page =
            page_with_annotation(&mut doc, dictionary! { "Subtype" => "Link", "FT" => "Sig" });

        assert!(!page_has_signature_widget(&doc, &page));
        assert!(!page_has_signature_widget(&doc, &dictionary! {}));
    }

    /// A `/Parent` chain that points at itself must end the walk, not spin.
    #[test]
    fn a_self_referential_parent_chain_terminates() {
        let mut doc = document();
        let field_id = doc.new_object_id();
        doc.objects.insert(
            field_id,
            Object::Dictionary(dictionary! { "Parent" => Object::Reference(field_id) }),
        );
        let page = page_with_annotation(
            &mut doc,
            dictionary! { "Subtype" => "Widget", "Parent" => Object::Reference(field_id) },
        );

        assert!(!page_has_signature_widget(&doc, &page));
    }
}
