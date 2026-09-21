//! AcroForm fields across the FFI boundary (T-140, Batch 20): the shapes a
//! side panel reads, and the translation from the nine field commands a
//! shell sends into the real `pdf_document::Command`s that carry them.
//!
//! Its own module rather than more of [`crate::types`]/[`crate::document`],
//! for the same reason [`crate::compress`] is: what crosses here is a second
//! crate's vocabulary (`pdf-form`) plus the validation that vocabulary owns,
//! not another way to edit a page.
//!
//! ## Where each decision is made
//!
//! - **Identity** is this boundary's: a field id is allocated here, one past
//!   the highest the open document already holds (see
//!   [`next_form_field_id`]). Unlike annotations — which
//!   `pdf_save::document_from_lopdf` always starts empty — an opened
//!   document arrives with the ids `pdf_form::read_form_fields` assigned to
//!   the fields already in the file, so starting a counter at zero would
//!   hand a new field an id another field already answers to.
//! - **Naming** is `pdf-document`'s: `FormFieldSet::unique_name` picks the
//!   `/T`, from the same four bases the GTK4 shell uses, so two shells
//!   editing the same document produce the same names. A caller that wants
//!   its own name sends [`FfiEditCommand::RenameFormField`] after placing
//!   the field — which is exactly what the GTK4 shell's own inspector does.
//! - **Validation** is `pdf-form`'s: [`set_field_value`] and
//!   [`rename_field`] run `pdf_form::ops` against a *copy* of the field and
//!   only then build the command, so a value the field cannot hold is
//!   refused before anything is recorded rather than at the save it would
//!   have taken down.
//!
//! Nothing here reads or writes PDF objects; `pdf-save` does that at save
//! time from the `FormFieldSet` these commands mutate.

use pdf_document::{
    Command, Document, FieldOrigin, FieldValue, FontFamily, FormField, FormFieldId, FormFieldKind,
    PageId, RadioOption, TextStyle,
};

use crate::error::FfiError;
use crate::types::{FfiColor, FfiEditCommand, FfiRect};

/// Mirrors `pdf_document::FontFamily` — the three Standard-14 families a
/// field's `/DA` can name (Batch 20 decision 3: no embedded fonts in v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiFontFamily {
    Helvetica,
    TimesRoman,
    Courier,
}

impl From<FontFamily> for FfiFontFamily {
    fn from(font: FontFamily) -> Self {
        match font {
            FontFamily::Helvetica => FfiFontFamily::Helvetica,
            FontFamily::TimesRoman => FfiFontFamily::TimesRoman,
            FontFamily::Courier => FfiFontFamily::Courier,
        }
    }
}

impl From<FfiFontFamily> for FontFamily {
    fn from(font: FfiFontFamily) -> Self {
        match font {
            FfiFontFamily::Helvetica => FontFamily::Helvetica,
            FfiFontFamily::TimesRoman => FontFamily::TimesRoman,
            FfiFontFamily::Courier => FontFamily::Courier,
        }
    }
}

/// Mirrors `pdf_document::TextStyle` — what a field's text looks like, and
/// what round-trips through its `/DA` string.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiTextStyle {
    pub font: FfiFontFamily,
    pub size_pt: f64,
    pub color: FfiColor,
}

impl From<TextStyle> for FfiTextStyle {
    fn from(style: TextStyle) -> Self {
        Self {
            font: style.font.into(),
            size_pt: style.size_pt,
            color: style.color.into(),
        }
    }
}

impl From<FfiTextStyle> for TextStyle {
    fn from(style: FfiTextStyle) -> Self {
        Self {
            font: style.font.into(),
            size_pt: style.size_pt,
            color: style.color.into(),
        }
    }
}

/// Mirrors `pdf_document::FieldValue` — what a field currently holds. Which
/// variant is meaningful depends on the field's [`FfiFormFieldKind`];
/// `pdf_form::ops::set_value` is what decides, and this boundary asks it
/// before recording anything (see [`set_field_value`]).
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum FfiFieldValue {
    Text {
        text: String,
    },
    Checked {
        checked: bool,
    },
    /// `None` means nothing selected — no radio button on, no dropdown value
    /// chosen yet.
    Choice {
        option: Option<String>,
    },
}

