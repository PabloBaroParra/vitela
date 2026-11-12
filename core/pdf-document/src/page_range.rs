//! Constructors for the two page-range commands a shell builds from
//! positions — [`Command::MovePages`] and [`Command::RotatePages`] — that
//! name the position that was out of range instead of only refusing.
//!
//! The model refuses a bad range itself, but only as `false` from
//! `EditLog::apply`. A shell has to tell the reader *which* page was not
//! there, so the bounds are checked here, against the same document the
//! command is about to be applied to, and every boundary (UniFFI, wasm)
//! reports the same position for the same request.

use crate::document::Document;
use crate::edit_log::Command;

/// A page position that does not exist in the document — the one a caller
/// should name in its error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePositionOutOfRange {
    pub position: usize,
}

impl std::fmt::Display for PagePositionOutOfRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "page position {} is out of range", self.position)
    }
}

impl std::error::Error for PagePositionOutOfRange {}

/// Whether the run of `count` pages starting at `start` fits in `len` pages.
/// Checked arithmetic: a position near `usize::MAX` (a 32-bit `usize` on
/// wasm32) must read as out of range, not wrap into range.
fn run_fits(start: usize, count: usize, len: usize) -> bool {
    start.checked_add(count).is_some_and(|end| end <= len)
}

impl Command {
    /// A move of the `count` pages starting at position `from` so the first
    /// of them ends up at position `to`.
    ///
    /// Refuses with `from` when the source run does not fit, then with `to`
    /// when the destination run does not. A zero `count` or a move onto
    /// itself is left to the model's own refusal on apply: neither names a
    /// bad position.
    pub fn move_pages(
        document: &Document,
        from: usize,
        count: usize,
        to: usize,
    ) -> Result<Command, PagePositionOutOfRange> {
        let len = document.pages.len();
        if !run_fits(from, count, len) {
            return Err(PagePositionOutOfRange { position: from });
        }
        if !run_fits(to, count, len) {
            return Err(PagePositionOutOfRange { position: to });
        }
        Ok(Command::MovePages { from, count, to })
    }

    /// A turn of each of the `count` pages starting at position `from` by
    /// `delta_degrees`, as one step.
    ///
    /// The positions are resolved to the pages that sit there now, so the
    /// step undoes the same pages even after a later reorder. An empty run
    /// is refused with `from` as well: it would be a history step that turns
    /// nothing.
    pub fn rotate_pages(
        document: &Document,
        from: usize,
        count: usize,
        delta_degrees: i32,
    ) -> Result<Command, PagePositionOutOfRange> {
        let out_of_range = PagePositionOutOfRange { position: from };
        if count == 0 || !run_fits(from, count, document.pages.len()) {
            return Err(out_of_range);
        }
        let pages = document.pages[from..from + count]
            .iter()
            .map(|page| page.id)
            .collect();
        Ok(Command::RotatePages {
            pages,
            delta_degrees,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Orientation, Page, PageId, PageSize};

    fn document_with_pages(ids: &[u32]) -> Document {
        Document::with_pages(
            ids.iter()
                .map(|id| Page::blank(PageId(*id), PageSize::A4, Orientation::Portrait))
                .collect(),
        )
    }

    fn out_of_range(position: usize) -> Result<Command, PagePositionOutOfRange> {
        Err(PagePositionOutOfRange { position })
    }

    #[test]
    fn a_move_that_fits_builds_the_command() {
        let document = document_with_pages(&[0, 1, 2]);

        assert_eq!(
            Command::move_pages(&document, 0, 2, 1),
            Ok(Command::MovePages {
                from: 0,
                count: 2,
                to: 1
            })
        );
    }

    #[test]
    fn a_move_whose_source_run_overflows_names_the_source() {
        let document = document_with_pages(&[0, 1, 2]);

        assert_eq!(Command::move_pages(&document, 2, 2, 0), out_of_range(2));
    }

    /// The source is asked first: when both runs overflow, `from` is named.
    #[test]
    fn a_move_whose_both_runs_overflow_names_the_source_first() {
        let document = document_with_pages(&[0, 1, 2]);

        assert_eq!(Command::move_pages(&document, 3, 1, 5), out_of_range(3));
    }

    #[test]
    fn a_move_whose_destination_run_overflows_names_the_destination() {
        let document = document_with_pages(&[0, 1, 2]);

        assert_eq!(Command::move_pages(&document, 0, 2, 2), out_of_range(2));
    }

    /// Neither names a bad position, so both are left to the model.
    #[test]
    fn an_empty_move_or_a_move_onto_itself_is_not_a_range_error() {
        let document = document_with_pages(&[0, 1, 2]);

        assert!(Command::move_pages(&document, 1, 0, 2).is_ok());
        assert!(Command::move_pages(&document, 1, 1, 1).is_ok());
    }

    #[test]
    fn a_move_near_the_end_of_the_index_space_does_not_wrap() {
        let document = document_with_pages(&[0, 1, 2]);

        assert_eq!(
            Command::move_pages(&document, usize::MAX, 2, 0),
            out_of_range(usize::MAX)
        );
        assert_eq!(
            Command::move_pages(&document, 0, 2, usize::MAX),
            out_of_range(usize::MAX)
        );
    }

    #[test]
    fn a_rotation_resolves_its_positions_to_the_pages_there_now() {
        // Ids out of position order, as after a reorder.
        let document = document_with_pages(&[7, 3, 5]);

        assert_eq!(
            Command::rotate_pages(&document, 1, 2, 90),
            Ok(Command::RotatePages {
                pages: vec![PageId(3), PageId(5)],
                delta_degrees: 90
            })
        );
    }

    #[test]
    fn an_empty_rotation_is_refused_at_its_start() {
        let document = document_with_pages(&[0, 1, 2]);

        assert_eq!(Command::rotate_pages(&document, 1, 0, 90), out_of_range(1));
    }

    #[test]
    fn a_rotation_past_the_last_page_is_refused_at_its_start() {
        let document = document_with_pages(&[0, 1, 2]);

        assert_eq!(Command::rotate_pages(&document, 2, 2, 90), out_of_range(2));
        assert_eq!(
            Command::rotate_pages(&document, usize::MAX, 2, 90),
            out_of_range(usize::MAX)
        );
    }
}
