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
            // Opening is the only manip call this boundary makes, so any
            // other failure means the bytes could not be read as a PDF.
            _ => ErrorKind::InvalidDocument,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<pdf_save::SaveError> for EngineError {
    /// Only `document_from_lopdf` reaches here: building the model of a
    /// document lopdf has already parsed.
    fn from(err: pdf_save::SaveError) -> Self {
        Self::new(ErrorKind::InvalidDocument, err.to_string())
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
    fn text_refusal_is_its_own_kind() {
        let err = EngineError::from(pdf_manip::TextExtractionDenied);
        assert_eq!(err.kind, ErrorKind::TextExtractionDenied);
        assert_eq!(err.kind.name(), "TextExtractionDenied");
    }
}
