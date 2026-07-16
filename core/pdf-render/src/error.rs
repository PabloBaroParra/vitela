//! Error type for `pdf-render` (T-015..T-021).
//!
//! Kept crate-local because this crate's renderer errors are part of its real
//! API; the Batch 0 shared port error was removed before B7.

use std::error::Error;
use std::fmt;
use std::path::PathBuf;

/// Errors produced by the pdfium actor, the renderer, and the bitmap registry.
#[derive(Debug)]
#[non_exhaustive]
pub enum RenderError {
    /// The pdfium dynamic library could not be loaded from the resolved path.
    LibraryLoad { path: PathBuf, message: String },
    /// The document failed to open (corrupt file, missing file, wrong format).
    OpenDocument(String),
    /// The document is password-protected and the supplied password (or lack
    /// thereof) was rejected by pdfium's security handler.
    InvalidPassword,
    /// The requested page index does not exist in the document.
    PageIndexOutOfBounds(u32),
    /// Rendering failed for a reason surfaced by pdfium itself.
    RenderFailed(String),
    /// The job was cancelled before rasterization began (checked at dequeue —
    /// see `design.md` "Threading — pdfium single-actor model").
    Cancelled,
    /// The pdfium actor's worker thread has already shut down; no further
    /// jobs can be submitted or awaited.
    ActorShutDown,
    /// The referenced document handle is not known to this actor (never
    /// opened, or already closed).
    DocumentNotFound,
    /// The referenced bitmap handle is invalid: either it was never issued by
    /// this registry, or it has already been released (dropped) — see T-019.
    BitmapNotFound,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::LibraryLoad { path, message } => {
                write!(
                    f,
                    "failed to load pdfium library at {path:?}: {message}. \
                     Set PDFIUM_DYNAMIC_LIB_PATH, or populate vendor/pdfium/bin/ \
                     (see vendor/pdfium/README.md)."
                )
            }
            RenderError::OpenDocument(message) => write!(f, "failed to open document: {message}"),
            RenderError::InvalidPassword => write!(f, "invalid or missing password"),
            RenderError::PageIndexOutOfBounds(index) => {
                write!(f, "page index {index} out of bounds")
            }
            RenderError::RenderFailed(message) => write!(f, "render failed: {message}"),
            RenderError::Cancelled => write!(f, "render job cancelled before rasterization"),
            RenderError::ActorShutDown => write!(f, "pdfium actor has shut down"),
            RenderError::DocumentNotFound => write!(f, "document handle not found"),
            RenderError::BitmapNotFound => write!(f, "bitmap handle not found or already released"),
        }
    }
}

impl Error for RenderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_are_human_readable() {
        assert_eq!(
            RenderError::Cancelled.to_string(),
            "render job cancelled before rasterization"
        );
        assert_eq!(
            RenderError::PageIndexOutOfBounds(7).to_string(),
            "page index 7 out of bounds"
        );
        assert_eq!(
            RenderError::BitmapNotFound.to_string(),
            "bitmap handle not found or already released"
        );
    }
}
