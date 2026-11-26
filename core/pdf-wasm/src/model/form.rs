//! The document's AcroForm fields, over the core (B26 T-242): the list a
//! forms panel reads, filling one in as an undo step, and the fill
//! permission.
//!
//! Every rule is the core's, asked in the order `pdf-ffi`'s `apply_edit`
//! asks it for `SetFieldValue`: the permission gate (`pdf_save::check_edit`
//! with `EditClass::ANNOTATION`, bit 6 — filling needs nothing more) first;
//! then the build (`pdf_form::fill_field`, which refuses an id the document
//! does not hold and a value the field cannot take, against a copy); then the
//! model's own refusal on apply.
//!
//! Unlike annotations, fields are not the session's own: the opened file's
//! fields are in the set from the start (`pdf_save::document_from_lopdf`), so
//! the list is not empty at open. What stays here is the boundary's
//! vocabulary: the list names page *positions* (a field whose page is gone —
//! removed, kept for an undo — is not listed, as in `pdf-ffi`), and ids cross
//! as `u32`.
//!
//! A fill changes what the preview shows (`pdf_save::shows_in_session_preview`):
//! pdfium paints the field's appearance, which the save regenerates, so the
//! boundary refreshes the preview after one.

use pdf_document::{Command, FieldOrigin, FieldValue, FormField, FormFieldId, FormFieldKind, Rect};
use pdf_save::EditClass;

use super::DocumentModel;
use crate::error::EngineError;

/// Fills field `id` with `value` — `pdf-ffi`'s `FfiEditCommand::SetFieldValue`
/// in core types. Which `FieldValue` a field takes is its kind's: text for a
/// text field, `Checked` for a checkbox, `Choice` for a radio group or a
/// dropdown.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldFill {
    pub id: u32,
    pub value: FieldValue,
}

impl FieldFill {
    /// What a fill is, in the terms `pdf_save::check_edit` asks about: the
    /// annotate-or-fill permission, nothing else (no structural form edit).
    /// Must agree with `EditClass::of` on the built command
    /// (`a_fill_classifies_like_the_command_it_builds`).
    pub fn class(&self) -> EditClass {
        EditClass::ANNOTATION
    }
}

/// One field as a forms panel lists it: its id, the position its page sits
/// at now, its `/T`, its widget box (a radio group's whole group; each button
/// has its own in `options`), whether the opened file already had it, and
/// what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct FormFieldView {
    pub id: u32,
    pub page: u32,
    pub name: String,
    /// In the page's unrotated PDF space.
    pub rect: Rect,
    /// The file already had it, so pdfium paints it from its own
    /// appearance (`pdf-ffi`'s `FfiFieldOrigin::Existing`).
    pub existing: bool,
    pub shape: FieldShape,
}

/// What a field can hold and holds now — `pdf-ffi`'s `FfiFormFieldKind` and
/// `FfiFieldValue` in one, since the kind decides which value it carries.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldShape {
    Text {
        multiline: bool,
        max_len: Option<u32>,
        value: String,
    },
    Checkbox {
        checked: bool,
    },
    /// Each button with its export value and its own box; `selected` is the
    /// export value of the one on, `None` when none is.
    Radio {
        options: Vec<(String, Rect)>,
        selected: Option<String>,
    },
    /// A combo box. `editable` lets it take a value outside `options`.
    Dropdown {
        options: Vec<String>,
        editable: bool,
        selected: Option<String>,
    },
}

impl DocumentModel {
    /// Gates, builds and records one fill as one undo step. Nothing is
    /// recorded when any step refuses.
    ///
    /// Refused with `EditNotPermitted` when the document withholds the
    /// annotate-or-fill permission (asked first, even for an unknown id),
    /// `FormFieldNotFound` when no field answers to `id`, or
    /// `UnsupportedOperation` when the value is not one the field can hold
    /// (the wrong kind, a choice it does not offer, text past its maximum
    /// length) — the same kinds `pdf-ffi` gives.
    pub fn fill_form_field(&mut self, fill: FieldFill) -> Result<(), EngineError> {
        pdf_save::check_edit(self.document.security.as_ref(), fill.class())?;
        let command = self.fill_command(fill)?;
        self.record(command)
    }

    fn fill_command(&self, fill: FieldFill) -> Result<Command, EngineError> {
        Ok(pdf_form::fill_field(
            &self.document.form_fields,
            FormFieldId(u64::from(fill.id)),
            fill.value,
        )?)
    }

    /// Every field the document holds, in `/Fields` order, each with the
    /// position its page is at now (`pdf-ffi`'s `list_form_fields`). The
    /// opened file's fields are in it from the start. Read-only, so it
    /// answers however restricted the document is. Changes with every fill,
    /// undo and redo, and a page move moves the fields on it.
    pub fn form_fields(&self) -> Vec<FormFieldView> {
        self.document
            .form_fields
            .iter()
            .filter_map(|field| {
                let page = self.document.render_index(field.page)?;
                Some(FormFieldView {
                    // An id past a `u32` (never, from the core's reader) is
                    // not offered rather than wrapped onto another field's.
                    id: u32::try_from(field.id.0).ok()?,
                    // Positions crossed in as `u32`, so they fit going out.
                    page: page as u32,
                    name: field.name.clone(),
                    rect: field.rect,
                    existing: matches!(field.origin, FieldOrigin::Existing(_)),
                    shape: shape(field)?,
                })
            })
            .collect()
    }

    /// Whether the document lets its fields be filled in
    /// (`pdf_save::form_fill_allowed`) — the question `fill_form_field` asks
    /// first, so a panel can show itself read-only before the user types.
    pub fn form_fill_allowed(&self) -> bool {
        pdf_save::form_fill_allowed(self.document.security.as_ref())
    }
}

/// The field's kind and value together, or `None` for a kind this boundary
/// has no shape for (`FormFieldKind` is `#[non_exhaustive]`; the core's
/// reader models only these four) or a value its kind cannot hold. Such a
/// field stays in the document and the saved file; it is just not offered.
fn shape(field: &FormField) -> Option<FieldShape> {
    Some(match (&field.kind, &field.value) {
        (FormFieldKind::Text { multiline, max_len }, FieldValue::Text(value)) => FieldShape::Text {
            multiline: *multiline,
            max_len: *max_len,
            value: value.clone(),
        },
        (FormFieldKind::Checkbox, FieldValue::Checked(checked)) => {
            FieldShape::Checkbox { checked: *checked }
        }
        (FormFieldKind::RadioGroup { options }, FieldValue::Choice(selected)) => {
            FieldShape::Radio {
                options: options
                    .iter()
                    .map(|option| (option.export_value.clone(), option.rect))
                    .collect(),
                selected: selected.clone(),
            }
        }
        (FormFieldKind::Dropdown { options, editable }, FieldValue::Choice(selected)) => {
            FieldShape::Dropdown {
                options: options.clone(),
                editable: *editable,
                selected: selected.clone(),
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
