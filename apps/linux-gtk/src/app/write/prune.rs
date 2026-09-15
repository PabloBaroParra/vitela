//! Cutting a document model down to a chosen set of pages.
//!
//! Shared by the two chains that build a **second** PDF out of a subset of
//! the open document's pages — [`extract`](super::extract), which writes one
//! file, and [`split`](super::split), which writes several. Both want exactly
//! the same thing from a snapshot of the live model ("leave these pages and
//! drop the rest"), and both must get it the same way, so the rule lives once
//! here rather than twice in two workers that would drift.
//!
//! Nothing in this module touches the session. Every function takes a
//! `Document` the caller already owns — in practice a **clone** of the
//! session's model, so the removals recorded below cannot reach the undo
//! history the user is still editing against.

use pdf_document::{Command, Document};

/// The runs of pages to drop so that only `keep` is left, as `(index, count)`
/// pairs **in descending index order**.
///
/// Descending is the whole point. Each removal shifts every later page down,
/// so a run removed before a run that follows it would leave the second one
/// naming pages that have already moved. Taking the last run first means
/// every index still describes the page it was computed from.
///
/// `keep` must be ascending and deduplicated — `pdf_save::parse_page_selection`
/// guarantees it for the ranges a user types — which is what lets the
/// membership test be a binary search rather than a scan per page. On a
/// thousand-page document the difference is a million comparisons.
pub(super) fn removal_runs(keep: &[u32], total: u32) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut open: Option<(usize, usize)> = None;
    for index in 0..total as usize {
        if keep.binary_search(&(index as u32)).is_ok() {
            if let Some(finished) = open.take() {
                runs.push(finished);
            }
        } else {
            match open.as_mut() {
                Some((_, count)) => *count += 1,
                None => open = Some((index, 1)),
            }
        }
    }
    runs.extend(open);
    runs.reverse();
    runs
}

/// Drops every page of `document` that is not in `keep`.
///
/// Through `Command::remove_pages` and the `EditLog`, not by truncating
/// `Document.pages` directly, because `pdf-save` materializes a page set from
/// the *log*: `has_structural_page_changes`/`replay_page_ops` read the
/// recorded commands to decide which `pdf_manip` calls the save makes. A
/// document whose `pages` were edited behind the log's back would serialize
/// as though nothing had been removed at all.
///
/// `remove_pages` rather than a `remove_page` per index for the same reason
/// `organize::command::delete_block` uses it: it captures the annotations and
/// form fields anchored to every page of the run, so the removal cannot
/// strand an annotation on a page id `pdf-save` would then refuse to write.
///
/// The log this builds is thrown away with the clone it was built on — the
/// live session's own undo history is never touched, because `document` here
/// is always a snapshot taken under a borrow the caller has already released.
pub(super) fn prune_to(document: &mut Document, keep: &[u32]) -> Result<(), String> {
    let total = document.pages.len() as u32;
    for (index, count) in removal_runs(keep, total) {
        let removal = Command::remove_pages(document, index, count)
            .ok_or_else(|| "the pages to leave out no longer exist".to_owned())?;
        if !apply(document, removal) {
            return Err("the pages to leave out could not be dropped".to_owned());
        }
    }
    Ok(())
}

/// `EditLog::apply` against a document's own pending log.
///
/// A local copy of `organize::command::apply_command` rather than a call to
/// it: `organize` already depends on `write` (its Save button opens this
/// module's chooser), so reaching back the other way would close a cycle
/// between the two. Five lines is the cheaper of the two prices.
fn apply(document: &mut Document, command: Command) -> bool {
    let mut log = std::mem::take(&mut document.pending_edits);
    let applied = log.apply(document, command);
    document.pending_edits = log;
    applied
}

#[cfg(test)]
mod tests {
    use pdf_document::{Command, Document, Orientation, Page, PageId, PageSize};

    use super::{prune_to, removal_runs};

    fn a_document_of(pages: usize) -> Document {
        let mut document = Document::blank();
        for index in 0..pages {
            document.pages.push(Page::blank(
                PageId(index as u32),
                PageSize::A4,
                Orientation::Portrait,
            ));
        }
        document
    }

    fn page_ids(document: &Document) -> Vec<u32> {
        document.pages.iter().map(|page| page.id.0).collect()
    }

    #[test]
    fn keeping_a_middle_run_drops_the_pages_on_both_sides_last_first() {
        assert_eq!(removal_runs(&[2, 3], 6), vec![(4, 2), (0, 2)]);
    }

    #[test]
    fn keeping_every_page_drops_nothing() {
        assert_eq!(removal_runs(&[0, 1, 2], 3), Vec::<(usize, usize)>::new());
    }

    #[test]
    fn keeping_one_page_drops_the_rest_as_two_runs() {
        assert_eq!(removal_runs(&[1], 3), vec![(2, 1), (0, 1)]);
    }

    #[test]
    fn keeping_the_first_page_drops_one_trailing_run() {
        assert_eq!(removal_runs(&[0], 4), vec![(1, 3)]);
    }

    /// Applying the runs in the order they are returned must leave exactly
    /// `keep` behind — the property the descending order exists for.
    #[test]
    fn applying_the_runs_in_order_leaves_exactly_the_kept_pages() {
        let keep = [0, 3, 4, 8];
        let mut pages: Vec<u32> = (0..10).collect();
        for (index, count) in removal_runs(&keep, 10) {
            pages.drain(index..index + count);
        }
        assert_eq!(pages, keep);
    }

    #[test]
    fn pruning_to_a_middle_run_leaves_exactly_those_pages() {
        let mut document = a_document_of(6);
        prune_to(&mut document, &[2, 3]).expect("the runs are in range");

        assert_eq!(page_ids(&document), vec![2, 3]);
    }

    #[test]
    fn pruning_to_a_scattered_selection_preserves_document_order() {
        let mut document = a_document_of(10);
        prune_to(&mut document, &[0, 3, 4, 8]).expect("the runs are in range");

        assert_eq!(page_ids(&document), vec![0, 3, 4, 8]);
    }

    #[test]
    fn pruning_to_the_last_page_leaves_it_alone() {
        let mut document = a_document_of(4);
        prune_to(&mut document, &[3]).expect("the run is in range");

        assert_eq!(page_ids(&document), vec![3]);
    }

    #[test]
    fn pruning_to_every_page_records_nothing() {
        let mut document = a_document_of(3);
        prune_to(&mut document, &[0, 1, 2]).expect("there is nothing to remove");

        assert_eq!(page_ids(&document), vec![0, 1, 2]);
        assert!(
            document.pending_edits.entries().is_empty(),
            "keeping the whole document should record no removal"
        );
    }

    /// The removals must reach the `EditLog`, because that is what `pdf-save`
    /// replays — a `Document.pages` edited behind the log's back would
    /// serialize as the original document.
    #[test]
    fn every_dropped_run_is_recorded_on_the_log() {
        let mut document = a_document_of(6);
        prune_to(&mut document, &[2, 3]).expect("the runs are in range");

        assert_eq!(
            document.pending_edits.entries().len(),
            2,
            "the pages before and after the kept run are two separate runs"
        );
        // Applied last, so it is the top of the undo stack: the leading run
        // has to go after the trailing one or its indices would have shifted.
        assert!(matches!(
            document.pending_edits.peek_undo(),
            Some(Command::RemovePages { index: 0, .. })
        ));
    }
}
