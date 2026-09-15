//! Error type for `pdf-compress` (Batch 24, T-190..T-197).
//!
//! Deliberately small today. T-190 owns the never-grow guarantee and nothing
//! that opens a PDF, so the only failure it can honestly name is a
//! caller-contract violation. The structural pass (T-191), the prune (T-192)
//! and the image stage (T-194) each bring a dependency of their own — `lopdf`
//! and `image` — and the variants that wrap *their* failures land with them,
//! not before. The enum is `#[non_exhaustive]` so that arrival is an addition
//! rather than a break.

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
}

impl fmt::Display for CompressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompressError::EmptyInput => write!(f, "there are no bytes to compress"),
        }
    }
}

impl std::error::Error for CompressError {}

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
}
