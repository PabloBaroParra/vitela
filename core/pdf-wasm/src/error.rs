//! What can go wrong across the web boundary, as a closed set of kinds.
//!
//! JavaScript receives every failure as an `Error` whose `name` is one of
//! [`ErrorKind::name`] — the UI branches on that (a password dialog for
//! `PasswordRequired`/`WrongPassword`, a disabled search for
//! `TextExtractionDenied`) and shows `message` as is. The mapping mirrors
//! `pdf-ffi`'s `FfiError` conversions so every shell classifies the same core
//! failure the same way.

use std::fmt;

/// The kind of a failure, stable across releases: the web UI matches on
/// [`ErrorKind::name`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The document is encrypted and was opened without a password.
    PasswordRequired,
    /// The password (or one of the two) does not open the document.
    WrongPassword,
    /// The document's security handler is not one the core can decrypt.
    UnsupportedSecurityHandler,
    /// The document's permissions withhold copying or extracting its text.
    TextExtractionDenied,
    /// No page at that position.
    PageIndexOutOfBounds,
    /// No open document under that id (never opened, or already closed).
    DocumentNotFound,
    /// The bytes are not a PDF the core can read.
    InvalidDocument,
    /// pdfium could not render the page.
    RenderFailed,
    /// pdfium was not bound before the engine was created.
    EngineUnavailable,
    /// The document's permissions withhold this kind of edit (for page
    /// edits, the document-assembly permission).
    EditNotPermitted,
    /// The edit cannot be recorded on this document: the model rejected it
    /// (a zero-page run, a move onto itself), or no writer could save it under the document's encryption.
    UnsupportedOperation,
    /// The save would break a signature the file carries, and the caller did
    /// not say the user was told.
    SignaturesWouldBeInvalidated,
    /// The edited document could not be written.
    SaveFailed,
    /// No annotation of this session answers to that id (removed, undone
    /// away, or never added).
    AnnotationNotFound,
    /// The bytes given for an image stamp are not a PNG or JPEG the core can
    /// decode.
    InvalidImage,
    /// Free text holds a character the box's font (Helvetica, WinAnsi)
    /// cannot show. The message names it; the UI keeps the dialog open.
    EncodingGap,
    /// No form field of the document answers to that id.
    FormFieldNotFound,
}

impl ErrorKind {
    /// The `name` the JavaScript `Error` carries.
    pub fn name(self) -> &'static str {
        match self {
            Self::PasswordRequired => "PasswordRequired",
            Self::WrongPassword => "WrongPassword",
            Self::UnsupportedSecurityHandler => "UnsupportedSecurityHandler",
            Self::TextExtractionDenied => "TextExtractionDenied",
            Self::PageIndexOutOfBounds => "PageIndexOutOfBounds",
            Self::DocumentNotFound => "DocumentNotFound",
            Self::InvalidDocument => "InvalidDocument",
            Self::RenderFailed => "RenderFailed",
            Self::EngineUnavailable => "EngineUnavailable",
            Self::EditNotPermitted => "EditNotPermitted",
            Self::UnsupportedOperation => "UnsupportedOperation",
            Self::SignaturesWouldBeInvalidated => "SignaturesWouldBeInvalidated",
            Self::SaveFailed => "SaveFailed",
            Self::AnnotationNotFound => "AnnotationNotFound",
            Self::InvalidImage => "InvalidImage",
            Self::EncodingGap => "EncodingGap",
            Self::FormFieldNotFound => "FormFieldNotFound",
        }
    }
}

/// A failure with its kind and a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError {
    pub kind: ErrorKind,
    pub message: String,
}

impl EngineError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        EngineError {
            kind,
            message: message.into(),
        }
    }

    pub fn document_not_found(doc: u32) -> Self {
        Self::new(
            ErrorKind::DocumentNotFound,
            format!("no open document with id {doc}"),
        )
    }

    pub fn page_out_of_bounds(page_index: u32) -> Self {
        Self::new(
            ErrorKind::PageIndexOutOfBounds,
            format!("page index {page_index} is out of bounds"),
        )
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.name(), self.message)
    }
}

impl std::error::Error for EngineError {}

