//! Form field data model (T-130, Batch 20) — text/checkbox/radio/dropdown
//! AcroForm fields as pure data, plus the `FormFieldSet` collection that
//! `Document` owns. Mirrors `annotation.rs`.
//!
//! A form field is a field dictionary (`/AcroForm /Fields`) whose visual
//! representation is a `/Subtype /Widget` annotation on the page — see
//! `docs/batch-forms.md` "Hecho clave del formato". Building the actual
//! `/AcroForm`, `/DA` and `/AP` PDF objects is `pdf-form`'s job (Fase 2/3)
//! — this crate only models the data.

use crate::annotation::{Color, Rect};
use crate::document::PageId;

/// Identifies a form field within a `Document`, independent of `PageId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FormFieldId(pub u64);

/// A Standard-14 font family (decision 3: no embedded fonts in v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    Helvetica,
    TimesRoman,
    Courier,
}

/// A field's appearance style — round-trips through `/DA` (decision 3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub font: FontFamily,
    pub size_pt: f64,
    pub color: Color,
}

/// A single radio button within a `RadioGroup` — one kid widget, each with
/// its own placement on the page. `export_value` is the `/AP` dictionary key
/// selected when this button is on, and is what `FieldValue::Choice` names
/// when this option is the group's current selection.
#[derive(Debug, Clone, PartialEq)]
pub struct RadioOption {
    pub export_value: String,
    pub rect: Rect,
}

/// The kind-specific shape of a form field: what it can hold and how it is
/// laid out, independent of its current value.
///
/// `#[non_exhaustive]`: pushbuttons and multi-select listboxes are
/// deliberately out of scope for v1 (see "Fuera de scope" in
/// `docs/batch-forms.md`) but existing ones must still round-trip opaquely —
/// keeping this open costs nothing today and avoids a breaking change if a
/// later batch models them.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FormFieldKind {
    Text {
        multiline: bool,
        max_len: Option<u32>,
    },
    Checkbox,
    RadioGroup {
        options: Vec<RadioOption>,
    },
    Dropdown {
        options: Vec<String>,
        editable: bool,
    },
}

/// A form field's current value. Which variant is meaningful depends on the
/// field's `FormFieldKind` — `pdf-form`'s `ops.rs` (T-134) validates that a
/// `Checked` value only targets a `Checkbox` and a `Choice` value only names
/// an option the field actually has when it is not editable.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Text(String),
    Checked(bool),
    /// `None` means no option selected (radio: no button on; dropdown: no
    /// value chosen yet).
    Choice(Option<String>),
}

/// Where a field came from: a field this session created, or one already
/// present in the opened PDF at the given indirect object id (`(number,
/// generation)`, a raw tuple because `pdf-document` cannot depend on lopdf —
/// mirrors the reasoning against a richer `AnnotationKind` payload).
///
/// Save uses this to decide how to write the field back: `New` appends a
/// fresh field+widget dict, `Existing` clones-and-modifies the original so
/// fields foreign to this editor are never duplicated or dropped (decision
/// 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldOrigin {
    New,
    Existing((u32, u16)),
}

/// A single AcroForm field attached to a page.
#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub id: FormFieldId,
    pub page: PageId,
    /// The field's fully-qualified `/T` name. Must be unique within the
    /// `FormFieldSet` — see `FormFieldSet::unique_name`.
    pub name: String,
    pub rect: Rect,
    pub style: TextStyle,
    pub value: FieldValue,
    pub kind: FormFieldKind,
    pub origin: FieldOrigin,
}

/// The collection of form fields owned by a `Document`.
///
/// Backed by a `Vec`, like `AnnotationSet`, so `/Fields` write order is
/// deterministic (spec "Cross-Platform Feature Parity" byte-identical CI
/// check) rather than a `HashMap`'s unspecified order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormFieldSet {
    fields: Vec<FormField>,
}

