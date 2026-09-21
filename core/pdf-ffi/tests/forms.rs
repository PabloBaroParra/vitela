//! Batch 20, T-140: AcroForm fields seen from a shell.
//!
//! `pdf-form` proves the builders and the value rules, `pdf-save` proves the
//! write; what is tested here is the only thing that exists once they reach
//! the boundary — that a shell can place a field, fill it in, list what the
//! document holds, and have all of it survive a save without linking either
//! crate. Every assertion is made through `pdf-ffi`'s own exported
//! functions, the same way a generated Swift/C#/Kotlin binding would reach
//! them.
//!
//! Its own file rather than more of `smoke.rs`, for the same reason
//! `compress.rs` is: what crosses here is a second crate's vocabulary, not
//! another way to edit a page.

use std::collections::BTreeSet;
use std::sync::Arc;

use pdf_ffi::{
    apply_edit, create_document_with_blank_page, open_from_bytes, redo, save_to_bytes, undo,
    DocumentHandle, FfiColor, FfiEditCommand, FfiError, FfiFieldOrigin, FfiFieldValue,
    FfiFontFamily, FfiFormFieldKind, FfiOrientation, FfiPageSize, FfiRadioOption, FfiRect,
    FfiSaveIntent, FfiSignatureAcknowledgement, FfiTextStyle,
};

fn a_text_style() -> FfiTextStyle {
    FfiTextStyle {
        font: FfiFontFamily::Helvetica,
        size_pt: 12.0,
        color: FfiColor { r: 0, g: 0, b: 0 },
    }
}

fn a_field_rect() -> FfiRect {
    FfiRect {
        x: 72.0,
        y: 600.0,
        width: 180.0,
        height: 24.0,
    }
}

fn a_blank_page() -> Arc<DocumentHandle> {
    create_document_with_blank_page(FfiPageSize::A4, FfiOrientation::Portrait)
        .expect("a one-page document should be creatable")
}

fn place_a_text_field(handle: &DocumentHandle) -> u64 {
    apply_edit(
        handle,
        FfiEditCommand::AddTextField {
            page: 0,
            rect: a_field_rect(),
            style: a_text_style(),
            multiline: false,
            max_len: None,
        },
    )
    .expect("placing a text field should succeed");

    handle
        .list_form_fields()
        .last()
        .expect("the field should be listed")
        .id
}

fn saved(handle: &DocumentHandle) -> Vec<u8> {
    save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save should succeed")
}

/// T-140's own acceptance criterion, end to end and without mocks: create a
/// field, fill it in, save, reopen, and read it back through the panel API —
/// `pdf-form`'s writer and `pdf-form`'s reader against each other, with this
/// boundary in between.
#[test]
fn a_created_field_survives_a_save_and_reopen_with_its_value() {
    let handle = a_blank_page();
    let id = place_a_text_field(&handle);

    apply_edit(
        &handle,
        FfiEditCommand::SetFieldValue {
            field_id: id,
            value: FfiFieldValue::Text {
                text: "Ada Lovelace".to_string(),
            },
        },
    )
    .expect("filling the field should succeed");

    let reopened = open_from_bytes(saved(&handle), None).expect("saved bytes should reopen");
    let fields = reopened.list_form_fields();

    assert_eq!(fields.len(), 1);
    let field = &fields[0];
    assert_eq!(field.page, 0);
    assert_eq!(
        field.value,
        FfiFieldValue::Text {
            text: "Ada Lovelace".to_string()
        }
    );
    assert!(matches!(
        field.kind,
        FfiFormFieldKind::Text {
            multiline: false,
            max_len: None
        }
    ));
    // Read back out of the file it was written to, so it is no longer this
    // session's own creation — the distinction a shell needs to decide
    // whether pdfium is already painting the value or its overlay must.
    assert_eq!(field.origin, FfiFieldOrigin::Existing);
}

#[test]
fn a_newly_placed_field_reports_itself_as_this_sessions_own() {
    let handle = a_blank_page();
    place_a_text_field(&handle);

    let fields = handle.list_form_fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].origin, FfiFieldOrigin::New);
    assert!(!fields[0].name.is_empty());
}

