//! `FfiError` (T-041): the single error type crossing the UniFFI boundary.
//!
//! Mirrors design.md's FFI Design: "a single `FfiError` enum (mirrors core
//! `PdfError`) mapped to Swift `Error`/C# exception/Kotlin exception via
//! UniFFI's error type support — no raw error strings". There is no single
//! `PdfError` type in this workspace (each core crate owns its own error
//! enum — `pdf_manip::ManipError`, `pdf_render::RenderError`,
//! `pdf_save::SaveError`, `pdf_annotate::AnnotateError`); this enum is the
//! FFI-facing union of the *meaningful, caller-actionable* categories across
//! all four, discriminated by variant (never a bare string a caller would
//! have to parse) — the spike's own `SpikeError` (T-008) validated that
//! UniFFI maps an enum like this to a typed exception hierarchy per
//! language, not a stringly-typed failure.
//!
//! Every source error is converted via `From` — callers inside this crate
//! use `?` and never construct raw error strings themselves. Each source
//! enum is `#[non_exhaustive]` from its owning crate's perspective, so every
//! match below carries a wildcard arm that folds any not-yet-modeled
//! variant into [`FfiError::Internal`] (still typed, just less specific)
//! rather than failing to compile on an upstream addition.

use thiserror::Error;

/// Errors returned across the UniFFI boundary by every `pdf-ffi` command.
#[derive(Debug, Error, uniffi::Error)]
pub enum FfiError {
    /// The document is encrypted and no password (or an incomplete one) was
    /// supplied to open it.
    #[error("a password is required to open this document")]
    PasswordRequired,
    /// The supplied password matched neither the user nor owner password.
    #[error("the supplied password was rejected")]
    WrongPassword,
    /// The document's `/Encrypt` dictionary uses a security handler this
    /// workspace doesn't (yet) recognize.
    #[error("document uses an unsupported security handler")]
    UnsupportedSecurityHandler,
    /// The referenced `DocumentHandle` (or its render-side counterpart) is
    /// not valid — never opened/created, already closed, or a blank
    /// document with no pages yet to render.
    #[error("document handle not found")]
    DocumentNotFound,
    /// The referenced `BitmapHandle` is invalid: never issued, or already
    /// released (spec "Bitmap Handle Lifecycle").
    #[error("bitmap handle not found or already released")]
    BitmapNotFound,
    /// A 0-indexed page index was out of bounds for this document.
    #[error("page index {index} out of bounds")]
    PageIndexOutOfBounds { index: u32 },
    /// `apply_edit`'s `RemoveAnnotation` referenced an id absent from this
    /// document's current `AnnotationSet`.
    #[error("annotation {annotation_id} not found on this document")]
    AnnotationNotFound { annotation_id: u64 },
    /// Image bytes passed to `insert_image_stamp` (or an `AddStamp` edit
    /// command) could not be decoded as a supported format (PNG/JPEG).
    #[error("invalid image bytes: {detail}")]
    InvalidImage { detail: String },
    /// A save was requested that violates a `pdf-save` contract (e.g.
    /// incremental save with structural page changes, or strip-protection
    /// requested as an incremental update).
    #[error("invalid save request: {detail}")]
    InvalidSaveRequest { detail: String },
    /// The file carries a signature that this save would break, and the
    /// shell has not said the user was told.
    ///
    /// The shell's move is to warn — `will_invalidate_signatures` answers
    /// the same question before anything is attempted — and then re-save
    /// with `FfiSignatureAcknowledgement::ProceedAndInvalidate` if the user
    /// wants to go ahead. Its own variant rather than an
    /// `InvalidSaveRequest`, because it is not a programming mistake: it is a
    /// decision that belongs to the person whose signature it is.
    #[error("saving would invalidate an existing signature and this was not acknowledged")]
    SignaturesWouldBeInvalidated,
    /// The requested operation does not apply to the given annotation kind.
    #[error("unsupported operation: {detail}")]
    UnsupportedOperation { detail: String },
    /// A page-content text edit's replacement text has a character the
    /// run's font cannot represent (Batch 21 decision 3). Reported per
    /// character, before the content stream is touched, so a shell can show
    /// the user exactly what to fix rather than a generic failure.
    #[error("font {resource_font_name} cannot represent {character:?}")]
    EncodingGap {
        character: String,
        resource_font_name: String,
    },
    /// Rendering failed for a reason surfaced by the render backend.
    #[error("render failed: {detail}")]
    RenderFailed { detail: String },
    /// I/O failure (reading a source path, writing a saved file).
    #[error("I/O error: {detail}")]
    Io { detail: String },
    /// Any other failure not modeled above — still a typed variant (not a
    /// raw string return value), carrying the source error's `Display`
    /// output as diagnostic detail.
    #[error("internal error: {detail}")]
    Internal { detail: String },
}

