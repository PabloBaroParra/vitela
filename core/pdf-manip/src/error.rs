//! Error type for `pdf-manip`'s public API (T-022..T-026).
//!
//! Wraps `lopdf::Error` for I/O/parse/encryption failures and adds
//! `pdf-manip`-specific variants for invalid page references and the
//! decrypt-on-open credential flow (spec.md "Open Password-Protected PDF").

use std::fmt;

/// Errors returned by `pdf-manip`'s manipulation and open operations.
///
/// `lopdf` types never leak past this boundary as a *public API contract*:
/// this enum wraps `lopdf::Error` for pass-through failures, but every
/// caller-facing function returns `ManipError`, never a bare `lopdf::Error`.
#[derive(Debug)]
#[non_exhaustive]
pub enum ManipError {
    /// Underlying lopdf failure (parse, I/O, structural) not otherwise
    /// classified below.
    Lopdf(lopdf::Error),
    /// The document is encrypted but no credential was supplied to open it.
    PasswordRequired,
    /// The supplied password matched neither the user nor owner password.
    WrongPassword,
    /// The document's `/Encrypt` dictionary uses a security handler this
    /// crate doesn't (yet) recognize (only RC4-128 and AES-128 are mapped to
    /// `pdf_document::SecurityHandler` as of Batch 4).
    UnsupportedSecurityHandler,
    /// `merge` was called with zero documents.
    EmptyMerge,
    /// `extract_pages` was called with an empty page selection.
    EmptyPageSelection,
    /// A 1-indexed page number was zero or beyond the document's page count.
    InvalidPageNumber(u32),
    /// A 0-indexed page/insertion index was beyond the document's bounds.
    InvalidPageIndex(usize),
    /// `split`'s `after_page` boundary would leave one side with zero pages.
    InvalidPageRange { after_page: u32, total_pages: u32 },
    /// `reorder_pages` was not given a permutation of every existing page
    /// number (wrong length, duplicate, or out-of-range entry).
    InvalidPageOrder,
    /// A page object's `/Parent` reference could not be resolved to a page
    /// tree dictionary (malformed or unsupported nested page tree shape).
    MalformedPageTree,
}

impl fmt::Display for ManipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManipError::Lopdf(err) => write!(f, "lopdf error: {err}"),
            ManipError::PasswordRequired => {
                write!(f, "document is encrypted; a password is required to open it")
            }
            ManipError::WrongPassword => {
                write!(f, "the supplied password is not valid for this document")
            }
            ManipError::UnsupportedSecurityHandler => {
                write!(f, "document uses an unsupported security handler")
            }
            ManipError::EmptyMerge => write!(f, "cannot merge zero documents"),
            ManipError::EmptyPageSelection => write!(f, "page selection must not be empty"),
            ManipError::InvalidPageNumber(n) => write!(f, "invalid page number: {n}"),
            ManipError::InvalidPageIndex(i) => write!(f, "invalid page index: {i}"),
            ManipError::InvalidPageRange {
                after_page,
                total_pages,
            } => write!(
                f,
                "split boundary after page {after_page} is invalid for a {total_pages}-page document"
            ),
            ManipError::InvalidPageOrder => write!(
                f,
                "reorder input must be a permutation of every existing page number"
            ),
            ManipError::MalformedPageTree => {
                write!(f, "page object's /Parent could not be resolved")
            }
        }
    }
}

impl std::error::Error for ManipError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ManipError::Lopdf(err) => Some(err),
            _ => None,
        }
    }
}

impl From<lopdf::Error> for ManipError {
    fn from(err: lopdf::Error) -> Self {
        ManipError::Lopdf(err)
    }
}