impl From<FieldValue> for FfiFieldValue {
    fn from(value: FieldValue) -> Self {
        match value {
            FieldValue::Text(text) => FfiFieldValue::Text { text },
            FieldValue::Checked(checked) => FfiFieldValue::Checked { checked },
            FieldValue::Choice(option) => FfiFieldValue::Choice { option },
        }
    }
}

impl From<FfiFieldValue> for FieldValue {
    fn from(value: FfiFieldValue) -> Self {
        match value {
            FfiFieldValue::Text { text } => FieldValue::Text(text),
            FfiFieldValue::Checked { checked } => FieldValue::Checked(checked),
            FfiFieldValue::Choice { option } => FieldValue::Choice(option),
        }
    }
}

/// Mirrors `pdf_document::RadioOption` — one button of a radio group, with
/// its own placement and the `/AP` state name it selects when on.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiRadioOption {
    pub export_value: String,
    pub rect: FfiRect,
}

impl From<RadioOption> for FfiRadioOption {
    fn from(option: RadioOption) -> Self {
        Self {
            export_value: option.export_value,
            rect: option.rect.into(),
        }
    }
}

impl From<FfiRadioOption> for RadioOption {
    fn from(option: FfiRadioOption) -> Self {
        Self {
            export_value: option.export_value,
            rect: option.rect.into(),
        }
    }
}

/// Mirrors `pdf_document::FormFieldKind` — what a field can hold and how it
/// is laid out, independent of its current value.
///
/// Fixed at four variants where the core enum is `#[non_exhaustive]`:
/// pushbuttons, listboxes and signature fields round-trip opaquely through
/// `pdf-save` and are deliberately never offered as editable (Batch 20
/// "Fuera de scope"). [`ffi_form_field`] therefore never has to render one.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum FfiFormFieldKind {
    Text {
        multiline: bool,
        max_len: Option<u32>,
    },
    Checkbox,
    RadioGroup {
        options: Vec<FfiRadioOption>,
    },
    Dropdown {
        options: Vec<String>,
        editable: bool,
    },
}

/// Where a field came from, as much of `pdf_document::FieldOrigin` as a
/// shell can act on.
///
/// The indirect object id `FieldOrigin::Existing` carries is deliberately
/// dropped: it addresses an object in the opened file, which is `pdf-save`'s
/// business and meaningless on the other side of a binding. What a shell
/// does need is the one bit — a field the file already had is one pdfium
/// rasterizes from its own `/AP`, so an overlay that draws its value too
/// draws it twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiFieldOrigin {
    New,
    Existing,
}

/// One AcroForm field as a side panel sees it — the FFI shape of
/// `pdf_document::FormField`, returned by
/// `DocumentHandle::list_form_fields`.
///
/// Converted one way only. A shell never sends a whole field back: every
/// command below names the field by `id` and carries just the attribute it
/// changes, so the boundary can resolve the `from` half itself and the
/// dropped object id above costs nothing.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiFormField {
    pub id: u64,
    pub page: u32,
    pub name: String,
    pub rect: FfiRect,
    pub style: FfiTextStyle,
    pub value: FfiFieldValue,
    pub kind: FfiFormFieldKind,
    pub origin: FfiFieldOrigin,
}

pub(crate) fn ffi_form_field(field: &FormField) -> FfiFormField {
    let kind = match &field.kind {
        FormFieldKind::Text { multiline, max_len } => FfiFormFieldKind::Text {
            multiline: *multiline,
            max_len: *max_len,
        },
        FormFieldKind::Checkbox => FfiFormFieldKind::Checkbox,
        FormFieldKind::RadioGroup { options } => FfiFormFieldKind::RadioGroup {
            options: options.iter().cloned().map(Into::into).collect(),
        },
        FormFieldKind::Dropdown { options, editable } => FfiFormFieldKind::Dropdown {
            options: options.clone(),
            editable: *editable,
        },
        // `FormFieldKind` is `#[non_exhaustive]` from this crate's
        // perspective. A kind this build does not model is reported as the
        // least capable thing it could be — a read-only, single-line text
        // field — rather than failing to compile on an upstream addition,
        // the same posture `FfiFontKind`'s conversion takes.
        _ => FfiFormFieldKind::Text {
            multiline: false,
            max_len: Some(0),
        },
    };

    FfiFormField {
        id: field.id.0,
        page: field.page.0,
        name: field.name.clone(),
        rect: field.rect.into(),
        style: field.style.into(),
        value: field.value.clone().into(),
        kind,
        origin: match field.origin {
            FieldOrigin::New => FfiFieldOrigin::New,
            FieldOrigin::Existing(_) => FfiFieldOrigin::Existing,
        },
    }
}

