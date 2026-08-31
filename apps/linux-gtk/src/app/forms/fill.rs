//! The fill panel (T-142): one input per form field in the open document,
//! generated from `document.form_fields` rather than through `pdf-ffi` (this
//! shell bypasses it for every form-field feature, same posture as T-141).
//! Changing a control's value records `Command::SetFieldValue` immediately —
//! no separate "Apply" step, the same direct-manipulation posture
//! `style::connect_style_controls` documents for the field-style inspector.
//!
//! Unlike the placement/style controls (a fixed set, resynced in place by
//! `style::refresh`), the rows here are as many as the document has form
//! fields, so every call to [`refresh`] tears the row list down and rebuilds
//! it from the model. That is safe only because filling a value never goes
//! through `refresh` itself — see `command::fill_command`'s own doc for why:
//! doing so would destroy the very `Entry` the user is mid-keystroke in and
//! drop focus on every character typed.
//!
//! Every control is an `Entry`, a `CheckButton`, or a `DropDown` — the same
//! vocabulary `content_edit`'s retype editor and this module's own style
//! inspector already use elsewhere in the shell, rather than introducing a
//! multi-line text widget this codebase has never needed before.

use gtk::prelude::*;
use gtk::{Box as GtkBox, CheckButton, DropDown, Entry, Label, Orientation, StringList};
use pdf_document::{Command, FieldValue, FormField, FormFieldId, FormFieldKind, RadioOption};

use crate::app::state::Viewer;

use super::command::{apply_command, fill_command, model};
use super::SELECTION_GONE;

/// The dropdown/choice entry meaning "nothing selected" — index 0 in every
/// `DropDown` this module builds, ahead of the field's real options.
const NONE_CHOICE: &str = "(none)";

/// Rebuilds the fill panel's rows from `document.form_fields` — called from
/// `toolbar::update_forms_controls` after every selection change, structural
/// edit, undo/redo, and document open/close. Never called from a fill commit
/// itself (see the module doc).
pub(super) fn refresh(viewer: &Viewer) {
    let state = viewer.state.borrow();
    let session = state.session.as_ref();
    let fields: Vec<FormField> = session
        .and_then(|session| session.document_model.as_ref())
        .map(|document| document.form_fields.iter().cloned().collect())
        .unwrap_or_default();
    // Fill-in only needs the annotate bit (ISO 32000-1 Table 22 bit 6) —
    // mirrors `command::fill_command`'s own gate, not `structural_edit_refusal`
    // (which also requires the modify-contents bit for placing/moving fields).
    let enabled = session.is_some_and(|session| session.annotation_access.refusal().is_none());
    drop(state);

    while let Some(child) = viewer.forms.fill_rows.first_child() {
        viewer.forms.fill_rows.remove(&child);
    }
    viewer.forms.fill_placeholder.set_visible(fields.is_empty());
    viewer.forms.fill_rows.set_visible(!fields.is_empty());

    for field in &fields {
        viewer
            .forms
            .fill_rows
            .append(&build_row(viewer, field, enabled));
    }
}

fn build_row(viewer: &Viewer, field: &FormField, enabled: bool) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.add_css_class("fill-field-row");

    let label = Label::new(Some(&field.name));
    label.set_xalign(0.0);
    label.set_width_chars(12);
    label.set_wrap(true);
    row.append(&label);

    let control = build_control(viewer, field, enabled);
    control.set_hexpand(true);
    row.append(&control);

    row
}

fn build_control(viewer: &Viewer, field: &FormField, enabled: bool) -> gtk::Widget {
    let id = field.id;
    match &field.kind {
        FormFieldKind::Text { max_len, .. } => {
            let entry = Entry::new();
            if let FieldValue::Text(text) = &field.value {
                entry.set_text(text);
            }
            if let Some(max_len) = max_len {
                entry.set_max_length((*max_len).min(i32::MAX as u32) as i32);
            }
            entry.set_sensitive(enabled);
            entry.connect_changed({
                let viewer = viewer.clone();
                move |entry| {
                    commit_value(&viewer, id, FieldValue::Text(entry.text().to_string()));
                }
            });
            entry.upcast()
        }
        FormFieldKind::Checkbox => {
            let check = CheckButton::new();
            check.set_active(matches!(field.value, FieldValue::Checked(true)));
            check.set_sensitive(enabled);
            check.connect_toggled({
                let viewer = viewer.clone();
                move |check| {
                    commit_value(&viewer, id, FieldValue::Checked(check.is_active()));
                }
            });
            check.upcast()
        }
        FormFieldKind::RadioGroup { options } => {
            build_radio_group(viewer, id, options, &field.value, enabled)
        }
        FormFieldKind::Dropdown { options, editable } if *editable => {
            let entry = Entry::new();
            if let FieldValue::Choice(Some(text)) = &field.value {
                entry.set_text(text);
            }
            entry.set_sensitive(enabled);
            entry.connect_changed({
                let viewer = viewer.clone();
                move |entry| {
                    let text = entry.text();
                    let value = if text.is_empty() {
                        None
                    } else {
                        Some(text.to_string())
                    };
                    commit_value(&viewer, id, FieldValue::Choice(value));
                }
            });
            entry.upcast()
        }
        FormFieldKind::Dropdown { options, .. } => {
            build_dropdown(viewer, id, options, &field.value, enabled)
        }
        // `FormFieldKind` is `#[non_exhaustive]`: a future/unmodeled kind (a
        // pushbutton, say) shows as read-only rather than as a control that
        // could silently do nothing.
        _ => Label::new(Some("Unsupported field")).upcast(),
    }
}

