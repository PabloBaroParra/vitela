//! Fixtures for AcroForm fields — the widget on the page, the field behind
//! it, and the `/AcroForm` in the catalog that owns both (checklist
//! "Estructuras de documento", `docs/batch-pdf-assembly.md` section 4).
//!
//! Kept apart from [`super`]'s page-level builders because a form field is
//! the one structure that lives in three places at once: the widget hangs off
//! the page's `/Annots`, the field it belongs to hangs off the catalog's
//! `/AcroForm /Fields`, and what draws it comes from the widget's `/AP` plus
//! the form's `/DR`. A fixture that only builds one of the three cannot catch
//! a merge that drops the other two.

use super::build_pdf_with_pages;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

/// The default appearance string every fixture here uses: a font *resource
/// name*, resolved against `/DR`, not a font object. That indirection is the
/// whole reason `/DR` has to be merged — the name travels with the field and
/// means nothing without the entry it points at.
pub const DEFAULT_APPEARANCE: &str = "/Helv 0 Tf 0 g";

fn font(doc: &mut Document, base_font: &str) -> ObjectId {
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => base_font,
    })
}

/// A form XObject standing in for a widget's normal appearance stream.
fn appearance(doc: &mut Document, text: &str) -> ObjectId {
    let drawn = Content {
        operations: vec![Operation::new("Tj", vec![Object::string_literal(text)])],
    };
    doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 20.into()],
        },
        drawn.encode().expect("encode widget appearance"),
    ))
}

fn set_acroform(doc: &mut Document, form: Dictionary) {
    let catalog = doc
        .trailer
        .get(b"Root")
        .and_then(|root| root.as_reference())
        .expect("the fixture has a catalog");
    doc.get_dictionary_mut(catalog)
        .expect("the fixture has a catalog")
        .set("AcroForm", form);
}

fn attach(doc: &mut Document, page: ObjectId, annotation: ObjectId) {
    doc.get_dictionary_mut(page)
        .expect("the fixture page exists")
        .set("Annots", vec![Object::Reference(annotation)]);
}

fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().into_values().collect()
}

/// The `/AcroForm` every fixture here is built on: it lists `field`, sets the
/// shared `/DA`, and binds the `/Helv` that `/DA` names to `base_font` — the
/// one knob a fixture changes, so that a source and a destination can disagree
/// about what `/Helv` means.
fn form_over(doc: &mut Document, field: ObjectId, base_font: &str) -> Dictionary {
    let font_id = font(doc, base_font);
    dictionary! {
        "Fields" => vec![Object::Reference(field)],
        "DA" => Object::string_literal(DEFAULT_APPEARANCE),
        "DR" => dictionary! { "Font" => dictionary! { "Helv" => font_id } },
    }
}

/// A two-page source whose first page carries a complete text field: the
/// widget is merged with its field (the common single-widget shape), it has a
/// value, a default appearance naming `/Helv`, and a real `/AP` stream. The
/// second page carries nothing, so a graft that selects only it must still
/// come out with no form at all.
pub fn pdf_with_a_form_field_on_first_page(name: &str, value: &str) -> Document {
    let mut doc = build_pdf_with_pages(&["Form", "Plain"]);
    let first = page_ids(&doc)[0];
    let normal = appearance(&mut doc, value);
    let widget = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal(name),
        "V" => Object::string_literal(value),
        "DA" => Object::string_literal(DEFAULT_APPEARANCE),
        "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
        "P" => first,
        "AP" => dictionary! { "N" => normal },
    });
    attach(&mut doc, first, widget);

    let form = form_over(&mut doc, widget, "Helvetica");
    set_acroform(&mut doc, form);
    doc
}

