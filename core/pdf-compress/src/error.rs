//! Error type for `pdf-compress` (Batch 24, T-190..T-197).
//!
//! Small on purpose, and it grows one stage at a time. T-190 opened no PDF at
//! all, so the only failure it could honestly name was a caller-contract
//! violation. T-191 brought `lopdf` in, and with it the two ways a repack can
//! fail: the input is not a document that can be read, or the repacked
//! document cannot be written out. The prune (T-192) adds nothing new; the
//! image stage (T-194) brings `image` and its own decode failures. The enum is
//! `#[non_exhaustive]` so each arrival is an addition rather than a break.
//!
//! What is deliberately *not* an error: a document that cannot be improved, or
//! one this crate declines to touch. Those come back as a [`Refusal`] riding a
//! successful result — see [`crate::report`] for why a refusal is reported
//! rather than thrown.
//!
//! [`Refusal`]: crate::report::Refusal

use std::fmt;

/// Errors produced by `pdf-compress`.
#[derive(Debug)]
#[non_exhaustive]
pub enum CompressError {
    /// [`crate::compress`] was handed an empty buffer.
    ///
    /// There is no document to compress, and answering "0 bytes in, 0 bytes
    /// out, no gain" would report a successful no-op for a call that cannot
    /// have one.
    EmptyInput,
    /// The input could not be read as a PDF, or the repacked document could
    /// not be serialised. `lopdf` types do not leak past this boundary as a
    /// public contract — the same wrapping `pdf_manip::ManipError` and
    /// `pdf_save::SaveError` already use.
    Lopdf(lopdf::Error),
    /// Writing the repacked document failed. In practice this is the
    /// serialiser's own error channel rather than a disk: compression writes
    /// into a `Vec`, and the caller decides where the bytes land.
    Io(std::io::Error),
}

impl fmt::Display for CompressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompressError::EmptyInput => write!(f, "there are no bytes to compress"),
            CompressError::Lopdf(err) => write!(f, "lopdf error: {err}"),
            CompressError::Io(err) => write!(f, "could not write the compressed document: {err}"),
        }
    }
}

impl std::error::Error for CompressError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CompressError::EmptyInput => None,
            CompressError::Lopdf(err) => Some(err),
            CompressError::Io(err) => Some(err),
        }
    }
}

impl From<lopdf::Error> for CompressError {
    fn from(err: lopdf::Error) -> Self {
        CompressError::Lopdf(err)
    }
}

impl From<std::io::Error> for CompressError {
    fn from(err: std::io::Error) -> Self {
        CompressError::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_input_says_so_in_words() {
        assert_eq!(
            CompressError::EmptyInput.to_string(),
            "there are no bytes to compress"
        );
    }

    #[test]
    fn an_unreadable_document_carries_lopdfs_own_words() {
        let err: CompressError = lopdf::Document::load_mem(b"not a document")
            .expect_err("that is not a document")
            .into();

        assert!(err.to_string().starts_with("lopdf error: "));
    }
}
