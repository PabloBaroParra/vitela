//! Fixtures for the document structures that live in a source's *catalog* —
//! destination name trees, outlines, optional content and tagged structure.
//!
//! Kept apart from [`super`]'s page-level builders because they are answering
//! a different question. Those build a page and ask "did its content arrive?";
//! these build a catalog and ask "what did leaving the catalog behind cost?"
//! (checklist "Estructuras de documento", `docs/batch-pdf-assembly.md`
//! section 4).

use lopdf::{dictionary, Document, Object};

use super::build_pdf_with_pages;

/// A two-page source whose first page links to the second through a **named**
/// destination held in a `/Names /Dests` name tree — and the tree is given a
/// `/Kids` level, because a name tree that is one flat leaf never exercises
/// the branch walk a real one needs.
pub fn pdf_with_a_named_destination_to_its_second_page() -> Document {
    let mut doc = build_pdf_with_pages(&["Linked", "Target"]);
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    let (first, second) = (pages[0], pages[1]);

    let leaf_id = doc.add_object(dictionary! {
        "Limits" => vec![
            Object::string_literal("chapter2"),
            Object::string_literal("chapter2"),
        ],
        "Names" => vec![
            Object::string_literal("chapter2"),
            Object::Array(vec![
                Object::Reference(second),
                "XYZ".into(),
                Object::Null,
                700.into(),
                Object::Null,
            ]),
        ],
    });
    let dests_root_id = doc.add_object(dictionary! {
        "Kids" => vec![Object::Reference(leaf_id)],
    });
    let names_id = doc.add_object(dictionary! {
        "Dests" => Object::Reference(dests_root_id),
    });
    doc.catalog_mut()
        .expect("catalog")
        .set("Names", Object::Reference(names_id));

    let action_id = doc.add_object(dictionary! {
        "S" => "GoTo",
        "D" => Object::string_literal("chapter2"),
    });
    let annotation_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        "A" => Object::Reference(action_id),
    });
    doc.get_dictionary_mut(first)
        .expect("first page")
        .set("Annots", vec![Object::Reference(annotation_id)]);
    doc
}

/// The same link written the PDF 1.1 way: a `/Dest` naming a key in the
/// catalog's own `/Dests` dictionary, with no name tree anywhere.
pub fn pdf_with_a_legacy_named_destination() -> Document {
    let mut doc = build_pdf_with_pages(&["Linked", "Target"]);
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    let (first, second) = (pages[0], pages[1]);

    let dests_id = doc.add_object(dictionary! {
        "chapter2" => Object::Array(vec![Object::Reference(second), "Fit".into()]),
    });
    doc.catalog_mut()
        .expect("catalog")
        .set("Dests", Object::Reference(dests_id));

    let annotation_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        "Dest" => "chapter2",
    });
    doc.get_dictionary_mut(first)
        .expect("first page")
        .set("Annots", vec![Object::Reference(annotation_id)]);
    doc
}

/// A one-page source whose only link names a destination the document never
/// defines — broken before the import, and still broken after it.
pub fn pdf_with_an_undefined_named_destination() -> Document {
    let mut doc = build_pdf_with_pages(&["Linked"]);
    let first = *doc.get_pages().values().next().expect("one page");
    let annotation_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        "Dest" => Object::string_literal("ghost"),
    });
    doc.get_dictionary_mut(first)
        .expect("first page")
        .set("Annots", vec![Object::Reference(annotation_id)]);
    doc
}

/// A two-page source with an outline whose two entries point at page one and
/// page two — so a graft of page one alone must report exactly one bookmark
/// left behind, not two.
pub fn pdf_with_an_outline_over_both_pages() -> Document {
    let mut doc = build_pdf_with_pages(&["First", "Second"]);
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    let (first, second) = (pages[0], pages[1]);

    let outlines_id = doc.new_object_id();
    let second_item_id = doc.new_object_id();
    let first_item_id = doc.add_object(dictionary! {
        "Title" => Object::string_literal("Chapter 1"),
        "Parent" => outlines_id,
        "Next" => second_item_id,
        "Dest" => Object::Array(vec![Object::Reference(first), "Fit".into()]),
    });
    doc.objects.insert(
        second_item_id,
        Object::Dictionary(dictionary! {
            "Title" => Object::string_literal("Chapter 2"),
            "Parent" => outlines_id,
            "Prev" => first_item_id,
            "Dest" => Object::Array(vec![Object::Reference(second), "Fit".into()]),
        }),
    );
    doc.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! {
            "Type" => "Outlines",
            "First" => first_item_id,
            "Last" => second_item_id,
            "Count" => 2,
        }),
    );
    doc.catalog_mut()
        .expect("catalog")
        .set("Outlines", Object::Reference(outlines_id));
    doc
}

/// A one-page source whose content is inside an optional-content group whose
/// configuration lives in the catalog's `/OCProperties` — the layer state an
/// import cannot carry, and without which hidden content comes back visible.
pub fn pdf_with_optional_content() -> Document {
    let mut doc = build_pdf_with_pages(&["Layered"]);
    let first = *doc.get_pages().values().next().expect("one page");

    let ocg_id = doc.add_object(dictionary! {
        "Type" => "OCG",
        "Name" => Object::string_literal("Watermark"),
    });
    doc.catalog_mut().expect("catalog").set(
        "OCProperties",
        dictionary! {
            "OCGs" => vec![Object::Reference(ocg_id)],
            // The layer is *off* by default: the whole point of refusing this
            // import is that a destination with no `/OCProperties` has nothing
            // that says so.
            "D" => dictionary! {
                "OFF" => vec![Object::Reference(ocg_id)],
            },
        },
    );

    let page = doc.get_dictionary_mut(first).expect("first page");
    let resources = dictionary! {
        "Properties" => dictionary! { "MC0" => Object::Reference(ocg_id) },
    };
    page.set("Resources", resources);
    doc
}

/// A two-page tagged source: every page keys into a `/StructTreeRoot` that
/// stays behind with the catalog.
pub fn pdf_with_tagged_pages() -> Document {
    let mut doc = build_pdf_with_pages(&["First", "Second"]);
    let struct_root_id = doc.add_object(dictionary! {
        "Type" => "StructTreeRoot",
    });
    doc.catalog_mut()
        .expect("catalog")
        .set("StructTreeRoot", Object::Reference(struct_root_id));

    let pages: Vec<_> = doc.get_pages().into_values().collect();
    for (index, page_id) in pages.into_iter().enumerate() {
        doc.get_dictionary_mut(page_id)
            .expect("page")
            .set("StructParents", index as i64);
    }
    doc
}