/// The id a newly placed field should take on `document`: one past the
/// highest already in use, `0` when it holds none.
///
/// Read from the document rather than kept as a counter seeded at open,
/// because `Command::RemoveFormField`/undo move fields in and out of the set
/// between calls and the only thing that always knows which ids are spent is
/// the set itself. The GTK4 shell seeds a session counter the same way
/// (`app::document::next_form_field_id`) — it can, because it owns the whole
/// session; this boundary hands out one id per command and has nowhere
/// cheaper to keep it.
pub(crate) fn next_form_field_id(document: &Document) -> FormFieldId {
    FormFieldId(
        document
            .form_fields
            .iter()
            .map(|field| field.id.0)
            .max()
            .map_or(0, |highest| highest + 1),
    )
}

/// The `/T` prefix each kind's generated name is built from — the same four
/// the GTK4 shell uses (`app::state::FieldKind::name_base`), so a document
/// edited on two platforms does not grow two naming schemes.
fn name_base(kind: &FormFieldKind) -> &'static str {
    match kind {
        FormFieldKind::Text { .. } => "Text",
        FormFieldKind::Checkbox => "Checkbox",
        FormFieldKind::RadioGroup { .. } => "RadioGroup",
        FormFieldKind::Dropdown { .. } => "Dropdown",
        _ => "Field",
    }
}

/// Builds the `AddFormField` command for a field of `kind` at `rect`,
/// allocating both its id and its `/T` name.
pub(crate) fn add_field(
    document: &Document,
    page: u32,
    rect: FfiRect,
    style: FfiTextStyle,
    kind: FormFieldKind,
) -> Command {
    let id = next_form_field_id(document);
    let name = document.form_fields.unique_name(name_base(&kind));
    let style: TextStyle = style.into();
    let rect = rect.into();

    let field = match kind {
        FormFieldKind::Text { multiline, max_len } => {
            pdf_form::text_field(id, PageId(page), name, rect, style, multiline, max_len)
        }
        FormFieldKind::Checkbox => pdf_form::checkbox(id, PageId(page), name, rect, style),
        FormFieldKind::RadioGroup { options } => {
            pdf_form::radio_group(id, PageId(page), name, rect, style, options)
        }
        FormFieldKind::Dropdown { options, editable } => {
            pdf_form::dropdown(id, PageId(page), name, rect, style, options, editable)
        }
        // Unreachable: `kind` is built from an `FfiEditCommand` variant, and
        // there is one per modelled kind. Handled rather than `unreachable!`
        // because `FormFieldKind` is `#[non_exhaustive]` here.
        _ => pdf_form::text_field(id, PageId(page), name, rect, style, false, None),
    };
    Command::AddFormField(field)
}

/// Looks up the field `field_id` names, or reports it missing.
///
/// The real `Command::RemoveFormField` carries the removed field itself (so
/// undo never has to reconstruct it) and every other field command carries
/// its own `from` half, so all of them resolve the field before the command
/// is built — the same shape `RemoveAnnotation` has at this boundary.
fn field(document: &Document, field_id: u64) -> Result<FormField, FfiError> {
    document
        .form_fields
        .get(FormFieldId(field_id))
        .cloned()
        .ok_or(FfiError::FormFieldNotFound { field_id })
}

pub(crate) fn remove_field(document: &Document, field_id: u64) -> Result<Command, FfiError> {
    Ok(Command::RemoveFormField(field(document, field_id)?))
}

pub(crate) fn move_field(
    document: &Document,
    field_id: u64,
    to: FfiRect,
) -> Result<Command, FfiError> {
    let existing = field(document, field_id)?;
    Ok(Command::MoveFormField {
        id: existing.id,
        from: existing.rect,
        to: to.into(),
    })
}

