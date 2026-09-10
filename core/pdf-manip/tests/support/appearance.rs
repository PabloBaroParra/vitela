//! Fixtures for annotation *appearance* — the `/AP` streams that say how an
//! annotation is actually drawn.
//!
//! Kept apart from [`super`]'s page-level builders for the same reason
//! [`super::structures`] is: those ask "did the page's content arrive?", this
//! asks "did what draws on top of it arrive?" (checklist "Pruebas del
//! núcleo", `docs/batch-pdf-assembly.md` section 12).

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

use super::build_pdf_with_pages;

/// A one-page source whose page carries a `/Square` annotation with a real
/// appearance stream: `/AP` names a normal (`/N`) state and a rollover
/// (`/R`) one, and the normal state's own `/Resources` name a font.
///
/// Exists because copying the annotation *dictionary* is not the same as
/// copying what draws it. An annotation whose `/AP` was left behind is not a
/// missing pixel: PDF 32000-1:2008 section 12.5.5 lets a viewer synthesize an
/// appearance when there is none, so the page still renders — differently, in
/// every viewer, and never again as the author drew it. The font one hop
/// inside the `/N` stream is the part a shallow copy loses first.
pub fn pdf_with_an_annotation_with_an_appearance_stream() -> Document {
    let mut doc = build_pdf_with_pages(&["Annotated"]);
    let page = *doc
        .get_pages()
        .values()
        .next()
        .expect("the fixture has one page");

    let appearance_font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Times-Roman",
    });
    let appearance_resources = doc.add_object(dictionary! {
        "Font" => dictionary! { "ApF" => appearance_font },
    });
    let normal = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["ApF".into(), 12.into()]),
            Operation::new("Tj", vec![Object::string_literal("normal")]),
            Operation::new("ET", vec![]),
        ],
    };
    let normal_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => appearance_resources,
        },
        normal.encode().expect("encode normal appearance"),
    ));
    let rollover = Content {
        operations: vec![Operation::new("Tj", vec![Object::string_literal("over")])],
    };
    let rollover_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        },
        rollover.encode().expect("encode rollover appearance"),
    ));

    let annotation_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        "AP" => dictionary! {
            "N" => Object::Reference(normal_id),
            "R" => Object::Reference(rollover_id),
        },
    });
    doc.get_dictionary_mut(page)
        .expect("page dict")
        .set("Annots", vec![Object::Reference(annotation_id)]);
    doc
}
