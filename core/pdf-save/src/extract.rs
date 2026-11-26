//! Extract (T-249): pulling a subset of a document's pages out into a new
//! PDF — the gates that decide whether it may run, the cut, and the save.
//!
//! Moved here from `pdf-ffi` so every boundary that cannot link the Linux
//! shell's `write::extract` chain (UniFFI today, wasm next) runs the same
//! orchestration. The cut itself is `pdf_document::prune::prune_to`, shared
//! with that chain rather than a second copy of it.
//!
//! ## The permission gate
//!
//! `/P` bit 5 — PDF 1.7 table 22 names it "copy or otherwise extract text
//! **and graphics** from the document", and pulling a page's content into a
//! second file is exactly that. Deliberately not the page-assembly bit: that
//! one governs changing *this* document's own pages, and an extraction
//! changes nothing about the document that was opened.
//!
//! A second gate follows for a different reason: a new file with a different
//! page set can only come from a full rewrite, and an encrypted document
//! opened with only one of its two passwords can never be re-encrypted
//! ([`crate::full_rewrite_blocker`]).
//!
//! ## The live model is never mutated
//!
//! [`extracted_pages`] prunes a **clone** of the source document, so nothing
//! here can reach the caller's own pending-edit log or undo history.

use std::fmt;

use pdf_document::prune::prune_to;
use pdf_document::Document;

use crate::error::SaveError;
use crate::rewrite::{full_rewrite_blocker, RewriteBlocker};
use crate::security::SaveIntent;
use crate::strategy::{self, SaveSource, SignatureAcknowledgement};

