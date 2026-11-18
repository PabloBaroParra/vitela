//! Filling a field in, as the command a boundary records (T-235, from
//! `pdf-ffi`'s `form::set_field_value`).
//!
//! Filling is the one form edit outside phase-3 authoring: every boundary
//! that shows a form needs it, so it lives here once instead of in each.

use pdf_document::{Command, FieldValue, FormFieldId, FormFieldSet};

use crate::FormError;

/// The `SetFieldValue` command that fills field `id` with `value`.
///
/// The value is checked by [`crate::set_value`] against a *copy* of the
/// field — that rule mutates in place, and the field itself must only ever
/// change through the edit log — so a value the field cannot hold is
/// refused before anything is recorded, not at the save it would have taken
/// down. The command records the value that survived the check.
pub fn fill_field(
    fields: &FormFieldSet,
    id: FormFieldId,
    value: FieldValue,
) -> Result<Command, FormError> {
    let existing = fields.get(id).ok_or(FormError::FieldNotFound(id))?;
    let mut validated = existing.clone();
    crate::set_value(&mut validated, value)?;
    Ok(Command::SetFieldValue {
        id,
        from: existing.value.clone(),
        to: validated.value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Color, FontFamily, PageId, Rect, TextStyle};

    fn style() -> TextStyle {
        TextStyle {
            font: FontFamily::Helvetica,
            size_pt: 12.0,
            color: Color { r: 0, g: 0, b: 0 },
        }
    }

    fn rect() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
        }
    }

    fn fields() -> FormFieldSet {
        let mut fields = FormFieldSet::new();
        fields.insert(crate::text_field(
            FormFieldId(3),
            PageId(0),
            "Name".to_string(),
            rect(),
            style(),
            false,
            Some(4),
        ));
        fields
    }

    #[test]
    fn a_fill_records_the_value_it_replaces_and_the_new_one() {
        let command = fill_field(
            &fields(),
            FormFieldId(3),
            FieldValue::Text("Ada".to_string()),
        );

        assert_eq!(
            command,
            Ok(Command::SetFieldValue {
                id: FormFieldId(3),
                from: FieldValue::Text(String::new()),
                to: FieldValue::Text("Ada".to_string()),
            })
        );
    }

    /// The rule's own cases live with the rule (`ops::set_value`); this
    /// asserts the command is only built from a value that passed it.
    #[test]
    fn a_value_the_field_cannot_hold_never_becomes_a_command() {
        let fields = fields();

        assert!(matches!(
            fill_field(&fields, FormFieldId(3), FieldValue::Checked(true)),
            Err(FormError::UnsupportedOperation(_))
        ));
        assert!(matches!(
            fill_field(
                &fields,
                FormFieldId(3),
                FieldValue::Text("Too long".to_string())
            ),
            Err(FormError::InvalidValue(_))
        ));
    }

    #[test]
    fn an_absent_field_is_reported_by_id() {
        assert_eq!(
            fill_field(&fields(), FormFieldId(9), FieldValue::Checked(true)),
            Err(FormError::FieldNotFound(FormFieldId(9)))
        );
    }
}
