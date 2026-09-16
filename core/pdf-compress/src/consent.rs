//! What the caller has taken responsibility for.
//!
//! ## The rule this module exists to keep
//!
//! *This crate never breaks something irreplaceable on its own authority, and
//! never refuses on behalf of a caller who has already decided.*
//!
//! Both halves matter. [`crate::session`] hands a signed document straight
//! back because compressing it rewrites every byte offset its `/ByteRange`
//! covers — measured, not reasoned: repacking
//! `tests/fixtures/signed/rsa2048_sha256.pdf` produced a file 92% smaller
//! with its page perfectly intact, and the 92% *was* the signature. That is
//! the right default for a caller who has not thought about it.
//!
//! It is the wrong answer for one that has. A shell whose user was shown the
//! warning and pressed "compress anyway" is not helped by a crate that
//! silently declines and reports `NoGain` — the user said yes, and a refusal
//! at this depth reads as a bug. So the decision arrives as a value
//! (`docs/batch-compress.md` decision 7): the policy is `pdf-save`'s, this
//! crate only obeys it.
//!
//! There is deliberately **no** such value for encryption. That is not a
//! decision anyone is allowed to make — an encrypted document loads with no
//! objects in it at all, so "compress anyway" would write out an empty file.
//! See [`crate::session`] for the measurement, and `pdf-save`'s `compress`
//! module for why having the password does not change the answer either.

/// What to do with a document that already carries a signature.
///
/// [`SignedDocuments::LeaveAlone`] is the default, and the only value a
/// caller reaches by not thinking: compression is refused and the reason
/// travels back in the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignedDocuments {
    /// Hand them back byte for byte, carrying
    /// [`Refusal::SignaturesWouldBeInvalidated`](crate::Refusal::SignaturesWouldBeInvalidated).
    #[default]
    LeaveAlone,
    /// Compress them like any other document. The caller is stating that the
    /// person whose signature it is has been told it will stop verifying —
    /// `pdf_save::SignatureAcknowledgement::ProceedAndInvalidate` is the
    /// value that says so on the other side of the boundary.
    CompressAnyway,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The safe answer is the one a caller gets for free. If this ever
    /// flips, every `..Default::default()` in the workspace starts breaking
    /// signatures quietly.
    #[test]
    fn not_deciding_means_leaving_signed_documents_alone() {
        assert_eq!(SignedDocuments::default(), SignedDocuments::LeaveAlone);
    }
}
