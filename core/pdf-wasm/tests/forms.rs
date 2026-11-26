//! The `Engine`'s forms surface against the real pdfium, in a headless
//! browser (B26 T-242). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{EditCommand, Engine, FormFieldEntry};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

fn field(fields: &[FormFieldEntry], name: &str) -> FormFieldEntry {
    fields
        .iter()
        .find(|field| field.name() == name)
        .unwrap_or_else(|| panic!("no field {name}"))
        .clone()
}

fn id_of(engine: &Engine, doc: u32, name: &str) -> u32 {
    field(&engine.form_fields(doc).unwrap(), name).id()
}

fn open_form(engine: &mut Engine) -> u32 {
    engine.open(pdfs::ACROFORM.to_vec(), None).unwrap()
}

/// The page's pixels inside `rect` (`x, y, width, height` in PDF space,
/// rendered at 72 dpi so a point is a pixel), row by row.
fn pixels_in(engine: &mut Engine, doc: u32, rect: &[f64]) -> Vec<u8> {
    let mut page = engine.render_page(doc, 0, 72, false).unwrap();
    let width = page.width() as usize;
    let height = page.height() as usize;
    let rgba = page.take_rgba();
    let left = rect[0] as usize;
    let top = height - (rect[1] + rect[3]) as usize;
    let mut out = Vec::new();
    for row in top..top + rect[3] as usize {
        let start = (row * width + left) * 4;
        out.extend_from_slice(&rgba[start..start + rect[2] as usize * 4]);
    }
    out
}

#[wasm_bindgen_test]
async fn the_four_kinds_fill_save_and_reopen_with_their_values() {
    let mut engine = engine().await;
    let doc = open_form(&mut engine);
    assert!(engine.form_fill_allowed(doc).unwrap());

    let listed = engine.form_fields(doc).unwrap();
    assert_eq!(listed.len(), 5, "the listbox is not offered");
    let full_name = field(&listed, "full_name");
    assert_eq!(
        (full_name.kind(), full_name.text(), full_name.existing()),
        ("text".to_string(), "Ada Lovelace".to_string(), true)
    );
    assert_eq!(full_name.rect(), [72.0, 680.0, 240.0, 22.0]);
    let plan = field(&listed, "plan");
    assert_eq!(plan.kind(), "radio");
    assert_eq!(plan.options(), ["basic", "pro"]);
    assert_eq!(plan.option_rects().len(), 8);
    assert_eq!(plan.selected().as_deref(), Some("pro"));
    let country = field(&listed, "country");
    assert_eq!(country.kind(), "dropdown");
    assert_eq!(country.options(), ["Argentina", "Uruguay", "Chile"]);
    assert!(!country.editable());
    assert_eq!(field(&listed, "subscribe").kind(), "checkbox");
    assert!(field(&listed, "subscribe").checked());

    for edit in [
        EditCommand::set_field_text(full_name.id(), "José Peña".into()),
        EditCommand::set_field_checked(id_of(&engine, doc, "subscribe"), false),
        EditCommand::set_field_choice(plan.id(), Some("basic".into())),
        EditCommand::set_field_choice(country.id(), Some("Chile".into())),
    ] {
        assert_eq!(engine.apply_edit(doc, &edit).unwrap(), None);
    }
    assert!(engine.can_undo(doc).unwrap());

    let saved = engine.save_to_bytes(doc, false).unwrap();
    let reopened = engine.open(saved, None).unwrap();
    let fields = engine.form_fields(reopened).unwrap();
    assert_eq!(field(&fields, "full_name").text(), "José Peña");
    assert!(!field(&fields, "subscribe").checked());
    assert_eq!(field(&fields, "plan").selected().as_deref(), Some("basic"));
    assert_eq!(
        field(&fields, "country").selected().as_deref(),
        Some("Chile")
    );
}

#[wasm_bindgen_test]
async fn a_fill_stales_the_preview_and_the_refresh_paints_the_new_value() {
    let mut engine = engine().await;
    let doc = open_form(&mut engine);
    let full_name = field(&engine.form_fields(doc).unwrap(), "full_name");
    let before = pixels_in(&mut engine, doc, &full_name.rect());

    engine
        .apply_edit(
            doc,
            &EditCommand::set_field_text(full_name.id(), "Grace Hopper".into()),
        )
        .unwrap();
    assert!(
        engine.preview_stale(doc).unwrap(),
        "pdfium paints the field: the pixels need a refresh"
    );
    assert_eq!(
        pixels_in(&mut engine, doc, &full_name.rect()),
        before,
        "nothing changes until the refresh"
    );
    engine.refresh_preview(doc).unwrap();
    assert!(!engine.preview_stale(doc).unwrap());
    let filled = pixels_in(&mut engine, doc, &full_name.rect());
    assert_ne!(filled, before, "the field shows its new value");

    assert!(engine.undo(doc).unwrap());
    assert!(engine.preview_stale(doc).unwrap());
    engine.refresh_preview(doc).unwrap();
    assert_eq!(
        field(&engine.form_fields(doc).unwrap(), "full_name").text(),
        "Ada Lovelace"
    );
    assert_ne!(pixels_in(&mut engine, doc, &full_name.rect()), filled);
}

#[wasm_bindgen_test]
async fn an_impossible_value_and_an_unknown_field_are_named() {
    let mut engine = engine().await;
    let doc = open_form(&mut engine);
    let country = id_of(&engine, doc, "country");
    let subscribe = id_of(&engine, doc, "subscribe");
    for (edit, name) in [
        (
            EditCommand::set_field_choice(country, Some("Atlantis".into())),
            "UnsupportedOperation",
        ),
        (
            EditCommand::set_field_text(subscribe, "yes".into()),
            "UnsupportedOperation",
        ),
        (
            EditCommand::set_field_checked(999, true),
            "FormFieldNotFound",
        ),
    ] {
        let err = engine.apply_edit(doc, &edit).unwrap_err();
        assert_eq!(error_name(err), name);
    }
    assert!(!engine.can_undo(doc).unwrap());
    assert!(!engine.preview_stale(doc).unwrap());
}

#[wasm_bindgen_test]
async fn without_the_fill_permission_a_fill_is_edit_not_permitted() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::restricted_form(), Some(pdfs::USER_PASSWORD.into()))
        .unwrap();
    assert!(!engine.form_fill_allowed(doc).unwrap());
    // Reading needs no permission.
    let full_name = id_of(&engine, doc, "full_name");
    for edit in [
        EditCommand::set_field_text(full_name, "Grace".into()),
        // The permission is asked before the id.
        EditCommand::set_field_text(999, "Grace".into()),
    ] {
        let err = engine.apply_edit(doc, &edit).unwrap_err();
        assert_eq!(error_name(err), "EditNotPermitted");
    }
    assert!(!engine.can_undo(doc).unwrap());
}
