//! What the Extract dialog is asking, as rules rather than widgets.
//!
//! The same cut `export::options` makes, for the same reason: everything a
//! person can get wrong is decided here, before a destination is chosen and
//! long before a byte is written, and none of it takes a widget. Each rule is
//! a plain function of its arguments and each has a test that reaches it
//! directly.
//!
//! The page grammar itself — `"1-3,7"` — is not here. It lives in
//! `pdf_save::export::selection`, shared with every shell that grows a screen
//! asking which pages it should act on; this module only decides what to do
//! with what that grammar answers.

use std::path::Path;

/// The pages `typed` names, as ascending deduplicated **zero-based** indices,
/// or the message to show instead.
///
/// One-based on the way in because that is what the user typed, zero-based on
/// the way out because that is what indexes `Document.pages`.
///
/// The empty case is caught here rather than left to
/// [`pdf_save::parse_page_selection`], whose own `Empty` message says
/// "export". Every other refusal it produces — a backwards range, a page past
/// the end, a word where a number belongs — is verb-neutral and is shown
/// verbatim.
pub(super) fn resolve_pages(typed: &str, total_pages: u32) -> Result<Vec<u32>, String> {
    if total_pages == 0 {
        return Err("This document has no pages to extract.".to_owned());
    }
    if typed.trim().is_empty() {
        return Err("Type which pages to extract, for example 1-3,7.".to_owned());
    }
    pdf_save::parse_page_selection(typed, total_pages).map_err(|error| error.to_string())
}

/// What the status line says once the extracted PDF is on disk.
///
/// Its own function so the singular/plural, the "where did it go" half and
/// the signature sentence are pinned by tests rather than by reading a
/// `format!` at the call site.
///
/// ## Why the signature note is here and not in a modal
///
/// An extraction writes a **new** file and leaves the open document — and the
/// file it came from — exactly as they were. So there is no irreversible
/// choice for the user to consent to, and `chooser::confirm_signature_loss`
/// would actively misdescribe what is about to happen: its words are "Saving
/// will break this document's signature", and nothing here saves *this*
/// document. What is true is narrower and worth saying afterwards — the file
/// that was just written carries a signature covering a page set it no longer
/// has, so a reader will report it invalid.
pub(super) fn extract_summary(count: usize, destination: &Path, signed: bool) -> String {
    let pages = if count == 1 { "page" } else { "pages" };
    let mut summary = format!("Extracted {count} {pages} to {}.", destination.display());
    if signed {
        summary.push_str(
            " The original document is signed, so the extracted PDF's signature \
             no longer verifies.",
        );
    }
    summary
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{extract_summary, resolve_pages};

    #[test]
    fn a_typed_range_becomes_zero_based_indices() {
        assert_eq!(resolve_pages("1-3,7", 10), Ok(vec![0, 1, 2, 6]));
    }

    #[test]
    fn an_empty_selection_is_refused_in_this_screen_s_own_words() {
        assert_eq!(
            resolve_pages("   ", 10),
            Err("Type which pages to extract, for example 1-3,7.".to_owned())
        );
    }

    /// The grammar's own refusals are shown verbatim rather than restated —
    /// this pins that they reach the dialog at all.
    #[test]
    fn a_page_past_the_end_is_refused_by_the_shared_grammar() {
        assert_eq!(
            resolve_pages("11", 10),
            Err("This document has 10 pages, so page 11 does not exist.".to_owned())
        );
    }

    #[test]
    fn a_document_with_no_pages_has_nothing_to_extract() {
        assert_eq!(
            resolve_pages("1", 0),
            Err("This document has no pages to extract.".to_owned())
        );
    }

    #[test]
    fn one_extracted_page_is_singular() {
        assert_eq!(
            extract_summary(1, Path::new("/tmp/part.pdf"), false),
            "Extracted 1 page to /tmp/part.pdf."
        );
    }

    #[test]
    fn several_extracted_pages_are_plural() {
        assert_eq!(
            extract_summary(3, Path::new("/tmp/part.pdf"), false),
            "Extracted 3 pages to /tmp/part.pdf."
        );
    }

    #[test]
    fn a_signed_source_earns_a_sentence_about_the_written_file() {
        let summary = extract_summary(2, Path::new("/tmp/part.pdf"), true);
        assert!(summary.starts_with("Extracted 2 pages to /tmp/part.pdf."));
        assert!(summary.contains("no longer verifies"));
    }
}
