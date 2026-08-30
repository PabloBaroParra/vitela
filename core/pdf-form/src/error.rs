//! Error type for `pdf-form` builders, edit ops, appearance-stream
//! construction, and existing-AcroForm parsing.

use std::fmt;

/// Errors surfaced by this crate.
///
/// `#[non_exhaustive]`: mirrors `pdf-annotate`'s `AnnotateError` — new
/// failure modes (e.g. an appearance-stream construction error in T-136) can
/// be added later without a breaking change.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormError {
    /// The requested operation does not apply to the given field (there are
    /// none yet — move/resize/restyle apply to every `FormFieldKind` — but
    /// the match stays exhaustive-by-wildcard because `FormFieldKind` is
    /// `#[non_exhaustive]`, and `set_value` uses this for a value whose kind
    /// (`Checked`/`Text`/`Choice`) does not match the field's kind).
    UnsupportedOperation(&'static str),
    /// A value failed a `set_value` validation rule: a `Choice` naming an
    /// option the field does not have (or, for a non-editable `Dropdown`/any
    /// `RadioGroup`, one it does not offer), or `Text` longer than the
    /// field's `max_len`.
    InvalidValue(String),
}

impl fmt::Display for FormError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormError::UnsupportedOperation(op) => {
                write!(f, "unsupported operation for this field kind: {op}")
            }
            FormError::InvalidValue(msg) => write!(f, "invalid field value: {msg}"),
        }
    }
}

impl std::error::Error for FormError {}
