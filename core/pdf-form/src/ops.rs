//! Form field edit operations (T-134): move, resize, restyle, set value.
//!
//! Mirrors `pdf-annotate::ops`. Unlike annotations, `rect` and `style` are
//! unconditional fields on every `FormField` regardless of `FormFieldKind`,
//! so `move_field`/`resize_field`/`restyle_field` cannot fail the way
//! `move_annotation`/`restyle_annotation` can for an `Ink` or a `Stamp` —
//! only `set_value` validates against the field's kind.
//!
//! Wiring these into `EditLog` commands (`MoveFormField`, `ResizeFormField`,
//! `RestyleFormField`, `SetFieldValue`) already exists (T-132); a caller
//! validates the new value here first, then records the command with the
//! validated value as its `to`.

use crate::error::FormError;
use pdf_document::{FieldValue, FormField, FormFieldKind, FormFieldSet, Rect, TextStyle};

/// Replaces a field's `rect`.
pub fn move_field(field: &mut FormField, to: Rect) {
    field.rect = to;
}

/// Replaces a field's `rect`. Structurally identical to [`move_field`] and
/// kept separate for intent, same reasoning as `pdf_document::Command`'s
/// `MoveImage`/`ResizeImage` split: a move comes from a drag, a resize from
/// a handle.
pub fn resize_field(field: &mut FormField, to: Rect) {
    field.rect = to;
}

/// Replaces a field's `style`.
pub fn restyle_field(field: &mut FormField, to: TextStyle) {
    field.style = to;
}

/// Renames a field's `/T`, after checking `to` against the rest of `fields`:
/// trimmed of surrounding whitespace, must not end up empty, and must not
/// collide with another field's name (`field` itself is exempt, so renaming
/// a field to the name it already has is a no-op success rather than a
/// spurious collision).
pub fn rename_field(
    field: &mut FormField,
    fields: &FormFieldSet,
    to: String,
) -> Result<(), FormError> {
    let trimmed = to.trim();
    if trimmed.is_empty() {
        return Err(FormError::InvalidValue(
            "a field name cannot be empty".to_string(),
        ));
    }
    let taken = fields
        .iter()
        .any(|other| other.id != field.id && other.name == trimmed);
    if taken {
        return Err(FormError::InvalidValue(format!(
            "\"{trimmed}\" is already used by another field"
        )));
    }
    field.name = trimmed.to_string();
    Ok(())
}

