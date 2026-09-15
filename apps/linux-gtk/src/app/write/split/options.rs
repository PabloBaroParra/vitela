//! What the Split dialog is asking, as rules rather than widgets.
//!
//! The same cut [`extract::options`](super::super::extract) and
//! `export::options` make, for the same reason: everything a person can get
//! wrong is decided here, before a folder is chosen and long before a byte is
//! written, and none of it takes a widget. Each rule is a plain function of
//! its arguments and each has a test that reaches it directly.
//!
//! The number grammar itself — `"3,7"` — is not here. It is
//! `pdf_save::parse_page_selection`, the same grammar Export and Extract ask
//! in; this module only decides what a *cut* at one of those numbers means.

use std::path::Path;

/// The cuts `typed` names, as ascending deduplicated **zero-based** indices of
/// the page each cut falls *after*, or the message to show instead.
///
/// One-based on the way in because that is what the user typed, zero-based on
/// the way out because that is what indexes `Document.pages`.
///
/// ## Why the last page is refused
///
/// `parse_page_selection` is asked for `total_pages`, not `total_pages - 1`,
/// so that a page past the end is still refused in the grammar's own honest
/// words — lowering the ceiling would make a ten-page document claim it has
/// nine. A cut *after* the last page is a different mistake: the page exists,
/// but there is nothing on the far side of it, so it would produce an empty
/// second file. That gets its own sentence rather than a shrug, because
/// silently dropping the cut would split into fewer files than the user asked
/// for and give no sign of having done so.
pub(super) fn resolve_cuts(typed: &str, total_pages: u32) -> Result<Vec<u32>, String> {
    if total_pages < 2 {
        return Err("A document of one page cannot be split.".to_owned());
    }
    if typed.trim().is_empty() {
        return Err("Type where to cut, for example 3 to split after page 3.".to_owned());
    }
    let cuts = pdf_save::parse_page_selection(typed, total_pages).map_err(|e| e.to_string())?;
    if cuts.contains(&(total_pages - 1)) {
        return Err(format!(
            "There is nothing after page {total_pages}, so the document cannot be split there."
        ));
    }
    Ok(cuts)
}

/// The page range each written file covers, as **inclusive** zero-based
/// `(first, last)` pairs in document order.
///
/// A cut at `c` ends a part: the pages `..=c` are one file and `c + 1..` are
/// the next. So `n` cuts always produce `n + 1` parts, every page of the
/// document lands in exactly one of them, and none of them is empty —
/// [`resolve_cuts`] is what guarantees the last of those by refusing a cut
/// after the final page.
///
/// `cuts` must be ascending and deduplicated, which `parse_page_selection`
/// guarantees.
pub(super) fn parts(cuts: &[u32], total_pages: u32) -> Vec<(u32, u32)> {
    let mut parts = Vec::with_capacity(cuts.len() + 1);
    let mut first = 0;
    for &cut in cuts {
        parts.push((first, cut));
        first = cut + 1;
    }
    parts.push((first, total_pages - 1));
    parts
}

/// What the status line says once every part is on disk.
///
/// Its own function so the "where did they go" half and the signature
/// sentence are pinned by tests rather than by reading a `format!` at the
/// call site.
///
/// Always plural: [`parts`] returns one file per cut plus one, and
/// [`resolve_cuts`] refuses an empty cut list, so the smallest split this
/// chain can reach is two files.
///
/// ## Why the signature note is here and not in a modal
///
/// The same argument [`extract::options::extract_summary`](super::super::
/// extract) makes. A split writes **new** files and leaves the open document —
/// and the file it came from — exactly as they were, so there is no
/// irreversible choice to consent to, and `chooser::confirm_signature_loss`
/// would misdescribe it: its words are "Saving will break this document's
/// signature", and nothing here saves *this* document. What is true is
/// narrower and worth saying afterwards — each file that was written carries
/// a signature covering a page set it no longer has, so a reader will report
/// it invalid.
pub(super) fn split_summary(parts: usize, folder: &Path, signed: bool) -> String {
    let mut summary = format!("Split into {parts} PDFs in {}.", folder.display());
    if signed {
        summary.push_str(
            " The original document is signed, so the signature in each part \
             no longer verifies.",
        );
    }
    summary
}

