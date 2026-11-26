use super::*;
use crate::error::ErrorKind;
use crate::model::{pdfs, PageEdit};
use pdf_manip::OpenCredentials;

fn open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
}

fn form() -> DocumentModel {
    open(pdfs::ACROFORM)
}

fn user_open_of_the_restricted_form() -> DocumentModel {
    DocumentModel::open(
        &pdfs::restricted_form(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("the user password opens it")
}

fn field<'a>(fields: &'a [FormFieldView], name: &str) -> &'a FormFieldView {
    fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("no field {name} in {fields:?}"))
}

fn id_of(model: &DocumentModel, name: &str) -> u32 {
    field(&model.form_fields(), name).id
}

fn fill(model: &mut DocumentModel, name: &str, value: FieldValue) -> Result<(), EngineError> {
    let id = id_of(model, name);
    model.fill_form_field(FieldFill { id, value })
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

fn choice(value: &str) -> FieldValue {
    FieldValue::Choice(Some(value.to_string()))
}

fn shape_of(model: &DocumentModel, name: &str) -> FieldShape {
    field(&model.form_fields(), name).shape.clone()
}

#[test]
fn the_files_own_fields_are_listed_at_open_with_their_values() {
    let model = form();
    let fields = model.form_fields();
    let mut names: Vec<_> = fields.iter().map(|field| field.name.as_str()).collect();
    names.sort_unstable();
    // The listbox is not a kind the core models: it stays in the file but is
    // not offered.
    assert_eq!(
        names,
        ["country", "full_name", "notes", "plan", "subscribe"]
    );
    assert!(fields.iter().all(|field| field.page == 0 && field.existing));

    let full_name = field(&fields, "full_name");
    assert_eq!(
        full_name.shape,
        FieldShape::Text {
            multiline: false,
            // reportlab writes its default `/MaxLen`.
            max_len: Some(100),
            value: "Ada Lovelace".to_string(),
        }
    );
    // reportlab's own box: x=72, y=680, 240 x 22.
    assert_eq!(
        (full_name.rect.x, full_name.rect.y),
        (72.0, 680.0),
        "{:?}",
        full_name.rect
    );
    assert_eq!((full_name.rect.width, full_name.rect.height), (240.0, 22.0));
    assert!(matches!(
        field(&fields, "notes").shape,
        FieldShape::Text {
            multiline: true,
            ..
        }
    ));
    assert_eq!(
        field(&fields, "subscribe").shape,
        FieldShape::Checkbox { checked: true }
    );
    let FieldShape::Radio { options, selected } = &field(&fields, "plan").shape else {
        panic!("plan is a radio group");
    };
    let exports: Vec<_> = options.iter().map(|(value, _)| value.as_str()).collect();
    assert_eq!(exports, ["basic", "pro"]);
    assert_eq!(selected.as_deref(), Some("pro"));
    assert_eq!(options[0].1.x, 72.0, "each button has its own box");
    assert_eq!(options[1].1.x, 132.0);
    assert_eq!(
        field(&fields, "country").shape,
        FieldShape::Dropdown {
            options: vec![
                "Argentina".to_string(),
                "Uruguay".to_string(),
                "Chile".to_string()
            ],
            editable: false,
            selected: Some("Uruguay".to_string()),
        }
    );
    let mut ids: Vec<_> = fields.iter().map(|field| field.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 5, "every field has its own id");
}

#[test]
fn a_fill_classifies_like_the_command_it_builds() {
    let model = form();
    let fill = FieldFill {
        id: id_of(&model, "full_name"),
        value: text("Grace"),
    };
    let command = model.fill_command(fill.clone()).expect("builds");
    assert_eq!(fill.class(), EditClass::of(&command));
}

#[test]
fn each_kind_is_filled_as_one_step_and_survives_a_save_and_reopen() {
    let mut model = form();
    // Accented text: the value goes out as UTF-16 and its appearance in
    // WinAnsi, and both have to come back.
    fill(&mut model, "full_name", text("José Peña")).unwrap();
    fill(&mut model, "notes", text("Línea uno\nLínea dos")).unwrap();
    fill(&mut model, "subscribe", FieldValue::Checked(false)).unwrap();
    fill(&mut model, "plan", choice("basic")).unwrap();
    fill(&mut model, "country", choice("Chile")).unwrap();
    assert!(model.can_undo());
    assert!(matches!(
        shape_of(&model, "full_name"),
        FieldShape::Text { value, .. } if value == "José Peña"
    ));

    let reopened = open(&model.save_to_bytes(false).unwrap());
    assert!(matches!(
        shape_of(&reopened, "full_name"),
        FieldShape::Text { value, .. } if value == "José Peña"
    ));
    assert!(matches!(
        shape_of(&reopened, "notes"),
        FieldShape::Text { value, .. } if value == "Línea uno\nLínea dos"
    ));
    assert_eq!(
        shape_of(&reopened, "subscribe"),
        FieldShape::Checkbox { checked: false }
    );
    assert!(matches!(
        shape_of(&reopened, "plan"),
        FieldShape::Radio { selected, .. } if selected.as_deref() == Some("basic")
    ));
    assert!(matches!(
        shape_of(&reopened, "country"),
        FieldShape::Dropdown { selected, .. } if selected.as_deref() == Some("Chile")
    ));
}

#[test]
fn undo_takes_a_fill_back_and_redo_brings_it_again() {
    let mut model = form();
    fill(&mut model, "country", choice("Argentina")).unwrap();
    fill(&mut model, "subscribe", FieldValue::Checked(false)).unwrap();

    assert!(model.undo());
    assert_eq!(
        shape_of(&model, "subscribe"),
        FieldShape::Checkbox { checked: true }
    );
    assert!(model.undo());
    assert!(matches!(
        shape_of(&model, "country"),
        FieldShape::Dropdown { selected, .. } if selected.as_deref() == Some("Uruguay")
    ));
    assert!(!model.can_undo());
    assert!(model.redo());
    assert!(matches!(
        shape_of(&model, "country"),
        FieldShape::Dropdown { selected, .. } if selected.as_deref() == Some("Argentina")
    ));
    // The undone fill is gone from the save too.
    let reopened = open(&model.save_to_bytes(false).unwrap());
    assert_eq!(
        shape_of(&reopened, "subscribe"),
        FieldShape::Checkbox { checked: true }
    );
}

#[test]
fn a_radio_group_and_a_dropdown_can_be_left_with_nothing_chosen() {
    let mut model = form();
    fill(&mut model, "plan", FieldValue::Choice(None)).unwrap();
    fill(&mut model, "country", FieldValue::Choice(None)).unwrap();
    assert!(matches!(
        shape_of(&model, "plan"),
        FieldShape::Radio { selected: None, .. }
    ));
    assert!(matches!(
        shape_of(&model, "country"),
        FieldShape::Dropdown { selected: None, .. }
    ));
}

#[test]
fn a_value_the_field_cannot_hold_is_unsupported_and_records_nothing() {
    let mut model = form();
    for (name, value) in [
        // A choice the field does not offer.
        ("country", choice("Atlantis")),
        ("plan", choice("enterprise")),
        // The wrong kind of value.
        ("subscribe", text("yes")),
        ("full_name", FieldValue::Checked(true)),
        ("plan", FieldValue::Checked(true)),
        ("country", text("Chile")),
        // Text past the field's `/MaxLen` (100).
        ("full_name", text(&"x".repeat(101))),
    ] {
        let err = fill(&mut model, name, value.clone()).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::UnsupportedOperation,
            "{name} {value:?}: {err}"
        );
    }
    assert!(!model.can_undo());
    assert!(!model.preview_stale());
}

