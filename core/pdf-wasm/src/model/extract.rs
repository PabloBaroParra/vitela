//! Extracting pages to a new PDF, and splitting a document into parts (B26
//! T-254).
//!
//! Every rule is the core's, asked the way `pdf-ffi`'s `extract` and `split`
//! modules ask it: the gates and the cut are `pdf_save::extracted_pages`
//! (moved there in T-249 so the web and `pdf-ffi` share it), the gates alone
//! are `pdf_save::extraction_refusal`, and a split's cut grammar, its parts
//! and their file names are `pdf_save::{resolve_split_cuts, split_parts,
//! split_part_file_name}` — the same names on every shell.
//!
//! The gate is `/P` bit 5 (extracting text **and graphics**), not page
//! assembly: an extraction changes nothing about the open document. A second
//! gate follows: a new page set needs a full rewrite, which an encrypted
//! document opened with only one of its two passwords can never produce.
//!
//! An extraction from a protected document is itself protected (the core
//! saves it with `SaveIntent::Default`), and from a signed one it carries a
//! signature that no longer verifies — never a refusal: nothing the user is
//! looking at is written over. The pages come from the session as it is now,
//! pending edits included, and a **clone** is pruned, so extracting records
//! no edit and leaves the session as it found it. Each part of a split is an
//! extraction of its range.

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// One part of a split: an inclusive run of zero-based positions, and the
/// file it is written under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPartPlan {
    pub first: u32,
    pub last: u32,
    pub file_name: String,
}

impl From<pdf_save::ExtractError> for EngineError {
    /// `pdf-ffi`'s mapping: a selection the core cannot cut is
    /// `InvalidPageSelection`, a position past the last page keeps its own
    /// kind, a failed save keeps the save's, and every refusal of the
    /// document itself (its permission, its encryption) is
    /// `UnsupportedOperation` with the core's sentence.
    fn from(err: pdf_save::ExtractError) -> Self {
        use pdf_save::ExtractError as E;
        match err {
            E::NoPages | E::NotAscending => {
                Self::new(ErrorKind::InvalidPageSelection, err.to_string())
            }
            E::PageOutOfBounds(index) => Self::page_out_of_bounds(index),
            E::Save(err) => err.into(),
            refused => Self::new(ErrorKind::UnsupportedOperation, refused.to_string()),
        }
    }
}

impl DocumentModel {
    /// Why no page of this document can be extracted (or split off), as the
    /// core's sentence, or `None` when they can. Cheap: it reads the
    /// permissions, it saves nothing.
    pub fn extraction_refusal(&self) -> Option<String> {
        pdf_save::extraction_refusal(&self.document).map(|refusal| refusal.to_string())
    }

    /// Whether the file as opened carries a digital signature, which an
    /// extracted copy cannot keep valid — the note a shell adds to its
    /// summary (`pdf-ffi`'s `extract_source_is_signed`).
    pub fn extract_source_is_signed(&self) -> bool {
        pdf_manip::document_has_signatures(&self.base)
    }

    /// A new PDF of `pages` (zero-based, ascending, unique), with every
    /// pending edit applied. The session is untouched.
    pub fn extracted_pages(&self, pages: &[u32]) -> Result<Vec<u8>, EngineError> {
        Ok(self.with_save_source(|source| pdf_save::extracted_pages(source, pages))?)
    }
}