#[test]
fn each_field_kind_places_a_field_the_panel_can_read_back() {
    let handle = a_blank_page();

    for command in [
        FfiEditCommand::AddTextField {
            page: 0,
            rect: a_field_rect(),
            style: a_text_style(),
            multiline: true,
            max_len: Some(40),
        },
        FfiEditCommand::AddCheckbox {
            page: 0,
            rect: a_field_rect(),
            style: a_text_style(),
        },
        FfiEditCommand::AddRadioGroup {
            page: 0,
            rect: a_field_rect(),
            style: a_text_style(),
            options: vec![
                FfiRadioOption {
                    export_value: "yes".to_string(),
                    rect: a_field_rect(),
                },
                FfiRadioOption {
                    export_value: "no".to_string(),
                    rect: a_field_rect(),
                },
            ],
        },
        FfiEditCommand::AddDropdown {
            page: 0,
            rect: a_field_rect(),
            style: a_text_style(),
            options: vec!["one".to_string(), "two".to_string()],
            editable: false,
        },
    ] {
        apply_edit(&handle, command.clone()).unwrap_or_else(|e| panic!("{command:?}: {e}"));
    }

    let fields = handle.list_form_fields();

    assert_eq!(fields.len(), 4);
    assert!(matches!(
        fields[0].kind,
        FfiFormFieldKind::Text {
            multiline: true,
            max_len: Some(40)
        }
    ));
    assert!(matches!(fields[1].kind, FfiFormFieldKind::Checkbox));
    assert!(
        matches!(&fields[2].kind, FfiFormFieldKind::RadioGroup { options } if options.len() == 2)
    );
    assert!(matches!(
        &fields[3].kind,
        FfiFormFieldKind::Dropdown { options, editable: false } if options.len() == 2
    ));
    // Four fields placed and the caller never named one. No two fields may
    // share a `/T`, so the names come from `FormFieldSet::unique_name`
    // rather than from the shell — the same names the GTK shell produces.
    let names: BTreeSet<_> = fields.iter().map(|field| field.name.clone()).collect();
    assert_eq!(names.len(), 4);
}

/// The reason `next_form_field_id` cannot start at zero the way
/// `next_annotation_id` does: `document_from_lopdf` populates the field set
/// at open, so an opened document already holds ids this boundary never
/// issued.
#[test]
fn ids_allocated_for_new_fields_never_collide_with_the_files_own() {
    let handle = a_blank_page();
    place_a_text_field(&handle);

    let reopened = open_from_bytes(saved(&handle), None).expect("saved bytes should reopen");
    apply_edit(
        &reopened,
        FfiEditCommand::AddCheckbox {
            page: 0,
            rect: a_field_rect(),
            style: a_text_style(),
        },
    )
    .expect("placing a second field should succeed");

    let fields = reopened.list_form_fields();
    let ids: BTreeSet<_> = fields.iter().map(|field| field.id).collect();
    assert_eq!(fields.len(), 2);
    assert_eq!(ids.len(), 2, "the two fields must have distinct ids");
}

#[test]
fn moving_a_field_is_undoable_and_redoable() {
    let handle = a_blank_page();
    let id = place_a_text_field(&handle);
    let moved = FfiRect {
        x: 100.0,
        y: 500.0,
        width: 180.0,
        height: 24.0,
    };

    apply_edit(
        &handle,
        FfiEditCommand::MoveFormField {
            field_id: id,
            to: moved,
        },
    )
    .expect("moving should succeed");
    assert_eq!(handle.list_form_fields()[0].rect, moved);

    assert!(undo(&handle), "undo should succeed");
    assert_eq!(handle.list_form_fields()[0].rect, a_field_rect());

    assert!(redo(&handle), "redo should succeed");
    assert_eq!(handle.list_form_fields()[0].rect, moved);
}

#[test]
fn resizing_restyling_and_renaming_each_reach_the_listed_field() {
    let handle = a_blank_page();
    let id = place_a_text_field(&handle);

    let bigger = FfiRect {
        x: 72.0,
        y: 560.0,
        width: 300.0,
        height: 48.0,
    };
    apply_edit(
        &handle,
        FfiEditCommand::ResizeFormField {
            field_id: id,
            to: bigger,
        },
    )
    .expect("resizing should succeed");
    assert_eq!(handle.list_form_fields()[0].rect, bigger);

    let restyled = FfiTextStyle {
        font: FfiFontFamily::Courier,
        size_pt: 9.0,
        color: FfiColor {
            r: 20,
            g: 40,
            b: 60,
        },
    };
    apply_edit(
        &handle,
        FfiEditCommand::RestyleFormField {
            field_id: id,
            style: restyled,
        },
    )
    .expect("restyling should succeed");
    assert_eq!(handle.list_form_fields()[0].style, restyled);

    apply_edit(
        &handle,
        FfiEditCommand::RenameFormField {
            field_id: id,
            name: "Signatory".to_string(),
        },
    )
    .expect("renaming should succeed");
    assert_eq!(handle.list_form_fields()[0].name, "Signatory");
}

