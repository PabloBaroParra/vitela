//! Error type for `pdf-annotate` builders, edit ops, and appearance-stream
//! construction.

use std::fmt;

/// Errors surfaced by this crate.
///
/// `#[non_exhaustive]`: mirrors the extensibility posture already used for
/// `pdf-document`'s `Command`/`AnnotationKind` (the signatures scope change
/// may need new failure modes later, e.g. invalid signature image input) —
/// marking this non-exhaustive costs nothing today and avoids a breaking
/// change later.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnnotateError {
    /// The requested operation does not apply to the given annotation kind
    /// (e.g. restyling a `Stamp`, which has no color; resizing an `Ink`,
    /// which has no rect).
    UnsupportedOperation(&'static str),
    /// Image bytes passed to an image-based builder or appearance-stream
    /// function could not be decoded as a supported format (PNG/JPEG).
    InvalidImage(String),
    /// Free text contains a character WinAnsi (and so Helvetica) cannot
    /// show. Refused rather than substituted: a `?` the user never typed
    /// is worse than being told.
    EncodingGap { character: char },
}

impl fmt::Display for AnnotateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnnotateError::UnsupportedOperation(op) => {
                write!(f, "unsupported operation for this annotation kind: {op}")
            }
            AnnotateError::InvalidImage(msg) => write!(f, "invalid image bytes: {msg}"),
            AnnotateError::EncodingGap { character } => {
                write!(f, "'{character}' cannot be shown in Helvetica (WinAnsi)")
            }
        }
    }
}

impl std::error::Error for AnnotateError {}
