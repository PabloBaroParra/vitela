//! Turning a finished placement gesture into a `FormField` (T-141).
//!
//! Every field-creation path in the shell funnels through here, so the kind
//! the user armed and the field that lands in the document cannot drift
//! apart — mirrors `annotations::builder`.

use pdf_document::{
    Color, FontFamily, FormField, FormFieldId, FormFieldSet, PageId, RadioOption, Rect, TextStyle,
};

use crate::app::state::{FieldKind, FormPlacement};

use super::geometry::committed_rect;

/// Style every freshly placed field starts with: Helvetica 12pt black.
/// T-142/T-143 (fill and tab order) have no reason yet to remember a
/// different default, and the inspector (`style::refresh`) lets it be
/// changed the moment the field is selected.
const DEFAULT_STYLE: TextStyle = TextStyle {
    font: FontFamily::Helvetica,
    size_pt: 12.0,
    color: Color { r: 0, g: 0, b: 0 },
};

/// Builds the field a finished [`FormPlacement`] describes, at `rect`.
///
/// `fields` supplies the `/T` name (`FormFieldSet::unique_name`) — read, not
/// mutated, since recording happens through `Command::AddFormField` like
/// every other edit.
fn field_at(
    fields: &FormFieldSet,
    kind: FieldKind,
    id: FormFieldId,
    page: PageId,
    rect: Rect,
) -> FormField {
    let name = fields.unique_name(kind.name_base());
    match kind {
        FieldKind::Text => pdf_form::text_field(id, page, name, rect, DEFAULT_STYLE, false, None),
        FieldKind::Checkbox => pdf_form::checkbox(id, page, name, rect, DEFAULT_STYLE),
        FieldKind::RadioGroup => pdf_form::radio_group(
            id,
            page,
            name,
            rect,
            DEFAULT_STYLE,
            default_radio_options(rect),
        ),
        FieldKind::Dropdown => pdf_form::dropdown(
            id,
            page,
            name,
            rect,
            DEFAULT_STYLE,
            default_dropdown_options(),
            false,
        ),
    }
}

/// The field a finished placement commits: `rect` comes from where the user
/// actually dragged (or the click default), the same "no creation path
/// carries a hard-coded rect" posture `annotations::builder::annotation_at`
/// documents.
pub(super) fn field_for_placement(
    fields: &FormFieldSet,
    id: FormFieldId,
    placement: &FormPlacement,
) -> FormField {
    let page = PageId(placement.page_index as u32);
    field_at(fields, placement.kind, id, page, committed_rect(placement))
}

/// Two stacked bands filling `rect`, the group's default layout: nothing in
/// T-141 asks the user to place radio buttons individually, so a freshly
/// placed group needs *some* starting geometry for its two options rather
/// than an empty one no reader could render — the user resizes the whole
/// field afterward like any other.
fn default_radio_options(rect: Rect) -> Vec<RadioOption> {
    let half_height = rect.height / 2.0;
    vec![
        RadioOption {
            export_value: "Option 1".to_string(),
            rect: Rect {
                x: rect.x,
                y: rect.y + half_height,
                width: rect.width,
                height: half_height,
            },
        },
        RadioOption {
            export_value: "Option 2".to_string(),
            rect: Rect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: half_height,
            },
        },
    ]
}

fn default_dropdown_options() -> Vec<String> {
    vec!["Option 1".to_string(), "Option 2".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{FieldOrigin, FieldValue, FormFieldKind};

    fn placement(kind: FieldKind, origin: (f64, f64), current: (f64, f64)) -> FormPlacement {
        FormPlacement {
            kind,
            page_index: 0,
            origin,
            current,
        }
    }

    #[test]
    fn a_placed_field_starts_new_with_the_default_style() {
        let fields = FormFieldSet::new();
        let field = field_for_placement(
            &fields,
            FormFieldId(1),
            &placement(FieldKind::Text, (100.0, 500.0), (300.0, 540.0)),
        );

        assert_eq!(field.origin, FieldOrigin::New);
        assert_eq!(field.style, DEFAULT_STYLE);
        assert_eq!(field.name, "Text_1");
        assert!(matches!(
            field.kind,
            FormFieldKind::Text {
                multiline: false,
                max_len: None
            }
        ));
        assert_eq!(field.value, FieldValue::Text(String::new()));
    }

    #[test]
    fn a_placed_field_lands_where_the_user_dragged() {
        let fields = FormFieldSet::new();
        let field = field_for_placement(
            &fields,
            FormFieldId(1),
            &placement(FieldKind::Checkbox, (100.0, 500.0), (118.0, 518.0)),
        );

        assert_eq!((field.rect.x, field.rect.y), (100.0, 500.0));
        assert_eq!((field.rect.width, field.rect.height), (18.0, 18.0));
    }

    #[test]
    fn a_placed_radio_group_starts_with_two_stacked_options() {
        let fields = FormFieldSet::new();
        let field = field_for_placement(
            &fields,
            FormFieldId(1),
            &placement(FieldKind::RadioGroup, (100.0, 500.0), (300.0, 540.0)),
        );

        match field.kind {
            FormFieldKind::RadioGroup { options } => {
                assert_eq!(options.len(), 2);
                assert_eq!(options[0].rect.height, field.rect.height / 2.0);
                assert_eq!(options[1].rect.height, field.rect.height / 2.0);
                assert_eq!(
                    options[0].rect.y,
                    options[1].rect.y + options[1].rect.height
                );
            }
            other => panic!("expected RadioGroup, got {other:?}"),
        }
    }

    #[test]
    fn a_placed_dropdown_starts_with_two_options_and_is_not_editable() {
        let fields = FormFieldSet::new();
        let field = field_for_placement(
            &fields,
            FormFieldId(1),
            &placement(FieldKind::Dropdown, (100.0, 500.0), (300.0, 540.0)),
        );

        match field.kind {
            FormFieldKind::Dropdown { options, editable } => {
                assert_eq!(
                    options,
                    vec!["Option 1".to_string(), "Option 2".to_string()]
                );
                assert!(!editable);
            }
            other => panic!("expected Dropdown, got {other:?}"),
        }
    }

    #[test]
    fn a_second_field_of_the_same_kind_gets_a_distinct_name() {
        let mut fields = FormFieldSet::new();
        fields.insert(field_for_placement(
            &fields.clone(),
            FormFieldId(1),
            &placement(FieldKind::Text, (0.0, 0.0), (10.0, 10.0)),
        ));

        let second = field_for_placement(
            &fields,
            FormFieldId(2),
            &placement(FieldKind::Text, (0.0, 0.0), (10.0, 10.0)),
        );

        assert_eq!(second.name, "Text_2");
    }
}
