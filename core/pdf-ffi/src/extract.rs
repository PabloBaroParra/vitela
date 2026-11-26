//! Extract: pulling a subset of the open document's pages out into a new
//! PDF, across the UniFFI boundary.
//!
//! The FFI twin of the Linux shell's `write::extract` chain
//! (`apps/linux-gtk/src/app/write/extract/`). That chain links
//! `pdf_document`/`pdf_manip`/`pdf_save` directly and does its own
//! destination-choosing and worker-thread dance; a shell on the other side of
//! this boundary (Windows today) has none of that available, so this module
//! carries across exactly the two things it cannot get any other way: the
//! cut itself and the gates that decide whether it may run at all.
//!
//! Both live in [`pdf_save::extract`] (T-249), shared with every other
//! boundary; this module only hands it the handle's snapshot and maps its
//! refusals onto [`FfiError`]. See that module for why the gate is `/P`
//! bit 5 and not the page-assembly bit, and why a source that cannot survive
//! a full rewrite is refused up front.
//!
//! ## The live handle is never mutated
//!
//! [`extract_pages_to_pdf`] hands `pdf-save` the live model, which prunes a
//! **clone** of it, exactly as the Linux chain's `ExtractRequest` clones the
//! session's model before pruning it on a worker thread. Nothing this module
//! does can reach the handle's own pending-edit log or undo history.

use crate::document::DocumentHandle;
use crate::error::FfiError;

/// Prunes a clone of `handle`'s document down to `pages` and returns the
/// bytes of the resulting PDF. The live handle is untouched.
///
/// `pages` must be zero-based, ascending and deduplicated positions — the
/// shape [`crate::parse_page_selection`] already produces from a typed range,
/// and the same contract `pdf_document::prune::removal_runs` documents.
///
/// An extraction from a protected document produces a protected file, and a
/// signed source's extracted file simply carries a signature that no longer
/// verifies — see [`pdf_save::extracted_pages`] and
/// [`extract_source_is_signed`], which is how a shell learns to say so in
/// its own summary, mirroring the Linux chain's
/// `write::extract::options::extract_summary`.
///
/// # Errors
///
/// - [`FfiError::InvalidPageSelection`] for an empty selection — the same
///   variant a bad typed range comes back as, since both mean "there is
///   nothing to extract" — or one that is not ascending and unique.
/// - [`FfiError::PageIndexOutOfBounds`] for a position the document does not
///   have.
/// - [`FfiError::UnsupportedOperation`] when `/P` bit 5 withholds extraction,
///   or when the source cannot survive a full rewrite (an encrypted document
///   opened with only one of its two passwords).
#[uniffi::export]
pub fn extract_pages_to_pdf(handle: &DocumentHandle, pages: Vec<u32>) -> Result<Vec<u8>, FfiError> {
    handle
        .lock()
        .with_save_source(|source| pdf_save::extracted_pages(source, &pages))
        .map_err(Into::into)
}

/// Whether the document `handle` was opened from carries a digital signature
/// — the one fact a shell's extraction summary needs beyond the page count,
/// the same note the Linux chain's `extract_summary` adds when it applies.
#[uniffi::export]
pub fn extract_source_is_signed(handle: &DocumentHandle) -> bool {
    handle.lock().has_signatures()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal text shells show is `pdf-save`'s, carried across intact.
    #[test]
    fn a_rewrite_refusal_keeps_its_user_facing_text() {
        let error: FfiError =
            pdf_save::ExtractError::RewriteBlocked(pdf_save::RewriteBlocker::IncompleteCredentials)
                .into();

        match error {
            FfiError::UnsupportedOperation { detail } => {
                assert!(detail.contains("user"));
                assert!(detail.contains("owner"));
            }
            other => panic!("expected UnsupportedOperation, got {other:?}"),
        }
    }

    #[test]
    fn selection_refusals_stay_invalid_page_selections() {
        for refusal in [
            pdf_save::ExtractError::NoPages,
            pdf_save::ExtractError::NotAscending,
        ] {
            let message = refusal.to_string();
            match FfiError::from(refusal) {
                FfiError::InvalidPageSelection { detail } => assert_eq!(detail, message),
                other => panic!("expected InvalidPageSelection, got {other:?}"),
            }
        }
    }

    #[test]
    fn an_out_of_bounds_position_stays_typed() {
        assert!(matches!(
            FfiError::from(pdf_save::ExtractError::PageOutOfBounds(7)),
            FfiError::PageIndexOutOfBounds { index: 7 }
        ));
    }
}
