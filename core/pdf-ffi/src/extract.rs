//! Extract: pulling a subset of the open document's pages out into a new
//! PDF, across the UniFFI boundary.
//!
//! The FFI twin of the Linux shell's `write::extract` chain
//! (`apps/linux-gtk/src/app/write/extract/`). That chain links
//! `pdf_document`/`pdf_manip`/`pdf_save` directly and does its own
//! destination-choosing and worker-thread dance; a shell on the other side of
//! this boundary (Windows today) has none of that available, so this module
//! carries across exactly the two things it cannot get any other way: the
//! cut itself (`pdf_document::prune::prune_to`, shared with the Linux chain
//! rather than a second copy of it) and the gates that decide whether it may
//! run at all.
//!
//! ## The permission gate
//!
//! `/P` bit 5 — the same bit `export::export_page_image` and
//! `pdf_manip::open_import_source_from_bytes` already gate on, and the one
//! the Linux `write::extract` module's own header explains at length: PDF 1.7
//! table 22 names it "copy or otherwise extract text **and graphics** from
//! the document", and pulling a page's content into a second file is exactly
//! that.
//!
//! Deliberately not the page-assembly bit. That one governs changing *this*
//! document's own pages; an extraction changes nothing about the document
//! that was opened.
//!
//! A second gate follows for a different reason: a new file with a different
//! page set can only come from a full rewrite, and an encrypted document
//! opened with only one of its two passwords can never be re-encrypted. That
//! is [`DocumentState::full_rewrite_blocker`], asked here for the same reason
//! the Linux chain asks it before opening its destination dialog — so a shell
//! can refuse up front instead of discovering it after the caller has done
//! the work of picking somewhere to write.
//!
//! ## The live handle is never mutated
//!
//! [`extract_pages_to_pdf`] prunes a **clone** of the document
//! ([`DocumentState::document`] is a shared reference; pruning needs
//! `&mut`), exactly as the Linux chain's `ExtractRequest` clones the
//! session's model before pruning it on a worker thread. Nothing this module
//! does can reach the handle's own pending-edit log or undo history.

use pdf_document::prune::prune_to;
use pdf_document::Document;

use crate::document::DocumentHandle;
use crate::error::FfiError;
use crate::types::{FfiSaveIntent, FfiSignatureAcknowledgement};

/// Prunes a clone of `handle`'s document down to `pages` and returns the
/// bytes of the resulting PDF. The live handle is untouched.
///
/// `pages` must be zero-based, ascending and deduplicated positions — the
/// shape [`crate::parse_page_selection`] already produces from a typed range,
/// and the same contract `pdf_document::prune::removal_runs` documents.
///
/// `SaveIntent::Default` and not a strip: an extraction from a protected
/// document produces a protected file. The permission that let it happen at
/// all is `/P` bit 5 (see the module docs), which is a permission to lift
/// content *out*, never a permission to hand it on unprotected.
///
/// `SignatureAcknowledgement::ProceedAndInvalidate` rather than a refusal a
/// caller has to acknowledge: there is nothing here for a caller to consent
/// to losing, because nothing the user is looking at is being saved over. A
/// signed source's extracted file simply carries a signature that no longer
/// verifies — see [`extract_source_is_signed`], which is how a shell learns
/// to say so in its own summary, mirroring the Linux chain's
/// `write::extract::options::extract_summary`.
///
/// # Errors
///
/// - [`FfiError::InvalidPageSelection`] for an empty selection — the same
///   variant a bad typed range comes back as, since both mean "there is
///   nothing to extract".
/// - [`FfiError::PageIndexOutOfBounds`] for a position the document does not
///   have.
/// - [`FfiError::UnsupportedOperation`] when `/P` bit 5 withholds extraction,
///   or when the source cannot survive a full rewrite (an encrypted document
///   opened with only one of its two passwords).
#[uniffi::export]
pub fn extract_pages_to_pdf(handle: &DocumentHandle, pages: Vec<u32>) -> Result<Vec<u8>, FfiError> {
    if pages.is_empty() {
        return Err(FfiError::InvalidPageSelection {
            detail: "no pages were selected to extract".to_string(),
        });
    }
    if pages.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(FfiError::InvalidPageSelection {
            detail: "pages to extract must be ascending and unique".to_string(),
        });
    }

    let state = handle.lock();
    if !state.text_extraction_allowed() {
        return Err(FfiError::UnsupportedOperation {
            detail: "this document does not permit extracting its pages".to_string(),
        });
    }
    if let Some(blocker) = state.full_rewrite_blocker() {
        return Err(FfiError::UnsupportedOperation {
            detail: rewrite_refusal(blocker).to_string(),
        });
    }

    let mut document: Document = state.document().clone();
    let total = document.pages.len() as u32;
    if let Some(&out_of_bounds) = pages.iter().find(|&&page| page >= total) {
        return Err(FfiError::PageIndexOutOfBounds {
            index: out_of_bounds,
        });
    }

    prune_to(&mut document, &pages).map_err(|detail| FfiError::UnsupportedOperation { detail })?;

    state
        .with_save_input_for(
            &document,
            FfiSaveIntent::Default,
            FfiSignatureAcknowledgement::ProceedAndInvalidate,
            pdf_save::save_document,
        )
        .map_err(Into::into)
}

/// Whether the document `handle` was opened from carries a digital signature
/// — the one fact a shell's extraction summary needs beyond the page count,
/// the same note the Linux chain's `extract_summary` adds when it applies.
#[uniffi::export]
pub fn extract_source_is_signed(handle: &DocumentHandle) -> bool {
    handle.lock().has_signatures()
}

/// A shell-neutral rendering of [`pdf_save::RewriteBlocker`].
///
/// `RewriteBlocker::reason()` exists, but it is written for a developer
/// reading a log (see its own doc comment); this is written for the person a
/// shell is about to show it to, the same split the Linux shell's own
/// `state::rewrite_refusal` makes for its Organize screen. Kept here rather
/// than exported as its own FFI function because nothing about it is
/// extract-specific yet — the day another FFI save gate needs it, it can move
/// up to `error` or `document` without extract's callers noticing.
fn rewrite_refusal(blocker: pdf_save::RewriteBlocker) -> &'static str {
    match blocker {
        pdf_save::RewriteBlocker::IncompleteCredentials => {
            "Extracting pages rewrites the whole file, which needs both this document's user \
             and owner passwords. Reopen it with both to continue."
        }
        _ => {
            "This document's encryption cannot yet be reproduced when the file is rewritten, \
             so its pages cannot be extracted."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_incomplete_credentials_blocker_names_both_passwords() {
        let message = rewrite_refusal(pdf_save::RewriteBlocker::IncompleteCredentials);
        assert!(message.contains("user"));
        assert!(message.contains("owner"));
    }

    #[test]
    fn an_unsupported_handler_blocker_names_no_password_at_all() {
        let message = rewrite_refusal(pdf_save::RewriteBlocker::UnsupportedHandler);
        assert!(!message.contains("password"));
    }
}