#[test]
fn an_id_no_field_answers_to_is_form_field_not_found() {
    let mut model = form();
    let err = model
        .fill_form_field(FieldFill {
            id: 999,
            value: text("x"),
        })
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::FormFieldNotFound);
    assert_eq!(err.message, "no form field with id 999");
    assert!(!model.can_undo());
}

#[test]
fn without_the_fill_permission_every_fill_is_refused_first() {
    let mut model = user_open_of_the_restricted_form();
    assert!(!model.form_fill_allowed());
    // Reading needs no permission.
    let fields = model.form_fields();
    assert_eq!(fields.len(), 5);
    let id = field(&fields, "full_name").id;
    for fill in [
        FieldFill {
            id,
            value: text("Grace"),
        },
        // The permission is asked before the id and the value.
        FieldFill {
            id: 999,
            value: text("Grace"),
        },
        FieldFill {
            id,
            value: FieldValue::Checked(true),
        },
    ] {
        let err = model.fill_form_field(fill).unwrap_err();
        assert_eq!(err.kind, ErrorKind::EditNotPermitted);
    }
    assert!(!model.can_undo());

    let owner = DocumentModel::open(
        &pdfs::restricted_form(),
        &OpenCredentials::UserAndOwner {
            user: pdfs::USER_PASSWORD.into(),
            owner: pdfs::OWNER_PASSWORD.into(),
        },
    )
    .unwrap();
    assert!(owner.form_fill_allowed());
    assert!(form().form_fill_allowed());
}

#[test]
fn a_fill_stales_the_preview_and_so_do_its_undo_and_redo() {
    let mut model = form();
    fill(&mut model, "full_name", text("Grace Hopper")).unwrap();
    assert!(model.preview_stale(), "pdfium paints the field itself");
    model.mark_preview_fresh();
    assert!(model.undo());
    assert!(model.preview_stale());
    model.mark_preview_fresh();
    assert!(model.redo());
    assert!(model.preview_stale());
}

#[test]
fn a_document_without_a_form_lists_no_fields() {
    let mut model = open(&pdfs::text_document(&["one", "two"]));
    assert!(model.form_fields().is_empty());
    assert!(model.form_fill_allowed());
    // A page edit leaves an empty list empty.
    model
        .apply_edit(PageEdit::Rotate {
            from: 0,
            count: 1,
            delta_degrees: 90,
        })
        .unwrap();
    assert!(model.form_fields().is_empty());
}