/// What one part of a split is called, for `total` parts of a document named
/// `stem`.
///
/// A thin pass through `pdf_save::split_part_file_name`, which is where the
/// padding and the path-safety live. It is named here so the worker and the
/// overwrite guard in [`super`] cannot disagree about what they are checking
/// for and writing — the guard's whole value is that it names the same files
/// the worker will.
pub(super) fn part_file_name(stem: &str, index: usize, total: usize) -> String {
    pdf_save::split_part_file_name(stem, index as u32, total as u32)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{part_file_name, parts, resolve_cuts, split_summary};

    #[test]
    fn a_typed_cut_becomes_a_zero_based_index() {
        assert_eq!(resolve_cuts("3", 10), Ok(vec![2]));
    }

    #[test]
    fn several_cuts_come_back_ascending_and_deduplicated() {
        assert_eq!(resolve_cuts("7,3,3", 10), Ok(vec![2, 6]));
    }

    /// A range is a run of cuts, which is what the shared grammar already
    /// means by it: "1-3" cuts after pages 1, 2 and 3.
    #[test]
    fn a_range_cuts_after_every_page_in_it() {
        assert_eq!(resolve_cuts("1-3", 10), Ok(vec![0, 1, 2]));
    }

    #[test]
    fn an_empty_selection_is_refused_in_this_screen_s_own_words() {
        assert_eq!(
            resolve_cuts("   ", 10),
            Err("Type where to cut, for example 3 to split after page 3.".to_owned())
        );
    }

    /// The grammar's own refusals are shown verbatim rather than restated —
    /// this pins that they reach the dialog at all, and that the ceiling
    /// asked for is the real page count.
    #[test]
    fn a_page_past_the_end_is_refused_by_the_shared_grammar() {
        assert_eq!(
            resolve_cuts("11", 10),
            Err("This document has 10 pages, so page 11 does not exist.".to_owned())
        );
    }

    /// The page exists; the far side of it does not. A different mistake from
    /// the one above, and it earns a different sentence.
    #[test]
    fn a_cut_after_the_last_page_is_refused_as_having_nothing_after_it() {
        assert_eq!(
            resolve_cuts("10", 10),
            Err(
                "There is nothing after page 10, so the document cannot be split there.".to_owned()
            )
        );
    }

    #[test]
    fn a_one_page_document_cannot_be_split() {
        assert_eq!(
            resolve_cuts("1", 1),
            Err("A document of one page cannot be split.".to_owned())
        );
    }

    #[test]
    fn one_cut_makes_two_parts_that_meet_at_it() {
        assert_eq!(parts(&[2], 10), vec![(0, 2), (3, 9)]);
    }

    #[test]
    fn two_cuts_make_three_parts() {
        assert_eq!(parts(&[2, 6], 10), vec![(0, 2), (3, 6), (7, 9)]);
    }

    #[test]
    fn a_cut_after_the_first_page_leaves_a_single_page_part() {
        assert_eq!(parts(&[0], 3), vec![(0, 0), (1, 2)]);
    }

    /// The property every part list must have: the parts tile the document
    /// exactly — no page in two of them, no page in none, and none empty.
    #[test]
    fn the_parts_cover_every_page_exactly_once() {
        let total = 12;
        let covered: Vec<u32> = parts(&[0, 4, 5, 10], total)
            .into_iter()
            .inspect(|&(first, last)| assert!(first <= last, "no part may be empty"))
            .flat_map(|(first, last)| first..=last)
            .collect();

        assert_eq!(covered, (0..total).collect::<Vec<u32>>());
    }

    #[test]
    fn the_summary_names_the_count_and_the_folder() {
        assert_eq!(
            split_summary(3, Path::new("/tmp/parts"), false),
            "Split into 3 PDFs in /tmp/parts."
        );
    }

    #[test]
    fn a_signed_source_earns_a_sentence_about_the_written_files() {
        let summary = split_summary(2, Path::new("/tmp/parts"), true);
        assert!(summary.starts_with("Split into 2 PDFs in /tmp/parts."));
        assert!(summary.contains("no longer verifies"));
    }

    /// The guard in `super` and the worker must name the same files, which
    /// they do by both calling this.
    #[test]
    fn a_part_is_named_one_based_after_the_document() {
        assert_eq!(part_file_name("report", 0, 2), "report-part1.pdf");
        assert_eq!(part_file_name("report", 1, 2), "report-part2.pdf");
    }
}