/// Sets a field's value, after checking it against the field's kind:
/// `Checked` only targets a `Checkbox`; `Text` only targets a `Text` field,
/// and must not exceed its `max_len` when set; `Choice` only targets a
/// `RadioGroup` or `Dropdown`, and (for a `RadioGroup`, or a `Dropdown` with
/// `editable: false`) `Some` must name one of the field's own options — a
/// `None` choice (nothing selected) is always accepted.
pub fn set_value(field: &mut FormField, value: FieldValue) -> Result<(), FormError> {
    match (&field.kind, &value) {
        (FormFieldKind::Checkbox, FieldValue::Checked(_)) => {}
        (FormFieldKind::Text { max_len, .. }, FieldValue::Text(text)) => {
            if let Some(max_len) = max_len {
                let len = text.chars().count();
                if len > *max_len as usize {
                    return Err(FormError::InvalidValue(format!(
                        "text is {len} characters, field's max_len is {max_len}"
                    )));
                }
            }
        }
        (FormFieldKind::RadioGroup { options }, FieldValue::Choice(Some(chosen))) => {
            if !options.iter().any(|option| &option.export_value == chosen) {
                return Err(FormError::InvalidValue(format!(
                    "\"{chosen}\" is not one of this radio group's options"
                )));
            }
        }
        (FormFieldKind::RadioGroup { .. }, FieldValue::Choice(None)) => {}
        (FormFieldKind::Dropdown { options, editable }, FieldValue::Choice(Some(chosen))) => {
            if !*editable && !options.iter().any(|option| option == chosen) {
                return Err(FormError::InvalidValue(format!(
                    "\"{chosen}\" is not one of this dropdown's options"
                )));
            }
        }
        (FormFieldKind::Dropdown { .. }, FieldValue::Choice(None)) => {}
        _ => return Err(FormError::UnsupportedOperation("set_value")),
    }

    field.value = value;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Color, FieldOrigin, FontFamily, FormFieldId, PageId, RadioOption};

    fn rect() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
        }
    }

    fn style() -> TextStyle {
        TextStyle {
            font: FontFamily::Helvetica,
            size_pt: 12.0,
            color: Color { r: 0, g: 0, b: 0 },
        }
    }

    fn text_field(max_len: Option<u32>) -> FormField {
        FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Text_1".to_string(),
            rect: rect(),
            style: style(),
            value: FieldValue::Text(String::new()),
            kind: FormFieldKind::Text {
                multiline: false,
                max_len,
            },
            origin: FieldOrigin::New,
        }
    }

    fn checkbox_field() -> FormField {
        FormField {
            id: FormFieldId(2),
            page: PageId(0),
            name: "Checkbox_1".to_string(),
            rect: rect(),
            style: style(),
            value: FieldValue::Checked(false),
            kind: FormFieldKind::Checkbox,
            origin: FieldOrigin::New,
        }
    }

    fn radio_field() -> FormField {
        FormField {
            id: FormFieldId(3),
            page: PageId(0),
            name: "Radio_1".to_string(),
            rect: rect(),
            style: style(),
            value: FieldValue::Choice(None),
            kind: FormFieldKind::RadioGroup {
                options: vec![
                    RadioOption {
                        export_value: "Yes".to_string(),
                        rect: rect(),
                    },
                    RadioOption {
                        export_value: "No".to_string(),
                        rect: rect(),
                    },
                ],
            },
            origin: FieldOrigin::New,
        }
    }

    fn dropdown_field(editable: bool) -> FormField {
        FormField {
            id: FormFieldId(4),
            page: PageId(0),
            name: "Dropdown_1".to_string(),
            rect: rect(),
            style: style(),
            value: FieldValue::Choice(None),
            kind: FormFieldKind::Dropdown {
                options: vec!["A".to_string(), "B".to_string()],
                editable,
            },
            origin: FieldOrigin::New,
        }
    }

    #[test]
    fn move_field_replaces_rect() {
        let mut field = checkbox_field();
        let to = Rect {
            x: 5.0,
            y: 5.0,
            ..rect()
        };
        move_field(&mut field, to);
        assert_eq!(field.rect, to);
    }

    #[test]
    fn resize_field_replaces_rect() {
        let mut field = checkbox_field();
        let to = Rect {
            width: 200.0,
            ..rect()
        };
        resize_field(&mut field, to);
        assert_eq!(field.rect, to);
    }

    #[test]
    fn restyle_field_replaces_style() {
        let mut field = text_field(None);
        let to = TextStyle {
            font: FontFamily::Courier,
            size_pt: 14.0,
            color: Color { r: 255, g: 0, b: 0 },
        };
        restyle_field(&mut field, to);
        assert_eq!(field.style, to);
    }

    #[test]
    fn rename_field_accepts_a_free_name() {
        let mut field = text_field(None);
        let fields = FormFieldSet::new();
        rename_field(&mut field, &fields, "Full Name".to_string()).expect("name is free");
        assert_eq!(field.name, "Full Name");
    }

    #[test]
    fn rename_field_trims_surrounding_whitespace() {
        let mut field = text_field(None);
        let fields = FormFieldSet::new();
        rename_field(&mut field, &fields, "  Full Name  ".to_string()).expect("name is free");
        assert_eq!(field.name, "Full Name");
    }

    #[test]
    fn rename_field_rejects_an_empty_name() {
        let mut field = text_field(None);
        let fields = FormFieldSet::new();
        let result = rename_field(&mut field, &fields, "   ".to_string());
        assert!(matches!(result, Err(FormError::InvalidValue(_))));
        assert_eq!(
            field.name, "Text_1",
            "a rejected rename must not change the field"
        );
    }

    #[test]
    fn rename_field_rejects_a_name_another_field_already_uses() {
        let mut field = text_field(None);
        let mut fields = FormFieldSet::new();
        fields.insert(field.clone());
        fields.insert(checkbox_field());
        let result = rename_field(&mut field, &fields, "Checkbox_1".to_string());
        assert!(matches!(result, Err(FormError::InvalidValue(_))));
    }

    #[test]
    fn rename_field_to_its_own_current_name_is_not_a_collision() {
        let mut field = text_field(None);
        let mut fields = FormFieldSet::new();
        fields.insert(field.clone());
        rename_field(&mut field, &fields, "Text_1".to_string())
            .expect("a field is not its own collision");
        assert_eq!(field.name, "Text_1");
    }

    #[test]
    fn set_value_checks_a_checkbox() {
        let mut field = checkbox_field();
        set_value(&mut field, FieldValue::Checked(true)).expect("checkbox accepts Checked");
        assert_eq!(field.value, FieldValue::Checked(true));
    }

    #[test]
    fn set_value_rejects_checked_on_a_text_field() {
        let mut field = text_field(None);
        let result = set_value(&mut field, FieldValue::Checked(true));
        assert!(matches!(result, Err(FormError::UnsupportedOperation(_))));
    }

    #[test]
    fn set_value_accepts_text_within_max_len() {
        let mut field = text_field(Some(5));
        set_value(&mut field, FieldValue::Text("hello".to_string())).expect("fits max_len");
        assert_eq!(field.value, FieldValue::Text("hello".to_string()));
    }

    #[test]
    fn set_value_rejects_text_exceeding_max_len() {
        let mut field = text_field(Some(4));
        let result = set_value(&mut field, FieldValue::Text("hello".to_string()));
        assert!(matches!(result, Err(FormError::InvalidValue(_))));
    }

    #[test]
    fn set_value_accepts_a_radio_option_that_exists() {
        let mut field = radio_field();
        set_value(&mut field, FieldValue::Choice(Some("Yes".to_string())))
            .expect("Yes is a real option");
        assert_eq!(field.value, FieldValue::Choice(Some("Yes".to_string())));
    }

    #[test]
    fn set_value_rejects_a_radio_option_that_does_not_exist() {
        let mut field = radio_field();
        let result = set_value(&mut field, FieldValue::Choice(Some("Maybe".to_string())));
        assert!(matches!(result, Err(FormError::InvalidValue(_))));
    }

    #[test]
    fn set_value_accepts_clearing_a_radio_selection() {
        let mut field = radio_field();
        field.value = FieldValue::Choice(Some("Yes".to_string()));
        set_value(&mut field, FieldValue::Choice(None)).expect("clearing is always allowed");
        assert_eq!(field.value, FieldValue::Choice(None));
    }

    #[test]
    fn set_value_rejects_an_unlisted_dropdown_choice_when_not_editable() {
        let mut field = dropdown_field(false);
        let result = set_value(&mut field, FieldValue::Choice(Some("Z".to_string())));
        assert!(matches!(result, Err(FormError::InvalidValue(_))));
    }

    #[test]
    fn set_value_accepts_any_dropdown_choice_when_editable() {
        let mut field = dropdown_field(true);
        set_value(&mut field, FieldValue::Choice(Some("Z".to_string())))
            .expect("editable dropdown accepts free text");
        assert_eq!(field.value, FieldValue::Choice(Some("Z".to_string())));
    }
}