impl From<pdf_manip::ManipError> for FfiError {
    fn from(err: pdf_manip::ManipError) -> Self {
        use pdf_manip::ManipError as E;
        match err {
            E::PasswordRequired => FfiError::PasswordRequired,
            E::WrongPassword => FfiError::WrongPassword,
            E::UnsupportedSecurityHandler => FfiError::UnsupportedSecurityHandler,
            E::InvalidPageNumber(index) => FfiError::PageIndexOutOfBounds { index },
            E::InvalidPageIndex(index) => FfiError::PageIndexOutOfBounds {
                index: index as u32,
            },
            other => FfiError::Internal {
                detail: other.to_string(),
            },
        }
    }
}

impl From<pdf_render::RenderError> for FfiError {
    fn from(err: pdf_render::RenderError) -> Self {
        use pdf_render::RenderError as E;
        match err {
            E::InvalidPassword => FfiError::WrongPassword,
            E::PageIndexOutOfBounds(index) => FfiError::PageIndexOutOfBounds { index },
            E::BitmapNotFound => FfiError::BitmapNotFound,
            E::DocumentNotFound => FfiError::DocumentNotFound,
            other => FfiError::RenderFailed {
                detail: other.to_string(),
            },
        }
    }
}

impl From<pdf_save::SaveError> for FfiError {
    fn from(err: pdf_save::SaveError) -> Self {
        use pdf_save::SaveError as E;
        match err {
            E::InvalidSaveRequest(message) => FfiError::InvalidSaveRequest {
                detail: message.to_string(),
            },
            E::SignaturesWouldBeInvalidated => FfiError::SignaturesWouldBeInvalidated,
            // Wrapper variants delegate to the wrapped error's own mapping so
            // a typed inner error (e.g. an out-of-bounds page during
            // replay-on-save) stays typed instead of folding into `Internal`.
            E::Manip(inner) => inner.into(),
            E::Annotate(inner) => inner.into(),
            E::Render(inner) => inner.into(),
            E::Edit(inner) => inner.into(),
            E::Io(inner) => inner.into(),
            other => FfiError::Internal {
                detail: other.to_string(),
            },
        }
    }
}

impl From<pdf_annotate::AnnotateError> for FfiError {
    fn from(err: pdf_annotate::AnnotateError) -> Self {
        use pdf_annotate::AnnotateError as E;
        match err {
            E::InvalidImage(message) => FfiError::InvalidImage { detail: message },
            E::UnsupportedOperation(op) => FfiError::UnsupportedOperation {
                detail: op.to_string(),
            },
            other => FfiError::Internal {
                detail: other.to_string(),
            },
        }
    }
}

impl From<pdf_edit::EditError> for FfiError {
    fn from(err: pdf_edit::EditError) -> Self {
        use pdf_edit::EditError as E;
        match err {
            E::EncodingGap {
                character,
                resource_font_name,
            } => FfiError::EncodingGap {
                character: character.to_string(),
                resource_font_name,
            },
            E::InvalidImage(message) => FfiError::InvalidImage { detail: message },
            other => FfiError::Internal {
                detail: other.to_string(),
            },
        }
    }
}