pub(crate) fn resize_field(
    document: &Document,
    field_id: u64,
    to: FfiRect,
) -> Result<Command, FfiError> {
    let existing = field(document, field_id)?;
    Ok(Command::ResizeFormField {
        id: existing.id,
        from: existing.rect,
        to: to.into(),
    })
}

pub(crate) fn restyle_field(
    document: &Document,
    field_id: u64,
    style: FfiTextStyle,
) -> Result<Command, FfiError> {
    let existing = field(document, field_id)?;
    Ok(Command::RestyleFormField {
        id: existing.id,
        from: existing.style,
        to: style.into(),
    })
}

/// Validates `value` against the field's kind before building the command.
///
/// The check runs against a throwaway copy: `pdf_form::ops::set_value` is
/// written to mutate a field in place, and the field this boundary owns must
/// only ever change through the edit log, so the copy is what absorbs the
/// mutation and the command records the value that survived it.
pub(crate) fn set_field_value(
    document: &Document,
    field_id: u64,
    value: FfiFieldValue,
) -> Result<Command, FfiError> {
    let existing = field(document, field_id)?;
    let mut validated = existing.clone();
    pdf_form::set_value(&mut validated, value.into())?;
    Ok(Command::SetFieldValue {
        id: existing.id,
        from: existing.value,
        to: validated.value,
    })
}

/// Validates `name` against the rest of the set before building the command
/// — non-empty once trimmed, and not already another field's `/T`. Same
/// copy-first reasoning as [`set_field_value`], and the trimming is why the
/// command records `validated.name` rather than the caller's string.
pub(crate) fn rename_field(
    document: &Document,
    field_id: u64,
    name: String,
) -> Result<Command, FfiError> {
    let existing = field(document, field_id)?;
    let mut validated = existing.clone();
    pdf_form::rename_field(&mut validated, &document.form_fields, name)?;
    Ok(Command::RenameFormField {
        id: existing.id,
        from: existing.name,
        to: validated.name,
    })
}

