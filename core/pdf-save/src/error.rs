//! Error type for `pdf-save` (Batch 6, T-032..T-038).
//!
//! Wraps the underlying crate errors this crate's writers touch
//! (`pdf_manip::ManipError` for the page-ops bridge, `pdf_annotate::AnnotateError`
//! for appearance-stream construction, `lopdf::Error` and `std::io::Error` for
//! serialization, `image::ImageError` for page-export encoding) so callers
//! never need to depend on those crates directly to handle a `pdf-save`
//! failure.

use std::fmt;

/// Errors produced by `pdf-save`'s bridge, save strategies, and export API.
#[derive(Debug)]
#[non_exhaustive]
pub enum SaveError {
    /// Failure from `pdf-manip`'s page-op functions during replay-on-save
    /// (T-032a).
    Manip(pdf_manip::ManipError),
    /// Failure building an annotation's PDF-level appearance (from
    /// `pdf-annotate`).
    Annotate(pdf_annotate::AnnotateError),
    /// Failure editing a page's content stream (from `pdf-edit`) — most
    /// often an `EncodingGap`, i.e. the run's font cannot represent the
    /// replacement text, which is refused before anything is written.
    Edit(pdf_edit::EditError),
    /// Underlying lopdf failure (parse/structural) not otherwise classified.
    Lopdf(lopdf::Error),
    /// I/O failure while serializing (from lopdf's `save_to`/`save_internal`,
    /// which return `std::io::Result`).
    Io(std::io::Error),
    /// Failure encoding a rendered page bitmap as PNG/JPEG (T-037).
    Image(image::ImageError),
    /// Failure from `pdf-render` while rendering a page for export (T-037).
    Render(pdf_render::RenderError),
    /// A save was requested with `SaveIntent::Default` re-encryption but no
    /// `SecurityContext` was supplied, or the reconciled page set could not
    /// be resolved to a base document — a caller-contract violation rather
    /// than a data problem.
    InvalidSaveRequest(&'static str),
    /// The file carries a signature, this save would break it, and the
    /// caller has not said it knows.
    ///
    /// Not a refusal to do the work: it is how the decision is handed back
    /// to the person whose signature it is. The caller asks the user, then
    /// re-submits with
    /// [`crate::SignatureAcknowledgement::ProceedAndInvalidate`]. See
    /// [`crate::will_invalidate_signatures`], which answers the same question
    /// without attempting a save, so a shell can warn before the user even
    /// presses save.
    SignaturesWouldBeInvalidated,
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Manip(err) => write!(f, "page-op replay failed: {err}"),
            SaveError::Annotate(err) => write!(f, "annotation appearance build failed: {err}"),
            SaveError::Edit(err) => write!(f, "page content edit failed: {err}"),
            SaveError::Lopdf(err) => write!(f, "lopdf error: {err}"),
            SaveError::Io(err) => write!(f, "I/O error while saving: {err}"),
            SaveError::Image(err) => write!(f, "image encode failed: {err}"),
            SaveError::Render(err) => write!(f, "render failed during export: {err}"),
            SaveError::InvalidSaveRequest(msg) => write!(f, "invalid save request: {msg}"),
            SaveError::SignaturesWouldBeInvalidated => write!(
                f,
                "this save rewrites the file and the file carries a signature, \
                 which would stop verifying; re-submit acknowledging it once \
                 the user has been told"
            ),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SaveError::Manip(err) => Some(err),
            SaveError::Annotate(err) => Some(err),
            SaveError::Edit(err) => Some(err),
            SaveError::Lopdf(err) => Some(err),
            SaveError::Io(err) => Some(err),
            SaveError::Image(err) => Some(err),
            SaveError::Render(err) => Some(err),
            SaveError::InvalidSaveRequest(_) | SaveError::SignaturesWouldBeInvalidated => None,
        }
    }
}

impl From<pdf_manip::ManipError> for SaveError {
    fn from(err: pdf_manip::ManipError) -> Self {
        SaveError::Manip(err)
    }
}

impl From<pdf_annotate::AnnotateError> for SaveError {
    fn from(err: pdf_annotate::AnnotateError) -> Self {
        SaveError::Annotate(err)
    }
}

impl From<pdf_edit::EditError> for SaveError {
    fn from(err: pdf_edit::EditError) -> Self {
        SaveError::Edit(err)
    }
}

impl From<lopdf::Error> for SaveError {
    fn from(err: lopdf::Error) -> Self {
        SaveError::Lopdf(err)
    }
}

impl From<std::io::Error> for SaveError {
    fn from(err: std::io::Error) -> Self {
        SaveError::Io(err)
    }
}

impl From<image::ImageError> for SaveError {
    fn from(err: image::ImageError) -> Self {
        SaveError::Image(err)
    }
}

impl From<pdf_render::RenderError> for SaveError {
    fn from(err: pdf_render::RenderError) -> Self {
        SaveError::Render(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_are_human_readable() {
        let err = SaveError::InvalidSaveRequest("no security context");
        assert_eq!(err.to_string(), "invalid save request: no security context");
    }

    #[test]
    fn manip_error_converts_via_from() {
        let manip_err = pdf_manip::ManipError::PasswordRequired;
        let err: SaveError = manip_err.into();
        assert!(matches!(err, SaveError::Manip(_)));
    }
}
