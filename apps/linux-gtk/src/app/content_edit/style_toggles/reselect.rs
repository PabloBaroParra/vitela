//! Keeping a block selected across the preview refresh a toggle starts, so
//! Bold and then Italic stack without the reader clicking the text again.
//!
//! Every refresh drops the selected block (`write::preview::swap`): the
//! block is a snapshot of a parse the refresh throws away, and after a
//! checkpoint its ids may name other runs. That stays true for every other
//! edit. A toggle only remembers *where* the block was — its page and the
//! middle of its box — and once the refresh has read the pages again the
//! block under that point is selected afresh, from the new parse. Nothing
//! held from before the refresh is reused, so no stale or aliased run can
//! reach the next command.
//!
//! The memory is spent once, and only when nothing happened in between: an
//! edit recorded since the toggle (the session's `edit_revision` moved on),
//! a block, editor or multi-selection the reader made meanwhile — each wins
//! over it.

use std::cell::Cell;
use std::rc::Rc;

use crate::app::content_edit::block;
use crate::app::state::{DocumentSession, Viewer};

/// Where a toggled block was, and the edit it was toggled by.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Marker {
    page_index: usize,
    /// The middle of the block's box, in PDF page space.
    point: (f64, f64),
    edit_revision: u64,
}

/// The block to select again once the refresh lands, if any. Shared by every
/// clone of the toggles.
#[derive(Clone, Default)]
pub(crate) struct Pending(Rc<Cell<Option<Marker>>>);

impl Pending {
    /// Remembers the block `session` has selected, after an edit of it was
    /// recorded. Anything else (an editor, a multi-selection) is not
    /// remembered: the editor survives the refresh, and a multi-selection is
    /// not dropped by it.
    pub(crate) fn remember(&self, session: &DocumentSession) {
        let marker = session.selected_text_block.as_ref().map(|selected| Marker {
            page_index: selected.page_index,
            point: (
                selected.run.bbox.x + selected.run.bbox.width / 2.0,
                selected.run.bbox.y + selected.run.bbox.height / 2.0,
            ),
            edit_revision: session.edit_revision,
        });
        self.0.set(marker);
    }
}

/// Selects the remembered block again, from the pages as the refresh left
/// them. Called once the refresh has read them (`reload_after_refresh`).
pub(crate) fn restore(viewer: &Viewer) {
    let Some(marker) = viewer.style_toggles.reselect.0.take() else {
        return;
    };
    let current = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| {
            session.edit_revision == marker.edit_revision
                && session.selected_text_block.is_none()
                && session.content_editor.is_none()
                && session.content_selection.is_none()
        });
    if !current {
        return;
    }
    if let block::TextHit::Block(run, runs) =
        block::text_block_at(viewer, marker.page_index, marker.point)
    {
        block::select(viewer, marker.page_index, run, runs);
    }
}