/// The parts a split at `typed` (one-based "split after page" positions,
/// `"3,7"`) makes of a document of `total_pages`, each named after
/// `document_name` (`"report.pdf"` → `"report-part1.pdf"`). Refused with
/// `InvalidPageSelection` and the core's sentence: a one-page document, no
/// cut, a cut the grammar cannot read, or one after the last page.
pub fn plan_split(
    typed: &str,
    total_pages: u32,
    document_name: &str,
) -> Result<Vec<SplitPartPlan>, EngineError> {
    let cuts = pdf_save::resolve_split_cuts(typed, total_pages)
        .map_err(|message| EngineError::new(ErrorKind::InvalidPageSelection, message))?;
    let ranges = pdf_save::split_parts(&cuts, total_pages);
    let total = ranges.len() as u32;
    let stem = pdf_save::document_file_stem(document_name);
    Ok(ranges
        .into_iter()
        .enumerate()
        .map(|(index, (first, last))| SplitPartPlan {
            first,
            last,
            file_name: pdf_save::split_part_file_name(stem, index as u32, total),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{pdfs, PageEdit};
    use pdf_manip::OpenCredentials;

    fn open(bytes: &[u8]) -> DocumentModel {
        DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
    }

    #[test]
    fn the_new_pdf_holds_the_selected_pages_and_the_session_is_untouched() {
        let model = open(&pdfs::text_document(&["one", "two", "three", "four"]));
        assert_eq!(model.extraction_refusal(), None);
        assert!(!model.extract_source_is_signed());

        let bytes = model.extracted_pages(&[1, 2]).unwrap();
        assert_eq!(open(&bytes).page_count(), 2);
        assert_eq!(model.page_count(), 4);
        assert!(!model.can_undo());
    }

    #[test]
    fn pending_edits_are_in_the_extracted_pages() {
        let mut model = open(&pdfs::text_document(&["one", "two", "three"]));
        model
            .apply_edit(PageEdit::Remove { index: 0, count: 1 })
            .unwrap();
        // Positions name the pages as they are now: "two" and "three".
        let bytes = model.extracted_pages(&[0, 1]).unwrap();
        assert_eq!(open(&bytes).page_count(), 2);
        assert_eq!(
            model.extracted_pages(&[2]).unwrap_err().kind,
            ErrorKind::PageIndexOutOfBounds
        );
        assert!(model.can_undo());
    }

    #[test]
    fn a_selection_the_core_cannot_cut_is_refused_with_its_kind() {
        let model = open(&pdfs::text_document(&["one", "two"]));
        for pages in [&[][..], &[1, 0][..], &[0, 0][..]] {
            let err = model.extracted_pages(pages).unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidPageSelection);
        }
        let err = model.extracted_pages(&[0, 5]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::PageIndexOutOfBounds);
    }

    #[test]
    fn a_document_withholding_extraction_is_refused_ahead() {
        let model = DocumentModel::open(
            &pdfs::restricted_document(),
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        let refusal = model.extraction_refusal().expect("refused");
        assert_eq!(
            refusal,
            "this document does not permit extracting its pages"
        );
        let err = model.extracted_pages(&[0]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
        assert_eq!(err.message, refusal);
    }

    #[test]
    fn one_password_of_two_cannot_rewrite_and_both_keep_the_protection() {
        let one = DocumentModel::open(
            &pdfs::locked_document(&["one", "two"]),
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        let refusal = one.extraction_refusal().expect("refused");
        assert!(refusal.contains("user") && refusal.contains("owner"));
        assert_eq!(
            one.extracted_pages(&[0]).unwrap_err().kind,
            ErrorKind::UnsupportedOperation
        );

        let both = DocumentModel::open(
            &pdfs::locked_document(&["one", "two"]),
            &OpenCredentials::UserAndOwner {
                user: pdfs::USER_PASSWORD.into(),
                owner: pdfs::OWNER_PASSWORD.into(),
            },
        )
        .unwrap();
        assert_eq!(both.extraction_refusal(), None);
        let bytes = both.extracted_pages(&[1]).unwrap();
        // The extracted file is protected like the source.
        assert_eq!(
            DocumentModel::open(&bytes, &OpenCredentials::NoPassword)
                .err()
                .unwrap()
                .kind,
            ErrorKind::PasswordRequired
        );
        let reopened = DocumentModel::open(
            &bytes,
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        assert_eq!(reopened.page_count(), 1);
    }

    #[test]
    fn a_signed_source_is_extracted_and_says_so() {
        let model = open(pdfs::SIGNED);
        assert!(model.extract_source_is_signed());
        let bytes = model.extracted_pages(&[0]).unwrap();
        assert_eq!(open(&bytes).page_count(), 1);
    }

    #[test]
    fn a_split_plan_names_the_parts_as_every_shell_does() {
        let parts = plan_split("2,4", 5, "report.pdf").unwrap();
        let named = |first, last, file_name: &str| SplitPartPlan {
            first,
            last,
            file_name: file_name.into(),
        };
        assert_eq!(
            parts,
            vec![
                named(0, 1, "report-part1.pdf"),
                named(2, 3, "report-part2.pdf"),
                named(4, 4, "report-part3.pdf"),
            ]
        );
    }

    #[test]
    fn a_split_the_core_refuses_is_an_invalid_selection_in_its_words() {
        for (typed, total, says) in [
            ("1", 1, "one page cannot be split"),
            ("  ", 5, "Type where to cut"),
            ("5", 5, "nothing after page 5"),
            ("x", 5, ""),
        ] {
            let err = plan_split(typed, total, "report.pdf").unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidPageSelection);
            assert!(err.message.contains(says), "{}", err.message);
        }
    }
}