/// Why [`extracted_pages`] produced no file. The [`fmt::Display`] text of
/// every variant but [`Self::Save`] is written for the person a shell shows
/// it to.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExtractError {
    /// The selection was empty — nothing to extract.
    NoPages,
    /// The selection was not strictly ascending (out of order or repeated).
    NotAscending,
    /// `/P` bit 5 withholds extraction.
    ExtractionDenied,
    /// The source cannot survive the full rewrite an extraction needs.
    RewriteBlocked(RewriteBlocker),
    /// A selected zero-based position the document does not have — the
    /// first such one in the selection.
    PageOutOfBounds(u32),
    /// `prune_to` refused the cut.
    Prune(String),
    /// The save of the pruned clone failed.
    Save(SaveError),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPages => f.write_str("no pages were selected to extract"),
            Self::NotAscending => f.write_str("pages to extract must be ascending and unique"),
            Self::ExtractionDenied => {
                f.write_str("this document does not permit extracting its pages")
            }
            Self::RewriteBlocked(blocker) => f.write_str(rewrite_refusal(*blocker)),
            Self::PageOutOfBounds(index) => {
                write!(f, "page index {index} is out of bounds")
            }
            Self::Prune(detail) => f.write_str(detail),
            Self::Save(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ExtractError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Save(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SaveError> for ExtractError {
    fn from(error: SaveError) -> Self {
        Self::Save(error)
    }
}

/// Prunes a clone of `source.document` down to `pages` and returns the bytes
/// of the resulting PDF. The source is untouched.
///
/// `pages` must be zero-based, ascending and deduplicated positions — the
/// same contract `pdf_document::prune::removal_runs` documents.
///
/// `SaveIntent::Default` and not a strip: an extraction from a protected
/// document produces a protected file. The permission that let it happen at
/// all is `/P` bit 5, a permission to lift content *out*, never one to hand
/// it on unprotected.
///
/// `SignatureAcknowledgement::ProceedAndInvalidate` rather than a refusal a
/// caller has to acknowledge: nothing the user is looking at is being saved
/// over. A signed source's extracted file simply carries a signature that no
/// longer verifies.
///
/// # Errors
///
/// The gates run in this order: [`ExtractError::NoPages`],
/// [`ExtractError::NotAscending`], [`ExtractError::ExtractionDenied`],
/// [`ExtractError::RewriteBlocked`], [`ExtractError::PageOutOfBounds`]; then
/// [`ExtractError::Prune`] or [`ExtractError::Save`] from the work itself.
pub fn extracted_pages(source: SaveSource<'_>, pages: &[u32]) -> Result<Vec<u8>, ExtractError> {
    if pages.is_empty() {
        return Err(ExtractError::NoPages);
    }
    if pages.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(ExtractError::NotAscending);
    }

    if let Some(refusal) = extraction_refusal(source.document) {
        return Err(refusal);
    }

    let total = source.document.pages.len() as u32;
    if let Some(&out_of_bounds) = pages.iter().find(|&&page| page >= total) {
        return Err(ExtractError::PageOutOfBounds(out_of_bounds));
    }

    let mut document = source.document.clone();
    prune_to(&mut document, pages).map_err(ExtractError::Prune)?;

    strategy::save_document(source.input_for(
        &document,
        SaveIntent::Default,
        SignatureAcknowledgement::ProceedAndInvalidate,
    ))
    .map_err(Into::into)
}

/// The two gates of an extraction that depend on the document alone, not on
/// the selection: [`ExtractError::ExtractionDenied`], then
/// [`ExtractError::RewriteBlocked`]. `None` when pages may be extracted.
///
/// What a shell asks before it offers Extract or Split at all, so the
/// refusal is said up front in the core's words; [`extracted_pages`] asks the
/// same two, in the same order, before it cuts anything. Cheap: it reads the
/// document's security, nothing else.
pub fn extraction_refusal(document: &Document) -> Option<ExtractError> {
    let security = document.security.as_ref();
    if !pdf_manip::text_extraction_is_allowed(security) {
        return Some(ExtractError::ExtractionDenied);
    }
    full_rewrite_blocker(security).map(ExtractError::RewriteBlocked)
}

/// A shell-neutral rendering of [`RewriteBlocker`].
///
/// `RewriteBlocker::reason()` exists, but it is written for a developer
/// reading a log; this is written for the person a shell is about to show
/// it to, the same split the Linux shell's own `state::rewrite_refusal`
/// makes for its Organize screen.
fn rewrite_refusal(blocker: RewriteBlocker) -> &'static str {
    match blocker {
        RewriteBlocker::IncompleteCredentials => {
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
    use crate::bridge::{document_from_lopdf, ImportedSources};
    use pdf_document::{
        Credential, Document, EncryptionCredentials, Permissions, SecurityContext, SecurityHandler,
    };
    use pdf_manip::LopdfDocument;

    struct Fixture {
        document: Document,
        base: LopdfDocument,
        bytes: Vec<u8>,
    }

    impl Fixture {
        fn source(&self) -> SaveSource<'_> {
            SaveSource {
                document: &self.document,
                base: &self.base,
                original_bytes: Some(&self.bytes),
                imported_sources: ImportedSources::none(),
            }
        }
    }

    fn fixture(pages: u32) -> Fixture {
        let mut doc = gen_fixtures::build_multi_page_document(pages, "extract");
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save fixture");
        let (base, security) =
            pdf_manip::open_document_from_bytes(&bytes, None).expect("open fixture");
        let document = document_from_lopdf(&base, security).expect("model");
        Fixture {
            document,
            base,
            bytes,
        }
    }

    /// A security context on the model alone is enough for the gates, which
    /// read nothing else. Opened with the user password: an owner open
    /// bypasses `/P` entirely.
    fn with_security(fixture: &mut Fixture, permissions: u32, credentials: EncryptionCredentials) {
        fixture.document.security = Some(SecurityContext {
            handler: SecurityHandler::Aes128,
            credential: Credential::User,
            credentials,
            permissions: Permissions(permissions),
        });
    }

    fn page_count(bytes: &[u8]) -> usize {
        lopdf::Document::load_mem(bytes)
            .expect("output must reload")
            .get_pages()
            .len()
    }

    #[test]
    fn extracting_keeps_only_the_selected_pages() {
        let fixture = fixture(4);

        let bytes = extracted_pages(fixture.source(), &[0, 3]).expect("extraction");

        assert_eq!(page_count(&bytes), 2);
        assert_eq!(fixture.document.pages.len(), 4, "the source is untouched");
    }

    #[test]
    fn an_empty_selection_is_refused() {
        let fixture = fixture(2);

        assert!(matches!(
            extracted_pages(fixture.source(), &[]),
            Err(ExtractError::NoPages)
        ));
    }

    #[test]
    fn an_unordered_or_repeated_selection_is_refused() {
        let fixture = fixture(3);

        for pages in [&[2, 0][..], &[0, 0][..]] {
            assert!(matches!(
                extracted_pages(fixture.source(), pages),
                Err(ExtractError::NotAscending)
            ));
        }
    }

    #[test]
    fn a_position_past_the_last_page_is_refused() {
        let fixture = fixture(2);

        assert!(matches!(
            extracted_pages(fixture.source(), &[0, 5]),
            Err(ExtractError::PageOutOfBounds(5))
        ));
    }

    #[test]
    fn a_document_withholding_extraction_is_refused() {
        let mut fixture = fixture(2);
        // Every bit but bit 5 (copy/extract, 0x10).
        with_security(
            &mut fixture,
            0xFFFF_FFFC & !0x10,
            EncryptionCredentials::both("u", "o"),
        );

        assert!(matches!(
            extracted_pages(fixture.source(), &[0]),
            Err(ExtractError::ExtractionDenied)
        ));
    }

    #[test]
    fn a_document_opened_with_one_password_is_refused() {
        let mut fixture = fixture(2);
        with_security(&mut fixture, 0xFFFF_FFFC, EncryptionCredentials::user("u"));

        assert!(matches!(
            extracted_pages(fixture.source(), &[0]),
            Err(ExtractError::RewriteBlocked(
                RewriteBlocker::IncompleteCredentials
            ))
        ));
    }

    #[test]
    fn the_document_gates_are_asked_without_a_selection() {
        let mut fixture = fixture(2);
        assert!(extraction_refusal(&fixture.document).is_none());

        with_security(&mut fixture, 0xFFFF_FFFC, EncryptionCredentials::user("u"));
        assert!(matches!(
            extraction_refusal(&fixture.document),
            Some(ExtractError::RewriteBlocked(
                RewriteBlocker::IncompleteCredentials
            ))
        ));

        // The permission is asked first, as `extracted_pages` asks it.
        with_security(
            &mut fixture,
            0xFFFF_FFFC & !0x10,
            EncryptionCredentials::user("u"),
        );
        assert!(matches!(
            extraction_refusal(&fixture.document),
            Some(ExtractError::ExtractionDenied)
        ));
    }

    #[test]
    fn an_incomplete_credentials_blocker_names_both_passwords() {
        let message = rewrite_refusal(RewriteBlocker::IncompleteCredentials);
        assert!(message.contains("user"));
        assert!(message.contains("owner"));
    }

    #[test]
    fn an_unsupported_handler_blocker_names_no_password_at_all() {
        let message = rewrite_refusal(RewriteBlocker::UnsupportedHandler);
        assert!(!message.contains("password"));
    }
}
