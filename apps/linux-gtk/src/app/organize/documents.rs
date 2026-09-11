//! The Organize screen's "Documents" view: one card per contiguous run of
//! pages from the same PDF, in page order (checklist §9,
//! `docs/batch-pdf-assembly.md`).
//!
//! ## Why a rebuilt list rather than a sorted grid
//!
//! The Pages view never rebuilds on a move — its cards are expensive (one
//! pdfium render each) and its `FlowBox` refuses to take a child back once
//! removed, so [`super::reorder_cards`] reorders a sort key instead. Neither
//! pressure applies here. A block list is one render per *document*, not per
//! page, and the blocks themselves are derived fresh from `Document.pages`
//! by `pdf_document::derive_blocks` on every call: a move can merge two runs
//! into one or split one in two, so there is no card identity to preserve
//! across the change even in principle. Rebuilding the whole list is both
//! cheaper to reason about and the only correct answer.
//!
//! ## Why the drop targets are the gaps, not the cards
//!
//! The checklist asks for insertions before, after and at the end of the
//! list, and for the destination to be obvious mid-drag. A card-sized drop
//! target has to guess "before or after?" from the pointer's offset within
//! it, and has nothing to show for the answer. A slim target *between* the
//! cards — one more of them than there are cards — is the position, so
//! highlighting the one under the pointer says exactly where the block will
//! land.

use gtk::prelude::*;
use pdf_document::{derive_blocks, BlockSource, PageId};
use pdf_render::DocumentHandle;

use crate::app::state::Viewer;

use card::build_card;
use gap::build_gap;

mod card;
pub(in crate::app::organize) mod gap;
mod style;

pub(crate) use style::ORGANIZE_CSS;

pub(super) const DOCUMENTS_VIEW: &str = "documents";
pub(super) const PAGES_VIEW: &str = "pages";

pub(super) const DOCUMENTS_HINT: &str = "Drag a document to move all of its pages.";
pub(super) const PAGES_HINT: &str = "Drag a page to reorder it.";

/// One block, flattened into everything a card needs and nothing that needs
/// the session borrow held open.
pub(super) struct Row {
    /// The block's stable identity — the `PageId` of its first page. Carried
    /// as the drag payload rather than the card's position, so a drop
    /// resolves against the blocks as they are at drop time.
    pub(super) anchor: PageId,
    pub(super) name: String,
    pub(super) part: Option<u32>,
    /// 0-based index of the block's first page in `Document.pages`.
    pub(super) start: usize,
    pub(super) count: usize,
    /// The block cover's page in the open pdfium handle, when it holds it
    /// yet — see `grid::backend_indexes` for why this is not `start`.
    pub(super) cover: Option<usize>,
}

impl Row {
    /// "report.pdf" or "report.pdf — Part 2", the card's title.
    pub(super) fn title(&self) -> String {
        match self.part {
            Some(part) => format!("{} — Part {part}", self.name),
            None => self.name.clone(),
        }
    }

    /// "7 pages · 3–9", the card's second line: how big the block is and
    /// where it currently sits in the assembled document.
    pub(super) fn meta(&self) -> String {
        let pages = if self.count == 1 { "page" } else { "pages" };
        let first = self.start + 1;
        let last = self.start + self.count;
        if self.count == 1 {
            format!("1 {pages} · {first}")
        } else {
            format!("{} {pages} · {first}–{last}", self.count)
        }
    }
}

/// Clears the list and rebuilds one card per block, separated by the drop
/// gaps described in this module's header.
///
/// Safe to call with no session or no model: the list is left empty, the
/// same way `grid::populate_grid` leaves the grid.
pub(super) fn populate(viewer: &Viewer) {
    let list = viewer.organize.documents_list.clone();
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let Some((rows, handle)) = rows(viewer) else {
        return;
    };

    list.append(&build_gap(viewer, 0));
    for (position, row) in rows.iter().enumerate() {
        list.append(&build_card(viewer, position, rows.len(), row, handle));
        list.append(&build_gap(viewer, position + 1));
    }
}

/// Every block of the current model, in page order, with the session's names
/// for their sources — or `None` when there is no model to derive from.
pub(super) fn rows(viewer: &Viewer) -> Option<(Vec<Row>, DocumentHandle)> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    let model = session.document_model.as_ref()?;

    let mut start = 0;
    let rows = derive_blocks(model)
        .into_iter()
        .map(|block| {
            let row = Row {
                anchor: block.anchor,
                name: match block.source {
                    BlockSource::Base => session.base_name.clone(),
                    BlockSource::Blank => "Blank pages".to_owned(),
                    BlockSource::Imported(id) => session
                        .imported_sources
                        .iter()
                        .find(|source| source.id == id)
                        .map_or_else(|| "Imported PDF".to_owned(), |source| source.name.clone()),
                },
                part: block.part,
                start,
                count: block.pages.len(),
                cover: session.backend_index(block.anchor),
            };
            start += row.count;
            row
        })
        .collect();
    Some((rows, session.document))
}

/// Rebuilds the list when `changed` says the model moved, and passes the
/// answer through.
///
/// The rebuild belongs here rather than only in
/// `super::refresh_after_reopen`, which runs after the command's preview
/// refresh: that refresh bails out when the session has no `save_backing` to
/// replay against, and a list still showing the old blocks would then be the
/// only thing the user sees. Doing both costs one render per *block* twice
/// over — checklist §11's thumbnail cache is what removes the second.
pub(super) fn rebuilt(viewer: &Viewer, changed: bool) -> bool {
    if changed {
        populate(viewer);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cards_two_lines_name_the_block_and_where_it_sits() {
        let split = Row {
            anchor: PageId(4),
            name: "report.pdf".to_owned(),
            part: Some(2),
            start: 2,
            count: 7,
            cover: None,
        };

        assert_eq!(split.title(), "report.pdf — Part 2");
        assert_eq!(split.meta(), "7 pages · 3–9");
    }

    #[test]
    fn a_single_page_block_says_page_not_pages_and_shows_no_range() {
        let single = Row {
            anchor: PageId(0),
            name: "cover.pdf".to_owned(),
            part: None,
            start: 0,
            count: 1,
            cover: None,
        };

        assert_eq!(single.title(), "cover.pdf");
        assert_eq!(single.meta(), "1 page · 1");
    }
}
