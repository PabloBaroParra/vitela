//! Form field builders (T-133): construct pure-data
//! [`pdf_document::FormField`] values for each of the four supported kinds.
//!
//! Mirrors `pdf-annotate::builders` — "no I/O" pure functions. Every builder
//! here always succeeds: unlike `stamp_from_image_bytes`, nothing here
//! decodes external bytes. Validating a field's *value* against its kind
//! (e.g. a `Choice` naming an option that does not exist) is `ops::set_value`
//! (T-134), not construction — a freshly built field always starts unset.

use pdf_document::{
    FieldOrigin, FieldValue, FormField, FormFieldId, FormFieldKind, PageId, RadioOption, Rect,
    TextStyle,
};

/// Builds a `Text` field, empty, at `origin: New`.
pub fn text_field(
    id: FormFieldId,
    page: PageId,
    name: impl Into<String>,
    rect: Rect,
    style: TextStyle,
    multiline: bool,
    max_len: Option<u32>,
) -> FormField {
    FormField {
        id,
        page,
        name: name.into(),
        rect,
        style,
        value: FieldValue::Text(String::new()),
        kind: FormFieldKind::Text { multiline, max_len },
        origin: FieldOrigin::New,
    }
}

/// Builds a `Checkbox` field, unchecked, at `origin: New`.
pub fn checkbox(
    id: FormFieldId,
    page: PageId,
    name: impl Into<String>,
    rect: Rect,
    style: TextStyle,
) -> FormField {
    FormField {
        id,
        page,
        name: name.into(),
        rect,
        style,
        value: FieldValue::Checked(false),
        kind: FormFieldKind::Checkbox,
        origin: FieldOrigin::New,
    }
}

/// Builds a `RadioGroup` field over `options`, with no button selected, at
/// `origin: New`.
pub fn radio_group(
    id: FormFieldId,
    page: PageId,
    name: impl Into<String>,
    rect: Rect,
    style: TextStyle,
    options: Vec<RadioOption>,
) -> FormField {
    FormField {
        id,
        page,
        name: name.into(),
        rect,
        style,
        value: FieldValue::Choice(None),
        kind: FormFieldKind::RadioGroup { options },
        origin: FieldOrigin::New,
    }
}

/// Builds a `Dropdown` field over `options`, with no value chosen, at
/// `origin: New`.
pub fn dropdown(
    id: FormFieldId,
    page: PageId,
    name: impl Into<String>,
    rect: Rect,
    style: TextStyle,
    options: Vec<String>,
    editable: bool,
) -> FormField {
    FormField {
        id,
        page,
        name: name.into(),
        rect,
        style,
        value: FieldValue::Choice(None),
        kind: FormFieldKind::Dropdown { options, editable },
        origin: FieldOrigin::New,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Color, FontFamily};

    fn rect() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
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

    #[test]
    fn text_field_starts_empty_and_new() {
        let field = text_field(
            FormFieldId(1),
            PageId(0),
            "Name",
            rect(),
            style(),
            false,
            Some(40),
        );
        assert_eq!(field.name, "Name");
        assert_eq!(field.value, FieldValue::Text(String::new()));
        assert_eq!(field.origin, FieldOrigin::New);
        match field.kind {
            FormFieldKind::Text { multiline, max_len } => {
                assert!(!multiline);
                assert_eq!(max_len, Some(40));
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn checkbox_starts_unchecked() {
        let field = checkbox(FormFieldId(2), PageId(0), "Agree", rect(), style());
        assert_eq!(field.value, FieldValue::Checked(false));
        assert!(matches!(field.kind, FormFieldKind::Checkbox));
    }

    #[test]
    fn radio_group_starts_with_no_selection() {
        let options = vec![
            RadioOption {
                export_value: "Yes".to_string(),
                rect: rect(),
            },
            RadioOption {
                export_value: "No".to_string(),
                rect: rect(),
            },
        ];
        let field = radio_group(
            FormFieldId(3),
            PageId(0),
            "Choice",
            rect(),
            style(),
            options.clone(),
        );
        assert_eq!(field.value, FieldValue::Choice(None));
        match field.kind {
            FormFieldKind::RadioGroup { options: o } => assert_eq!(o, options),
            other => panic!("expected RadioGroup, got {other:?}"),
        }
    }

    #[test]
    fn dropdown_starts_with_no_selection() {
        let options = vec!["A".to_string(), "B".to_string()];
        let field = dropdown(
            FormFieldId(4),
            PageId(0),
            "Pick",
            rect(),
            style(),
            options.clone(),
            false,
        );
        assert_eq!(field.value, FieldValue::Choice(None));
        match field.kind {
            FormFieldKind::Dropdown {
                options: o,
                editable,
            } => {
                assert_eq!(o, options);
                assert!(!editable);
            }
            other => panic!("expected Dropdown, got {other:?}"),
        }
    }
}
