//! T-144: the read path against an AcroForm this workspace did not write.
//!
//! Every other fixture this crate parses is built with lopdf — the same
//! library `pdf-form`'s writer half uses — so those tests prove the reader
//! agrees with the writer and nothing more.
//! `tests/fixtures/forms/reportlab_acroform.pdf` is committed precisely to
//! break that circle: reportlab laid out the field tree its own way, once,
//! and the file is versioned so the disagreement (if any) is reproducible.
//!
//! It earned its place immediately. reportlab restates the inheritable
//! `/FT` on every radio-button widget, which `kids_are_widgets` used to read
//! as "this kid is a field of its own" — turning one radio group into one
//! nameless checkbox per button, both answering to the parent's `/T`. Every
//! lopdf-built fixture in this crate omits `/FT` on its kids, so the whole
//! suite agreed with itself straight past it. See the fixture's generator
//! script for what else it exercises and why.

use pdf_document::{FieldOrigin, FieldValue, FontFamily, FormFieldKind, PageId};
use pdf_form::read_form_fields;

fn external_fixture() -> lopdf::Document {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("forms")
        .join("reportlab_acroform.pdf");
    lopdf::Document::load(&path).expect("the committed AcroForm fixture must load")
}

#[test]
fn every_modeled_field_is_read_in_acroform_order() {
    let fields = read_form_fields(&external_fixture());

    let names: Vec<_> = fields.iter().map(|field| field.name.as_str()).collect();
    assert_eq!(
        names,
        ["full_name", "notes", "subscribe", "plan", "country"],
        "read order is /Fields order"
    );
    assert!(
        fields.iter().all(
            |field| field.page == PageId(0) && matches!(field.origin, FieldOrigin::Existing(_))
        ),
        "every field resolves to the page its widget sits on, and came from the file"
    );
}

/// T-137's read resilience, against a real file rather than a synthetic one:
/// a field this crate does not model stays in the document and simply never
/// appears in the editable set. The fixture's `languages` listbox is a
/// `/Ch` field *without* the combo flag — the one choice shape that is not a
/// dropdown.
#[test]
fn the_listbox_is_left_unmodeled_without_disturbing_its_neighbours() {
    let document = external_fixture();

    let fields = read_form_fields(&document);

    assert!(
        !fields.iter().any(|field| field.name == "languages"),
        "a listbox is not a field this crate models"
    );
    assert_eq!(fields.len(), 5, "and nothing around it was dropped either");
}

#[test]
fn a_text_field_carries_its_value_style_and_max_len() {
    let fields = read_form_fields(&external_fixture());
    let field = &fields[0];

    assert_eq!(field.value, FieldValue::Text("Ada Lovelace".to_string()));
    assert_eq!(field.style.font, FontFamily::Helvetica);
    assert_eq!(field.style.size_pt, 12.0);
    // reportlab writes `/MaxLen 100` on a text field by default. Read back
    // rather than ignored, because `ops::set_value` enforces it.
    assert_eq!(
        field.kind,
        FormFieldKind::Text {
            multiline: false,
            max_len: Some(100)
        }
    );
}

/// `/Ff` bit 13, set by someone else's encoder. reportlab writes an explicit
/// `/Ff` on every field, so this is the only fixture where the multiline bit
/// does not come from this workspace's own writer.
#[test]
fn the_multiline_flag_survives_a_foreign_encoder() {
    let fields = read_form_fields(&external_fixture());
    let field = &fields[1];

    assert_eq!(field.value, FieldValue::Text(String::new()));
    assert!(matches!(
        field.kind,
        FormFieldKind::Text {
            multiline: true,
            ..
        }
    ));
}

#[test]
fn a_checked_checkbox_reads_as_checked() {
    let fields = read_form_fields(&external_fixture());
    let field = &fields[2];

    assert_eq!(field.kind, FormFieldKind::Checkbox);
    assert_eq!(field.value, FieldValue::Checked(true));
}

/// The regression this fixture was worth having for. One group, two options
/// in `/Kids` order, the selected one named by the parent's `/V`, and a rect
/// spanning both buttons rather than either one of them.
#[test]
fn the_radio_group_is_one_field_with_both_of_its_buttons() {
    let fields = read_form_fields(&external_fixture());
    let field = &fields[3];

    assert_eq!(field.value, FieldValue::Choice(Some("pro".to_string())));
    let FormFieldKind::RadioGroup { options } = &field.kind else {
        panic!("expected a RadioGroup, got {:?}", field.kind);
    };
    let exports: Vec<_> = options
        .iter()
        .map(|option| option.export_value.as_str())
        .collect();
    assert_eq!(exports, ["basic", "pro"]);
    assert_eq!(field.rect.x, 72.0);
    assert_eq!(
        field.rect.width, 78.0,
        "the group's rect is the bounding box of both buttons (72..150)"
    );
}

#[test]
fn a_dropdown_reads_its_options_and_its_current_selection() {
    let fields = read_form_fields(&external_fixture());
    let field = &fields[4];

    assert_eq!(field.value, FieldValue::Choice(Some("Uruguay".to_string())));
    assert_eq!(
        field.kind,
        FormFieldKind::Dropdown {
            options: vec![
                "Argentina".to_string(),
                "Uruguay".to_string(),
                "Chile".to_string()
            ],
            editable: false,
        }
    );
    // Not the default: `parse_da` really read reportlab's `/DA`, rather than
    // falling back when it could not.
    assert_eq!(field.style.font, FontFamily::Courier);
}