impl From<pdf_manip::ManipError> for EngineError {
    fn from(err: pdf_manip::ManipError) -> Self {
        use pdf_manip::ManipError as E;
        let kind = match err {
            E::PasswordRequired => ErrorKind::PasswordRequired,
            E::WrongPassword => ErrorKind::WrongPassword,
            E::UnsupportedSecurityHandler => ErrorKind::UnsupportedSecurityHandler,
            E::InvalidPageNumber(_) | E::InvalidPageIndex(_) => ErrorKind::PageIndexOutOfBounds,
            // What an import source refuses (its copy permission, what a
            // graft cannot carry) is a fact about the PDF the user picked,
            // shown as is — `pdf-ffi`'s `UnsupportedOperation` too.
            E::SourceForbidsCopying
            | E::SourceHasFormFields(_)
            | E::SourceHasXfaForm(_)
            | E::SourceHasOptionalContent(_)
            | E::SourceHasSignature(_) => ErrorKind::UnsupportedOperation,
            // Opening (a document or an import source) is the only other
            // manip call this boundary makes, so any other failure means the
            // bytes could not be read as a PDF.
            _ => ErrorKind::InvalidDocument,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<pdf_save::SaveError> for EngineError {
    /// A save or a preview snapshot that failed. Building the model at open
    /// is not a save and maps to `InvalidDocument` at its call site.
    fn from(err: pdf_save::SaveError) -> Self {
        let kind = match err {
            pdf_save::SaveError::SignaturesWouldBeInvalidated => {
                ErrorKind::SignaturesWouldBeInvalidated
            }
            _ => ErrorKind::SaveFailed,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<pdf_save::EditRefusal> for EngineError {
    /// A permission the document withholds is `EditNotPermitted`; an edit no
    /// writer could save under its encryption is not a permission question,
    /// and is `UnsupportedOperation` (`pdf-ffi` reports both as its
    /// `UnsupportedOperation`, with the same message).
    fn from(refusal: pdf_save::EditRefusal) -> Self {
        let kind = match refusal {
            pdf_save::EditRefusal::FullRewrite(_) => ErrorKind::UnsupportedOperation,
            _ => ErrorKind::EditNotPermitted,
        };
        Self::new(kind, refusal.to_string())
    }
}

impl From<pdf_document::PagePositionOutOfRange> for EngineError {
    /// The position the core named, as `pdf-ffi`'s `PageIndexOutOfBounds`.
    fn from(err: pdf_document::PagePositionOutOfRange) -> Self {
        // A position came from a `u32` on the way in, so it fits on the way
        // out; saturate rather than wrap if that ever stops being true.
        Self::page_out_of_bounds(u32::try_from(err.position).unwrap_or(u32::MAX))
    }
}

impl From<pdf_save::PrepareError> for EngineError {
    /// An import source that could not be prepared: a password it needs
    /// keeps its own kind, so the UI asks for it.
    fn from(err: pdf_save::PrepareError) -> Self {
        match err {
            pdf_save::PrepareError::Open(err) => err.into(),
            refused => Self::new(ErrorKind::UnsupportedOperation, refused.to_string()),
        }
    }
}

impl From<pdf_save::ImportError> for EngineError {
    /// An import the destination refused, by the same kinds as a page edit:
    /// a withheld permission is `EditNotPermitted`, a rewrite the encryption
    /// cannot survive `UnsupportedOperation`, a position past the end
    /// `PageIndexOutOfBounds`.
    fn from(err: pdf_save::ImportError) -> Self {
        use pdf_save::{ImportError as E, ImportRefusal as R};
        let kind = match &err {
            E::Refused(R::ContentEditing | R::DocumentAssembly) => ErrorKind::EditNotPermitted,
            E::IndexOutOfRange { index } => {
                return Self::page_out_of_bounds(u32::try_from(*index).unwrap_or(u32::MAX))
            }
            E::Source(_) => ErrorKind::SaveFailed,
            _ => ErrorKind::UnsupportedOperation,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<pdf_annotate::AnnotateError> for EngineError {
    /// An annotation the core would not build or change, by the kinds
    /// `pdf-ffi` gives the same failures: a missing id, an image that does
    /// not decode and a character Helvetica cannot show keep their own kinds;
    /// a change the kind cannot take and an unusable box are
    /// `UnsupportedOperation`.
    fn from(err: pdf_annotate::AnnotateError) -> Self {
        use pdf_annotate::AnnotateError as E;
        let kind = match err {
            E::AnnotationNotFound(_) => ErrorKind::AnnotationNotFound,
            E::InvalidImage(_) => ErrorKind::InvalidImage,
            E::EncodingGap { .. } => ErrorKind::EncodingGap,
            // `UnsupportedOperation`, `InvalidRect`, and whatever the core
            // adds later (the enum is `#[non_exhaustive]`): a refusal of the
            // request that leaves the document intact.
            _ => ErrorKind::UnsupportedOperation,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<pdf_form::FormError> for EngineError {
    /// A fill the core would not build, by the kinds `pdf-ffi` gives the same
    /// failures: a missing id keeps its own kind; a value the field cannot
    /// hold (the wrong kind, a choice it does not offer, text too long) is a
    /// refusal of the request that leaves the document intact,
    /// `UnsupportedOperation`, with the core's own words.
    fn from(err: pdf_form::FormError) -> Self {
        let kind = match err {
            pdf_form::FormError::FieldNotFound(_) => ErrorKind::FormFieldNotFound,
            // `UnsupportedOperation`, `InvalidValue`, and whatever the core
            // adds later (the enum is `#[non_exhaustive]`).
            _ => ErrorKind::UnsupportedOperation,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<pdf_render::RenderError> for EngineError {
    fn from(err: pdf_render::RenderError) -> Self {
        use pdf_render::RenderError as E;
        let kind = match err {
            E::InvalidPassword => ErrorKind::WrongPassword,
            E::PageIndexOutOfBounds(_) => ErrorKind::PageIndexOutOfBounds,
            E::DocumentNotFound => ErrorKind::DocumentNotFound,
            E::OpenDocument(_) => ErrorKind::InvalidDocument,
            E::LibraryUnavailable(_) | E::LibraryLoad { .. } => ErrorKind::EngineUnavailable,
            _ => ErrorKind::RenderFailed,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<pdf_manip::TextExtractionDenied> for EngineError {
    fn from(err: pdf_manip::TextExtractionDenied) -> Self {
        Self::new(ErrorKind::TextExtractionDenied, err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_failures_keep_their_own_kinds() {
        assert_eq!(
            EngineError::from(pdf_manip::ManipError::PasswordRequired).kind,
            ErrorKind::PasswordRequired
        );
        assert_eq!(
            EngineError::from(pdf_manip::ManipError::WrongPassword).kind,
            ErrorKind::WrongPassword
        );
        assert_eq!(
            EngineError::from(pdf_render::RenderError::InvalidPassword).kind,
            ErrorKind::WrongPassword
        );
    }

    #[test]
    fn unreadable_bytes_are_an_invalid_document() {
        let err =
            pdf_manip::open_document_from_bytes(b"this is not a PDF", None).expect_err("not a PDF");
        assert_eq!(EngineError::from(err).kind, ErrorKind::InvalidDocument);
    }

    #[test]
    fn render_errors_map_like_the_ffi_boundary() {
        use pdf_render::RenderError as E;
        let kind = |err| EngineError::from(err).kind;
        assert_eq!(
            kind(E::PageIndexOutOfBounds(3)),
            ErrorKind::PageIndexOutOfBounds
        );
        assert_eq!(kind(E::DocumentNotFound), ErrorKind::DocumentNotFound);
        assert_eq!(
            kind(E::LibraryUnavailable("unbound".into())),
            ErrorKind::EngineUnavailable
        );
        assert_eq!(kind(E::RenderFailed("x".into())), ErrorKind::RenderFailed);
    }

    #[test]
    fn a_permission_refusal_is_edit_not_permitted() {
        use pdf_save::EditRefusal as R;
        for refusal in [
            R::AnnotationEditing,
            R::DocumentAssembly,
            R::FormFieldEditing,
            R::ContentEditing,
        ] {
            let err = EngineError::from(refusal);
            assert_eq!(err.kind, ErrorKind::EditNotPermitted, "{refusal:?}");
            assert_eq!(err.message, refusal.to_string());
        }
        assert_eq!(ErrorKind::EditNotPermitted.name(), "EditNotPermitted");
    }

    #[test]
    fn an_edit_no_writer_could_save_is_unsupported_not_denied() {
        let refusal =
            pdf_save::EditRefusal::FullRewrite(pdf_save::RewriteBlocker::UnsupportedHandler);
        assert_eq!(
            EngineError::from(refusal).kind,
            ErrorKind::UnsupportedOperation
        );
    }

    #[test]
    fn an_out_of_range_page_position_is_page_index_out_of_bounds() {
        let err = EngineError::from(pdf_document::PagePositionOutOfRange { position: 7 });
        assert_eq!(err.kind, ErrorKind::PageIndexOutOfBounds);
        assert_eq!(err.message, "page index 7 is out of bounds");
    }

    #[test]
    fn save_failures_keep_the_signature_refusal_apart() {
        assert_eq!(
            EngineError::from(pdf_save::SaveError::SignaturesWouldBeInvalidated).kind,
            ErrorKind::SignaturesWouldBeInvalidated
        );
        assert_eq!(
            EngineError::from(pdf_save::SaveError::InvalidSaveRequest("x")).kind,
            ErrorKind::SaveFailed
        );
    }

    #[test]
    fn what_an_import_source_refuses_is_unsupported_not_invalid() {
        use pdf_manip::ManipError as E;
        for err in [
            E::SourceForbidsCopying,
            E::SourceHasSignature(0),
            E::SourceHasXfaForm(0),
        ] {
            let message = err.to_string();
            let mapped = EngineError::from(pdf_save::PrepareError::Open(err));
            assert_eq!(mapped.kind, ErrorKind::UnsupportedOperation);
            assert_eq!(mapped.message, message);
        }
        assert_eq!(
            EngineError::from(pdf_save::PrepareError::Open(E::PasswordRequired)).kind,
            ErrorKind::PasswordRequired
        );
        let none = EngineError::from(pdf_save::PrepareError::NoPages);
        assert_eq!(none.kind, ErrorKind::UnsupportedOperation);
        assert_eq!(none.message, "the PDF has no pages");
    }

    #[test]
    fn an_import_refusal_maps_like_a_page_edit_refusal() {
        use pdf_save::{ImportError as E, ImportRefusal as R};
        let kind = |err| EngineError::from(err).kind;
        assert_eq!(
            kind(E::Refused(R::DocumentAssembly)),
            ErrorKind::EditNotPermitted
        );
        assert_eq!(
            kind(E::Refused(R::ContentEditing)),
            ErrorKind::EditNotPermitted
        );
        assert_eq!(
            kind(E::Refused(R::FullRewrite(
                pdf_save::RewriteBlocker::IncompleteCredentials
            ))),
            ErrorKind::UnsupportedOperation
        );
        assert_eq!(
            kind(E::IndexOutOfRange { index: 9 }),
            ErrorKind::PageIndexOutOfBounds
        );
        assert_eq!(kind(E::AlreadyImported), ErrorKind::UnsupportedOperation);
        assert_eq!(kind(E::NothingToImport), ErrorKind::UnsupportedOperation);
    }

    #[test]
    fn text_refusal_is_its_own_kind() {
        let err = EngineError::from(pdf_manip::TextExtractionDenied);
        assert_eq!(err.kind, ErrorKind::TextExtractionDenied);
        assert_eq!(err.kind.name(), "TextExtractionDenied");
    }

    #[test]
    fn annotation_failures_map_like_the_ffi_boundary() {
        use pdf_annotate::AnnotateError as E;
        let kind = |err| EngineError::from(err).kind;
        assert_eq!(
            kind(E::AnnotationNotFound(pdf_document::AnnotationId(4))),
            ErrorKind::AnnotationNotFound
        );
        assert_eq!(kind(E::InvalidImage("x".into())), ErrorKind::InvalidImage);
        assert_eq!(
            kind(E::EncodingGap { character: '日' }),
            ErrorKind::EncodingGap
        );
        assert_eq!(kind(E::InvalidRect), ErrorKind::UnsupportedOperation);
        assert_eq!(
            kind(E::UnsupportedOperation("restyle")),
            ErrorKind::UnsupportedOperation
        );
        let gap = EngineError::from(E::EncodingGap { character: '日' });
        assert!(gap.message.contains('日'), "{}", gap.message);
        assert_eq!(ErrorKind::EncodingGap.name(), "EncodingGap");
        assert_eq!(ErrorKind::AnnotationNotFound.name(), "AnnotationNotFound");
        assert_eq!(ErrorKind::InvalidImage.name(), "InvalidImage");
    }

    #[test]
    fn form_failures_map_like_the_ffi_boundary() {
        use pdf_form::FormError as E;
        let missing = EngineError::from(E::FieldNotFound(pdf_document::FormFieldId(7)));
        assert_eq!(missing.kind, ErrorKind::FormFieldNotFound);
        assert_eq!(missing.message, "no form field with id 7");
        assert_eq!(ErrorKind::FormFieldNotFound.name(), "FormFieldNotFound");
        for refusal in [
            E::UnsupportedOperation("set_value"),
            E::InvalidValue("not an option".into()),
        ] {
            let message = refusal.to_string();
            let mapped = EngineError::from(refusal);
            assert_eq!(mapped.kind, ErrorKind::UnsupportedOperation);
            assert_eq!(mapped.message, message);
        }
    }
}