impl From<std::io::Error> for FfiError {
    fn from(err: std::io::Error) -> Self {
        FfiError::Io {
            detail: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manip_password_required_maps_to_typed_variant() {
        let err: FfiError = pdf_manip::ManipError::PasswordRequired.into();
        assert!(matches!(err, FfiError::PasswordRequired));
    }

    #[test]
    fn manip_wrong_password_maps_to_typed_variant() {
        let err: FfiError = pdf_manip::ManipError::WrongPassword.into();
        assert!(matches!(err, FfiError::WrongPassword));
    }

    #[test]
    fn manip_invalid_page_number_carries_the_index() {
        let err: FfiError = pdf_manip::ManipError::InvalidPageNumber(7).into();
        assert!(matches!(err, FfiError::PageIndexOutOfBounds { index: 7 }));
    }

    #[test]
    fn manip_unclassified_variant_folds_into_internal() {
        let err: FfiError = pdf_manip::ManipError::EmptyMerge.into();
        assert!(matches!(err, FfiError::Internal { .. }));
    }

    #[test]
    fn render_invalid_password_maps_to_wrong_password() {
        let err: FfiError = pdf_render::RenderError::InvalidPassword.into();
        assert!(matches!(err, FfiError::WrongPassword));
    }

    #[test]
    fn render_bitmap_not_found_maps_to_typed_variant() {
        let err: FfiError = pdf_render::RenderError::BitmapNotFound.into();
        assert!(matches!(err, FfiError::BitmapNotFound));
    }

    #[test]
    fn render_page_index_out_of_bounds_carries_the_index() {
        let err: FfiError = pdf_render::RenderError::PageIndexOutOfBounds(3).into();
        assert!(matches!(err, FfiError::PageIndexOutOfBounds { index: 3 }));
    }

    #[test]
    fn save_wrapped_manip_error_reuses_the_manip_mapping() {
        let err: FfiError =
            pdf_save::SaveError::Manip(pdf_manip::ManipError::InvalidPageNumber(7)).into();
        assert!(matches!(err, FfiError::PageIndexOutOfBounds { index: 7 }));
    }

    #[test]
    fn save_wrapped_annotate_error_reuses_the_annotate_mapping() {
        let err: FfiError = pdf_save::SaveError::Annotate(
            pdf_annotate::AnnotateError::InvalidImage("bad png".into()),
        )
        .into();
        assert!(matches!(err, FfiError::InvalidImage { .. }));
    }

    #[test]
    fn save_wrapped_render_error_reuses_the_render_mapping() {
        let err: FfiError =
            pdf_save::SaveError::Render(pdf_render::RenderError::PageIndexOutOfBounds(3)).into();
        assert!(matches!(err, FfiError::PageIndexOutOfBounds { index: 3 }));
    }

    #[test]
    fn save_io_error_maps_to_the_io_variant() {
        let err: FfiError = pdf_save::SaveError::Io(std::io::Error::other("disk full")).into();
        assert!(matches!(err, FfiError::Io { .. }));
    }

    #[test]
    fn save_invalid_save_request_carries_the_message() {
        let err: FfiError = pdf_save::SaveError::InvalidSaveRequest("bad request").into();
        match err {
            FfiError::InvalidSaveRequest { detail } => assert_eq!(detail, "bad request"),
            other => panic!("expected InvalidSaveRequest, got {other:?}"),
        }
    }

    #[test]
    fn annotate_invalid_image_carries_the_message() {
        let err: FfiError = pdf_annotate::AnnotateError::InvalidImage("bad png".into()).into();
        match err {
            FfiError::InvalidImage { detail } => assert_eq!(detail, "bad png"),
            other => panic!("expected InvalidImage, got {other:?}"),
        }
    }

    #[test]
    fn annotate_unsupported_operation_carries_the_message() {
        let err: FfiError = pdf_annotate::AnnotateError::UnsupportedOperation("resize ink").into();
        match err {
            FfiError::UnsupportedOperation { detail } => assert_eq!(detail, "resize ink"),
            other => panic!("expected UnsupportedOperation, got {other:?}"),
        }
    }

    #[test]
    fn edit_encoding_gap_carries_the_character_and_font_name() {
        let err: FfiError = pdf_edit::EditError::EncodingGap {
            character: '日',
            resource_font_name: "F1".to_string(),
        }
        .into();
        match err {
            FfiError::EncodingGap {
                character,
                resource_font_name,
            } => {
                assert_eq!(character, "日");
                assert_eq!(resource_font_name, "F1");
            }
            other => panic!("expected EncodingGap, got {other:?}"),
        }
    }

    #[test]
    fn edit_invalid_image_carries_the_message() {
        let err: FfiError = pdf_edit::EditError::InvalidImage("bad png".to_string()).into();
        match err {
            FfiError::InvalidImage { detail } => assert_eq!(detail, "bad png"),
            other => panic!("expected InvalidImage, got {other:?}"),
        }
    }

    #[test]
    fn edit_unclassified_variant_folds_into_internal() {
        let err: FfiError = pdf_edit::EditError::PageNotFound(pdf_document::PageId(3)).into();
        assert!(matches!(err, FfiError::Internal { .. }));
    }

    #[test]
    fn save_wrapped_edit_error_reuses_the_edit_mapping() {
        let err: FfiError = pdf_save::SaveError::Edit(pdf_edit::EditError::EncodingGap {
            character: 'x',
            resource_font_name: "F1".to_string(),
        })
        .into();
        assert!(matches!(err, FfiError::EncodingGap { .. }));
    }

    #[test]
    fn display_messages_are_human_readable() {
        let err = FfiError::PageIndexOutOfBounds { index: 5 };
        assert_eq!(err.to_string(), "page index 5 out of bounds");
    }
}
