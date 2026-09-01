//! `pdf-form`: AcroForm field builders, edit operations, `/DA` styling,
//! appearance-stream construction, and existing-AcroForm read interop
//! (Batch 20, `docs/batch-forms.md`).
//!
//! Isolated from `pdf-annotate` (decision 1): form fields carry
//! document-level state (`/AcroForm`, `/DR`, unique `/T` names, a field
//! tree) that markup annotations do not — same isolation criterion as
//! `pdf-sign`.
//!
//! - [`builders`] (T-133): pure-data `FormField` constructors, one per kind.
//! - [`ops`] (T-134): move/resize/restyle/set-value operations on fields.
//! - [`da`] (T-135): `/DA` string ↔ `TextStyle` serialization.
//! - [`appearance`] (T-136): `/AP` content stream construction per kind.
//! - [`read`] (T-137): parses an existing `/AcroForm` into `FormField`s.
//! - [`error`]: `FormError`, the shared error type across this crate.

pub mod appearance;
pub mod builders;
pub mod da;
pub mod error;
pub mod ops;
pub mod read;

pub use appearance::{
    build_field_appearance, FieldAppearance, RadioButtonAppearance, CHECKBOX_ON_STATE,
    ZAPF_DINGBATS_RESOURCE,
};
pub use builders::{checkbox, dropdown, radio_group, text_field};
pub use da::{base_font_name, format_da, parse_da, resource_name};
pub use error::FormError;
pub use ops::{move_field, rename_field, resize_field, restyle_field, set_value};
pub use read::read_form_fields;