/// A two-page source with **one** field whose `/Kids` are two widgets, one on
/// each page. Importing a single page must bring the field along with only
/// the widget that page actually shows — the other one would be a kid with no
/// page to live on, sharing the field's value from nowhere.
pub fn pdf_with_one_field_over_two_pages(name: &str) -> Document {
    let mut doc = build_pdf_with_pages(&["First", "Second"]);
    let pages = page_ids(&doc);
    let field = doc.new_object_id();

    let mut kids = Vec::with_capacity(pages.len());
    for (index, &page) in pages.iter().enumerate() {
        let normal = appearance(&mut doc, &format!("kid {index}"));
        let widget = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Parent" => field,
            "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
            "P" => page,
            "AP" => dictionary! { "N" => normal },
        });
        attach(&mut doc, page, widget);
        kids.push(Object::Reference(widget));
    }

    doc.objects.insert(
        field,
        Object::Dictionary(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal(name),
            "V" => Object::string_literal("shared"),
            "DA" => Object::string_literal(DEFAULT_APPEARANCE),
            "Kids" => kids,
        }),
    );
    let form = form_over(&mut doc, field, "Helvetica");
    set_acroform(&mut doc, form);
    doc
}

/// A one-page source whose field sets neither `/DA` nor `/Q`: both are
/// inherited from the `/AcroForm`, which is exactly the dictionary a graft
/// leaves behind. The same problem the page tree has with `/Resources`, one
/// level up.
pub fn pdf_with_a_field_inheriting_the_form_defaults(name: &str) -> Document {
    let mut doc = build_pdf_with_pages(&["Form"]);
    let first = page_ids(&doc)[0];
    let normal = appearance(&mut doc, "inherited");
    let widget = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal(name),
        "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
        "P" => first,
        "AP" => dictionary! { "N" => normal },
    });
    attach(&mut doc, first, widget);

    let mut form = form_over(&mut doc, widget, "Helvetica");
    form.set("Q", 1);
    set_acroform(&mut doc, form);
    doc
}

/// A one-page source whose widget has no `/AP` at all: the source got away
/// with it because its own `/AcroForm` sets `/NeedAppearances`, telling the
/// viewer to build one. That instruction lives in the catalog and does not
/// travel, so an import that only copies the widget renders an empty box.
pub fn pdf_with_a_widget_that_needs_its_appearance_built(name: &str) -> Document {
    let mut doc = build_pdf_with_pages(&["Form"]);
    let first = page_ids(&doc)[0];
    let widget = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal(name),
        "V" => Object::string_literal("unpainted"),
        "DA" => Object::string_literal(DEFAULT_APPEARANCE),
        "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
        "P" => first,
    });
    attach(&mut doc, first, widget);

    let mut form = form_over(&mut doc, widget, "Helvetica");
    form.set("NeedAppearances", true);
    set_acroform(&mut doc, form);
    doc
}

/// A one-page source whose form is an **XFA** form: the `/AcroForm` carries
/// an `/XFA` entry, so the AcroForm fields under it are only the fallback
/// shell of a form whose real definition is the XML in the catalog.
pub fn pdf_with_an_xfa_form(name: &str) -> Document {
    let mut doc = pdf_with_a_form_field_on_first_page(name, "shell");
    let xfa = doc.add_object(Stream::new(
        dictionary! {},
        b"<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\"></xdp:xdp>".to_vec(),
    ));
    let catalog = doc
        .trailer
        .get(b"Root")
        .and_then(|root| root.as_reference())
        .expect("the fixture has a catalog");
    let mut form = doc
        .get_dictionary(catalog)
        .expect("the fixture has a catalog")
        .get(b"AcroForm")
        .and_then(|form| form.as_dict())
        .expect("the fixture has an /AcroForm")
        .clone();
    form.set("XFA", Object::Reference(xfa));
    doc.get_dictionary_mut(catalog)
        .expect("the fixture has a catalog")
        .set("AcroForm", form);
    doc
}

/// A **destination** that already has a form of its own: one field, and a
/// `/DR` that binds the very same resource name `/Helv` to a different font.
/// Both halves matter — the field name is what an imported field can collide
/// with, and the resource name is what its `/DA` would silently start
/// resolving to.
pub fn destination_with_a_form_field(name: &str, value: &str) -> Document {
    let mut doc = build_pdf_with_pages(&["D1"]);
    let first = page_ids(&doc)[0];
    let normal = appearance(&mut doc, value);
    let widget = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal(name),
        "V" => Object::string_literal(value),
        "DA" => Object::string_literal(DEFAULT_APPEARANCE),
        "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
        "P" => first,
        "AP" => dictionary! { "N" => normal },
    });
    attach(&mut doc, first, widget);

    let form = form_over(&mut doc, widget, "Courier");
    set_acroform(&mut doc, form);
    doc
}