impl FormFieldSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a field. Does not check for duplicate ids or names — callers
    /// (EditLog command application) are responsible for both; use
    /// `unique_name` beforehand to pick a name that cannot collide.
    pub fn insert(&mut self, field: FormField) {
        self.fields.push(field);
    }

    /// Removes and returns the field with the given id, if present.
    pub fn remove(&mut self, id: FormFieldId) -> Option<FormField> {
        let index = self.fields.iter().position(|f| f.id == id)?;
        Some(self.fields.remove(index))
    }

    /// Swaps in a new value for the field that already carries `field`'s id,
    /// **keeping its position**, and returns the previous value. Returns
    /// `None` and leaves the set untouched when no field has that id.
    ///
    /// Mirrors `AnnotationSet::replace` for the same reason: a
    /// remove-then-insert pair would move the field to the end of `/Fields`
    /// on every move/resize/restyle/set_value, breaking the determinism
    /// guarantee above.
    pub fn replace(&mut self, field: FormField) -> Option<FormField> {
        let slot = self
            .fields
            .iter_mut()
            .find(|existing| existing.id == field.id)?;
        Some(std::mem::replace(slot, field))
    }

    pub fn get(&self, id: FormFieldId) -> Option<&FormField> {
        self.fields.iter().find(|f| f.id == id)
    }

    /// Mutable lookup, for commands (`MoveFormField`, `RestyleFormField`,
    /// `SetFieldValue`, …) that change one attribute of an existing field
    /// in place rather than replacing the whole value.
    pub fn get_mut(&mut self, id: FormFieldId) -> Option<&mut FormField> {
        self.fields.iter_mut().find(|f| f.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &FormField> {
        self.fields.iter()
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Generates a `/T` name derived from `base` that no field in the set
    /// currently uses, trying `"{base}_1"`, `"{base}_2"`, … in order.
    pub fn unique_name(&self, base: &str) -> String {
        let mut suffix = 1u32;
        loop {
            let candidate = format!("{base}_{suffix}");
            if !self.fields.iter().any(|f| f.name == candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_style() -> TextStyle {
        TextStyle {
            font: FontFamily::Helvetica,
            size_pt: 12.0,
            color: Color { r: 0, g: 0, b: 0 },
        }
    }

    fn sample_field(id: u64, name: &str) -> FormField {
        FormField {
            id: FormFieldId(id),
            page: PageId(0),
            name: name.to_string(),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            style: sample_style(),
            value: FieldValue::Text(String::new()),
            kind: FormFieldKind::Text {
                multiline: false,
                max_len: None,
            },
            origin: FieldOrigin::New,
        }
    }

    #[test]
    fn insert_then_get_returns_the_field() {
        let mut set = FormFieldSet::new();
        set.insert(sample_field(1, "Text_1"));

        assert_eq!(set.len(), 1);
        assert!(set.get(FormFieldId(1)).is_some());
    }

    #[test]
    fn remove_returns_and_deletes_the_field() {
        let mut set = FormFieldSet::new();
        set.insert(sample_field(1, "Text_1"));

        let removed = set.remove(FormFieldId(1)).expect("should be present");
        assert_eq!(removed.id, FormFieldId(1));
        assert!(set.is_empty());
        assert!(set.get(FormFieldId(1)).is_none());
    }

    #[test]
    fn remove_missing_id_returns_none() {
        let mut set = FormFieldSet::new();
        assert!(set.remove(FormFieldId(42)).is_none());
    }

    #[test]
    fn replace_keeps_the_field_in_place() {
        let mut set = FormFieldSet::new();
        set.insert(sample_field(1, "Text_1"));
        set.insert(sample_field(2, "Text_2"));
        set.insert(sample_field(3, "Text_3"));
        let mut edited = sample_field(2, "Text_2");
        edited.value = FieldValue::Text("hello".to_string());

        let previous = set.replace(edited).expect("id 2 is present");

        assert_eq!(previous.value, FieldValue::Text(String::new()));
        assert_eq!(
            set.iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![FormFieldId(1), FormFieldId(2), FormFieldId(3)],
            "an edit must not move the field to the end of the set"
        );
        assert_eq!(
            set.get(FormFieldId(2)).expect("present").value,
            FieldValue::Text("hello".to_string())
        );
    }

    #[test]
    fn replace_missing_id_returns_none_and_adds_nothing() {
        let mut set = FormFieldSet::new();
        set.insert(sample_field(1, "Text_1"));

        assert!(set.replace(sample_field(42, "Text_42")).is_none());
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn unique_name_starts_at_one() {
        let set = FormFieldSet::new();
        assert_eq!(set.unique_name("Text"), "Text_1");
    }

    #[test]
    fn unique_name_skips_taken_suffixes() {
        let mut set = FormFieldSet::new();
        set.insert(sample_field(1, "Text_1"));
        set.insert(sample_field(2, "Text_2"));

        assert_eq!(set.unique_name("Text"), "Text_3");
    }

    #[test]
    fn unique_name_is_independent_per_base() {
        let mut set = FormFieldSet::new();
        set.insert(sample_field(1, "Text_1"));

        assert_eq!(set.unique_name("Checkbox"), "Checkbox_1");
    }
}