/// The `DropDown` index for `value`, with index 0 reserved for
/// [`NONE_CHOICE`] and every option shifted up by one — falls back to 0 for
/// a choice the field no longer offers, same "never crash on stale state"
/// posture `style::font_from_selected` takes for an out-of-range index.
fn dropdown_index_for(options: &[String], value: &FieldValue) -> u32 {
    match value {
        FieldValue::Choice(Some(chosen)) => options
            .iter()
            .position(|option| option == chosen)
            .map(|index| index as u32 + 1)
            .unwrap_or(0),
        _ => 0,
    }
}

/// The inverse of [`dropdown_index_for`]: index 0 is always "no choice".
fn dropdown_choice_for(options: &[String], index: u32) -> Option<String> {
    if index == 0 {
        None
    } else {
        options.get(index as usize - 1).cloned()
    }
}

fn build_dropdown(
    viewer: &Viewer,
    id: FormFieldId,
    options: &[String],
    value: &FieldValue,
    enabled: bool,
) -> gtk::Widget {
    let mut items: Vec<&str> = vec![NONE_CHOICE];
    items.extend(options.iter().map(String::as_str));
    let dropdown = DropDown::new(Some(StringList::new(&items)), None::<gtk::Expression>);
    dropdown.set_selected(dropdown_index_for(options, value));
    dropdown.set_sensitive(enabled);

    let options = options.to_vec();
    dropdown.connect_selected_notify({
        let viewer = viewer.clone();
        move |dropdown| {
            let value = dropdown_choice_for(&options, dropdown.selected());
            commit_value(&viewer, id, FieldValue::Choice(value));
        }
    });
    dropdown.upcast()
}

/// One `CheckButton` per option, grouped as radio buttons — mirrors how the
/// field itself works in every real PDF viewer: at most one on at a time,
/// and (like a physical radio group) nothing in the UI clears a selection
/// once made. `pdf-form::ops::set_value` still accepts `Choice(None)` from a
/// caller, but this shell has no button that sends it.
///
/// Built in three passes — group, then set the initial active state, then
/// connect `toggled` — so wiring up the group never fires a handler for the
/// state a row is merely starting in.
fn build_radio_group(
    viewer: &Viewer,
    id: FormFieldId,
    options: &[RadioOption],
    value: &FieldValue,
    enabled: bool,
) -> gtk::Widget {
    let column = GtkBox::new(Orientation::Vertical, 4);
    let buttons: Vec<CheckButton> = options
        .iter()
        .map(|option| CheckButton::with_label(&option.export_value))
        .collect();
    for button in buttons.iter().skip(1) {
        button.set_group(Some(&buttons[0]));
    }

    let selected = match value {
        FieldValue::Choice(Some(chosen)) => Some(chosen.as_str()),
        _ => None,
    };
    for (button, option) in buttons.iter().zip(options) {
        button.set_active(selected == Some(option.export_value.as_str()));
        button.set_sensitive(enabled);
    }
    for (button, option) in buttons.iter().zip(options) {
        let export_value = option.export_value.clone();
        button.connect_toggled({
            let viewer = viewer.clone();
            move |button| {
                if button.is_active() {
                    commit_value(&viewer, id, FieldValue::Choice(Some(export_value.clone())));
                }
            }
        });
        column.append(button);
    }
    column.upcast()
}

/// Validates and records one `SetFieldValue`, the fill twin of
/// `style::restyle_selected`. Validation (`pdf_form::set_value`) runs on a
/// cloned field first — every control here is already built so it cannot
/// *offer* an invalid value (a capped `Entry` length, a `DropDown` limited to
/// real options), but recording still goes through the crate's own gate
/// rather than trusting that construction stays airtight forever.
fn commit_value(viewer: &Viewer, id: FormFieldId, value: FieldValue) {
    fill_command(viewer, move |session| {
        let document = model(session)?;
        let mut probe = document
            .form_fields
            .get(id)
            .cloned()
            .ok_or_else(|| SELECTION_GONE.to_string())?;
        let before = probe.value.clone();
        pdf_form::set_value(&mut probe, value.clone()).map_err(|error| error.to_string())?;
        apply_command(
            document,
            Command::SetFieldValue {
                id,
                from: before,
                to: value,
            },
        );
        Ok("Field value set. Changes are pending save.".to_string())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Vec<String> {
        vec!["A".to_string(), "B".to_string()]
    }

    #[test]
    fn dropdown_index_defaults_to_the_none_choice() {
        assert_eq!(dropdown_index_for(&options(), &FieldValue::Choice(None)), 0);
    }

    #[test]
    fn dropdown_index_finds_the_chosen_option() {
        assert_eq!(
            dropdown_index_for(&options(), &FieldValue::Choice(Some("B".to_string()))),
            2
        );
    }

    #[test]
    fn dropdown_index_falls_back_to_none_for_a_choice_the_field_no_longer_offers() {
        assert_eq!(
            dropdown_index_for(&options(), &FieldValue::Choice(Some("Z".to_string()))),
            0
        );
    }

    #[test]
    fn dropdown_choice_for_index_zero_is_always_none() {
        assert_eq!(dropdown_choice_for(&options(), 0), None);
    }

    #[test]
    fn dropdown_index_and_choice_round_trip_every_real_option() {
        for (index, option) in options().iter().enumerate() {
            let index = index as u32 + 1;
            assert_eq!(
                dropdown_choice_for(&options(), index).as_ref(),
                Some(option)
            );
            assert_eq!(
                dropdown_index_for(&options(), &FieldValue::Choice(Some(option.clone()))),
                index
            );
        }
    }
}