/// Whether `command` places, removes, or changes the geometry, style or name
/// of a field — as opposed to filling one in.
///
/// The distinction is a permission one and it is the PDF spec's, not this
/// crate's. ISO 32000-1 table 22 bit 6 covers filling in an *existing*
/// field on its own; creating or modifying a field needs bit 4 as well
/// ("…and, if bit 4 is also set, create or modify interactive form fields").
/// A document granting bit 6 without bit 4 is both real and legal, so
/// `apply_edit` asks this before letting a structural field command through
/// — the same gate the GTK4 shell applies in `forms::command::
/// structural_edit_refusal`, so a restricted document behaves identically on
/// both.
///
/// `SetFieldValue` is the one field command this excludes, and there is no
/// predicate for "is a form command at all" because nothing needs one: bit 6
/// is the floor for every one of them, which is exactly what
/// `document::is_annotation_command` already grants by not excluding them.
pub(crate) fn is_structural_form_command(command: &FfiEditCommand) -> bool {
    matches!(
        command,
        FfiEditCommand::AddTextField { .. }
            | FfiEditCommand::AddCheckbox { .. }
            | FfiEditCommand::AddRadioGroup { .. }
            | FfiEditCommand::AddDropdown { .. }
            | FfiEditCommand::RemoveFormField { .. }
            | FfiEditCommand::MoveFormField { .. }
            | FfiEditCommand::ResizeFormField { .. }
            | FfiEditCommand::RestyleFormField { .. }
            | FfiEditCommand::RenameFormField { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Color, Rect};

    fn style() -> FfiTextStyle {
        FfiTextStyle {
            font: FfiFontFamily::Helvetica,
            size_pt: 12.0,
            color: FfiColor { r: 0, g: 0, b: 0 },
        }
    }

    fn rect() -> FfiRect {
        FfiRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
        }
    }

    fn text_kind() -> FormFieldKind {
        FormFieldKind::Text {
            multiline: false,
            max_len: None,
        }
    }

    /// `apply` is what puts the built field in the set; these tests go
    /// through it so the id/name allocation is exercised against a document
    /// that really holds what came before.
    fn place(document: &mut Document, kind: FormFieldKind) -> FormFieldId {
        let command = add_field(document, 0, rect(), style(), kind);
        let id = match &command {
            Command::AddFormField(field) => field.id,
            other => panic!("expected AddFormField, got {other:?}"),
        };
        let mut log = std::mem::take(&mut document.pending_edits);
        assert!(log.apply(document, command));
        document.pending_edits = log;
        id
    }

    #[test]
    fn the_first_field_on_an_empty_document_takes_id_zero() {
        assert_eq!(next_form_field_id(&Document::blank()), FormFieldId(0));
    }

    #[test]
    fn a_new_field_lands_one_past_the_highest_id_already_in_use() {
        let mut document = Document::blank();
        assert_eq!(place(&mut document, text_kind()), FormFieldId(0));
        assert_eq!(
            place(&mut document, FormFieldKind::Checkbox),
            FormFieldId(1)
        );
    }

    /// Ids are not positions: removing the lowest must not make the next
    /// field reuse an id the document still answers to.
    #[test]
    fn an_id_is_never_reused_while_a_higher_one_is_still_in_the_set() {
        let mut document = Document::blank();
        let first = place(&mut document, text_kind());
        place(&mut document, text_kind());
        document.form_fields.remove(first);

        assert_eq!(next_form_field_id(&document), FormFieldId(2));
    }

    #[test]
    fn generated_names_follow_the_gtk_shells_four_bases_and_never_collide() {
        let mut document = Document::blank();
        for kind in [
            text_kind(),
            text_kind(),
            FormFieldKind::Checkbox,
            FormFieldKind::RadioGroup {
                options: Vec::new(),
            },
            FormFieldKind::Dropdown {
                options: Vec::new(),
                editable: false,
            },
        ] {
            place(&mut document, kind);
        }

        let names: Vec<_> = document
            .form_fields
            .iter()
            .map(|field| field.name.clone())
            .collect();
        assert_eq!(
            names,
            [
                "Text_1",
                "Text_2",
                "Checkbox_1",
                "RadioGroup_1",
                "Dropdown_1"
            ]
        );
    }

    #[test]
    fn a_move_resolves_its_from_half_from_the_field_it_names() {
        let mut document = Document::blank();
        let id = place(&mut document, text_kind());
        let to = FfiRect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 20.0,
        };

        let command = move_field(&document, id.0, to).expect("the field exists");

        assert!(matches!(
            command,
            Command::MoveFormField { id: moved, from, to: landed }
                if moved == id && from == Rect { x: 0.0, y: 0.0, width: 100.0, height: 20.0 }
                    && landed == Rect::from(to)
        ));
    }

    #[test]
    fn every_command_that_names_a_field_reports_an_absent_one_by_id() {
        let document = Document::blank();
        let results = [
            remove_field(&document, 7),
            move_field(&document, 7, rect()),
            resize_field(&document, 7, rect()),
            restyle_field(&document, 7, style()),
            set_field_value(
                &document,
                7,
                FfiFieldValue::Text {
                    text: String::new(),
                },
            ),
            rename_field(&document, 7, "x".to_string()),
        ];

        for result in results {
            assert!(matches!(
                result,
                Err(FfiError::FormFieldNotFound { field_id: 7 })
            ));
        }
    }

    /// The rule's own cases live with the rule (`pdf_form::ops::set_value`);
    /// what this asserts is that the boundary consults it at all.
    #[test]
    fn a_value_the_field_cannot_hold_never_becomes_a_command() {
        let mut document = Document::blank();
        let id = place(&mut document, text_kind());

        let refused = set_field_value(&document, id.0, FfiFieldValue::Checked { checked: true });

        assert!(matches!(
            refused,
            Err(FfiError::UnsupportedOperation { .. })
        ));
    }

    #[test]
    fn a_rename_records_the_trimmed_name_not_the_callers_string() {
        let mut document = Document::blank();
        let id = place(&mut document, text_kind());

        let command = rename_field(&document, id.0, "  Signatory  ".to_string())
            .expect("a free name should be accepted");

        assert!(matches!(command, Command::RenameFormField { to, .. } if to == "Signatory"));
    }

    #[test]
    fn a_field_read_out_of_a_file_reports_itself_as_existing() {
        let mut document = Document::blank();
        let id = place(&mut document, text_kind());
        document
            .form_fields
            .get_mut(id)
            .expect("just placed")
            .origin = FieldOrigin::Existing((12, 0));

        let listed = ffi_form_field(document.form_fields.get(id).expect("just placed"));

        assert_eq!(listed.origin, FfiFieldOrigin::Existing);
    }

    #[test]
    fn every_kind_survives_the_trip_to_the_panel_shape() {
        let mut document = Document::blank();
        place(
            &mut document,
            FormFieldKind::Text {
                multiline: true,
                max_len: Some(8),
            },
        );
        place(&mut document, FormFieldKind::Checkbox);
        place(
            &mut document,
            FormFieldKind::RadioGroup {
                options: vec![RadioOption {
                    export_value: "yes".to_string(),
                    rect: Rect::from(rect()),
                }],
            },
        );
        place(
            &mut document,
            FormFieldKind::Dropdown {
                options: vec!["one".to_string()],
                editable: true,
            },
        );

        let kinds: Vec<_> = document
            .form_fields
            .iter()
            .map(|field| ffi_form_field(field).kind)
            .collect();

        assert_eq!(
            kinds,
            [
                FfiFormFieldKind::Text {
                    multiline: true,
                    max_len: Some(8)
                },
                FfiFormFieldKind::Checkbox,
                FfiFormFieldKind::RadioGroup {
                    options: vec![FfiRadioOption {
                        export_value: "yes".to_string(),
                        rect: rect(),
                    }]
                },
                FfiFormFieldKind::Dropdown {
                    options: vec!["one".to_string()],
                    editable: true
                },
            ]
        );
    }

    #[test]
    fn a_style_round_trips_through_the_boundary_shape() {
        let style = TextStyle {
            font: FontFamily::TimesRoman,
            size_pt: 9.5,
            color: Color { r: 1, g: 2, b: 3 },
        };

        assert_eq!(TextStyle::from(FfiTextStyle::from(style)), style);
    }

    #[test]
    fn every_field_value_round_trips_through_the_boundary_shape() {
        for value in [
            FieldValue::Text("hello".to_string()),
            FieldValue::Checked(true),
            FieldValue::Choice(None),
            FieldValue::Choice(Some("yes".to_string())),
        ] {
            assert_eq!(
                FieldValue::from(FfiFieldValue::from(value.clone())),
                value,
                "{value:?}"
            );
        }
    }

    /// Bit 6 alone is enough to fill in a field a document already has, so
    /// this must be the one field command the structural gate lets past.
    #[test]
    fn filling_a_field_is_the_one_form_command_that_is_not_structural() {
        assert!(!is_structural_form_command(
            &FfiEditCommand::SetFieldValue {
                field_id: 0,
                value: FfiFieldValue::Checked { checked: true },
            }
        ));
    }

    #[test]
    fn the_nine_field_editing_commands_all_need_the_modify_contents_bit() {
        for command in [
            FfiEditCommand::AddTextField {
                page: 0,
                rect: rect(),
                style: style(),
                multiline: false,
                max_len: None,
            },
            FfiEditCommand::AddCheckbox {
                page: 0,
                rect: rect(),
                style: style(),
            },
            FfiEditCommand::AddRadioGroup {
                page: 0,
                rect: rect(),
                style: style(),
                options: Vec::new(),
            },
            FfiEditCommand::AddDropdown {
                page: 0,
                rect: rect(),
                style: style(),
                options: Vec::new(),
                editable: false,
            },
            FfiEditCommand::RemoveFormField { field_id: 0 },
            FfiEditCommand::MoveFormField {
                field_id: 0,
                to: rect(),
            },
            FfiEditCommand::ResizeFormField {
                field_id: 0,
                to: rect(),
            },
            FfiEditCommand::RestyleFormField {
                field_id: 0,
                style: style(),
            },
            FfiEditCommand::RenameFormField {
                field_id: 0,
                name: "x".to_string(),
            },
        ] {
            assert!(is_structural_form_command(&command), "{command:?}");
        }
    }
}
