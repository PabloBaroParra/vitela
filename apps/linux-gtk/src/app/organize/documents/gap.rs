//! The drop positions between the Documents view's block cards, and the
//! move they resolve to.
//!
//! One gap more than there are cards: before the first block, between every
//! pair, and after the last — see [`super`]'s header for why the positions
//! are targets of their own rather than halves of a card.

use gtk::prelude::*;
use gtk::{gdk, Box as GtkBox, DropTarget, Orientation};
use pdf_document::PageId;

use crate::app::state::Viewer;

use super::super::command::move_block;
use super::{rebuilt, rows, Row};

/// One drop position, at `slot` — "before the block currently at `slot`",
/// with `slot == rows.len()` meaning "after the last one".
///
/// The highlight is added on enter and removed on both leave and drop, so a
/// gap the pointer merely passed through never stays lit.
pub(super) fn build_gap(viewer: &Viewer, slot: usize) -> GtkBox {
    let gap = GtkBox::new(Orientation::Horizontal, 0);
    gap.add_css_class("organize-gap");
    gap.set_hexpand(true);

    let target = DropTarget::new(u32::static_type(), gdk::DragAction::MOVE);
    target.connect_enter({
        let gap = gap.clone();
        move |_, _, _| {
            gap.add_css_class("organize-gap-active");
            gdk::DragAction::MOVE
        }
    });
    target.connect_leave({
        let gap = gap.clone();
        move |_| gap.remove_css_class("organize-gap-active")
    });
    target.connect_drop({
        let viewer = viewer.clone();
        let gap = gap.clone();
        move |_, value, _, _| {
            gap.remove_css_class("organize-gap-active");
            let Ok(anchor) = value.get::<u32>() else {
                return false;
            };
            drop_block(&viewer, PageId(anchor), slot)
        }
    });
    gap.add_controller(target);
    gap
}

/// Moves the block anchored at `anchor` into `slot`, resolving both against
/// the blocks as they are *now* rather than as they were when the drag
/// started.
pub(in crate::app::organize) fn drop_block(viewer: &Viewer, anchor: PageId, slot: usize) -> bool {
    let Some((rows, _)) = rows(viewer) else {
        return false;
    };
    let Some(from_block) = rows.iter().position(|row| row.anchor == anchor) else {
        return false;
    };
    let Some(to) = destination(&rows, from_block, slot) else {
        return false;
    };
    rebuilt(
        viewer,
        move_block(viewer, rows[from_block].start, rows[from_block].count, to),
    )
}

/// Where the block at `from_block` must start for it to land in `slot`, in
/// `Document.pages` positions — the `to` of `Command::MovePages`.
///
/// `None` for the two slots that would not move it: the one it already
/// starts at, and the one immediately after it.
///
/// A slot past the block counts the pages *without* the block in them,
/// because `MovePages` re-inserts into the order the removal leaves behind.
fn destination(rows: &[Row], from_block: usize, slot: usize) -> Option<usize> {
    if from_block >= rows.len() || slot > rows.len() || slot == from_block || slot == from_block + 1
    {
        return None;
    }
    let before: usize = rows[..slot].iter().map(|row| row.count).sum();
    Some(if slot > from_block {
        before - rows[from_block].count
    } else {
        before
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_of(counts: &[usize]) -> Vec<Row> {
        let mut start = 0;
        counts
            .iter()
            .enumerate()
            .map(|(position, count)| {
                let row = Row {
                    anchor: PageId(position as u32),
                    name: format!("block-{position}"),
                    part: None,
                    start,
                    count: *count,
                    cover: None,
                };
                start += count;
                row
            })
            .collect()
    }

    #[test]
    fn moving_a_block_earlier_lands_on_the_slot_s_own_page_offset() {
        let rows = rows_of(&[2, 3, 1]);

        assert_eq!(destination(&rows, 2, 0), Some(0), "to the very front");
        assert_eq!(destination(&rows, 2, 1), Some(2), "between the first two");
        assert_eq!(destination(&rows, 1, 0), Some(0));
    }

    #[test]
    fn moving_a_block_later_counts_the_pages_it_leaves_behind() {
        let rows = rows_of(&[2, 3, 1]);

        // Slot 2 is "before the third block": pages 0..5 precede it now, but
        // the moving block's own 2 are among them and will not be once it
        // has been lifted out.
        assert_eq!(destination(&rows, 0, 2), Some(3));
        assert_eq!(destination(&rows, 0, 3), Some(4), "to the very end");
        assert_eq!(destination(&rows, 1, 3), Some(3));
    }

    #[test]
    fn the_two_slots_that_would_not_move_a_block_are_refused() {
        let rows = rows_of(&[2, 3, 1]);

        assert_eq!(destination(&rows, 1, 1), None, "its own slot");
        assert_eq!(destination(&rows, 1, 2), None, "the slot just after it");
    }

    #[test]
    fn a_slot_or_block_outside_the_list_is_refused() {
        let rows = rows_of(&[2, 3]);

        assert_eq!(destination(&rows, 0, 3), None);
        assert_eq!(destination(&rows, 2, 0), None);
    }
}