#[test]
fn removing_a_field_takes_it_off_the_panel_and_undo_brings_it_back() {
    let handle = a_blank_page();
    let id = place_a_text_field(&handle);

    apply_edit(&handle, FfiEditCommand::RemoveFormField { field_id: id })
        .expect("removing should succeed");
    assert!(handle.list_form_fields().is_empty());

    assert!(undo(&handle), "undo should succeed");
    let fields = handle.list_form_fields();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].id, id);
}

#[test]
fn a_command_naming_an_absent_field_is_refused_by_id() {
    let handle = a_blank_page();
    place_a_text_field(&handle);

    let error = apply_edit(&handle, FfiEditCommand::RemoveFormField { field_id: 9_999 })
        .expect_err("an unknown field id should be refused");

    assert!(
        matches!(error, FfiError::FormFieldNotFound { field_id: 9_999 }),
        "unexpected error: {error}"
    );
}

/// `pdf-form::ops::set_value` owns the rule; what this asserts is that the
/// boundary consults it *before* recording, so a value the field cannot hold
/// never reaches the edit log and cannot take a later save down with it.
#[test]
fn a_value_that_does_not_match_the_fields_kind_is_refused_before_it_is_recorded() {
    let handle = a_blank_page();
    let id = place_a_text_field(&handle);

    let error = apply_edit(
        &handle,
        FfiEditCommand::SetFieldValue {
            field_id: id,
            value: FfiFieldValue::Checked { checked: true },
        },
    )
    .expect_err("a checkbox value on a text field should be refused");

    assert!(
        matches!(error, FfiError::UnsupportedOperation { .. }),
        "unexpected error: {error}"
    );
    assert_eq!(
        handle.list_form_fields()[0].value,
        FfiFieldValue::Text {
            text: String::new()
        }
    );
}

#[test]
fn a_dropdown_refuses_a_choice_it_does_not_offer() {
    let handle = a_blank_page();
    apply_edit(
        &handle,
        FfiEditCommand::AddDropdown {
            page: 0,
            rect: a_field_rect(),
            style: a_text_style(),
            options: vec!["one".to_string(), "two".to_string()],
            editable: false,
        },
    )
    .expect("placing a dropdown should succeed");
    let id = handle.list_form_fields()[0].id;

    let error = apply_edit(
        &handle,
        FfiEditCommand::SetFieldValue {
            field_id: id,
            value: FfiFieldValue::Choice {
                option: Some("three".to_string()),
            },
        },
    )
    .expect_err("an option the dropdown does not have should be refused");
    assert!(
        matches!(error, FfiError::UnsupportedOperation { .. }),
        "unexpected error: {error}"
    );

    apply_edit(
        &handle,
        FfiEditCommand::SetFieldValue {
            field_id: id,
            value: FfiFieldValue::Choice {
                option: Some("two".to_string()),
            },
        },
    )
    .expect("an option it does offer should be accepted");
    assert_eq!(
        handle.list_form_fields()[0].value,
        FfiFieldValue::Choice {
            option: Some("two".to_string())
        }
    );
}

#[test]
fn renaming_a_field_onto_another_fields_name_is_refused() {
    let handle = a_blank_page();
    let first = place_a_text_field(&handle);
    place_a_text_field(&handle);
    let taken = handle.list_form_fields()[1].name.clone();

    let error = apply_edit(
        &handle,
        FfiEditCommand::RenameFormField {
            field_id: first,
            name: taken,
        },
    )
    .expect_err("a name another field already holds should be refused");

    assert!(
        matches!(error, FfiError::UnsupportedOperation { .. }),
        "unexpected error: {error}"
    );
}

/// Listing is a read, and a read stays available however restricted the
/// document is — the same posture `DocumentHandle::annotations` takes.
#[test]
fn listing_fields_on_a_document_that_has_none_is_empty_not_an_error() {
    assert!(a_blank_page().list_form_fields().is_empty());
}
