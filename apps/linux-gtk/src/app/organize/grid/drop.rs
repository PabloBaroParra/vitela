//! Where a dragged page lands, and how the grid says so before it is
//! dropped (checklist §10).
//!
//! The drag carries the page's [`PageId`], never the position its card held
//! when the gesture began, and the drop resolves that id against
//! `Document.pages` as it stands at drop time. A captured index is only
//! correct while nothing else moves, and the Undo/Redo buttons sit in this
//! screen's own header.
//!
//! The destination is an *insertion slot* — `k` reads "before the page
//! currently at `k`", and `cards.len()` reads "after the last one", so a drop
//! past the final card has an answer. [`super::super::documents`] spells the
//! same idea out as slim gap widgets between its cards; that is not available
//! here, because this view's cards are the children of a homogeneous
//! `FlowBox` and a gap would have to be a child too — it would take a whole
//! grid cell and re-flow the rows around it. So the slot is derived from
//! which half of a card the pointer is over ([`slot_at`]), and
//! [`highlight_slot`] draws it on the card's near edge.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib, Box as GtkBox, DropTarget, FlowBox};
use pdf_document::PageId;

use crate::app::state::Viewer;

use super::super::command::move_page;
use super::card::renumber;

/// The accent drawn on the edge of the card the dragged page would land
/// beside — see this module's header for why the slot is shown on a card
/// rather than in a gap of its own.
const DROP_BEFORE: &str = "organize-card-drop-before";
const DROP_AFTER: &str = "organize-card-drop-after";

/// Wires the grid's drag-and-drop: one target for the whole `FlowBox` rather
/// than one per card, since the slot the page lands in is a property of where
/// the pointer is, not of which widget it happens to be over.
pub(in crate::app::organize) fn connect_drop(viewer: &Viewer) {
    // The card currently wearing a drop accent, so a motion event clears one
    // class instead of walking every card in the grid to clear all of them.
    let highlighted: Rc<RefCell<Option<GtkBox>>> = Rc::new(RefCell::new(None));

    let target = DropTarget::new(u32::static_type(), gdk::DragAction::MOVE);
    target.connect_motion({
        let viewer = viewer.clone();
        let highlighted = highlighted.clone();
        move |_, x, y| {
            highlight_slot(&viewer, &highlighted, x, y);
            gdk::DragAction::MOVE
        }
    });
    target.connect_leave({
        let highlighted = highlighted.clone();
        move |_| clear_highlight(&highlighted)
    });
    target.connect_drop({
        let viewer = viewer.clone();
        move |_, value, x, y| {
            clear_highlight(&highlighted);
            handle_drop(&viewer, value, x, y)
        }
    });
    viewer.organize.grid.add_controller(target);
}

/// The grid's single drop handler: resolves the dragged page's id and the
/// pointer's insertion slot against the page order as it is *now*, records
/// the move as `Command::MovePage`, then moves the card to match.
pub(in crate::app::organize) fn handle_drop(
    viewer: &Viewer,
    value: &glib::Value,
    x: f64,
    y: f64,
) -> bool {
    let Ok(id) = value.get::<u32>() else {
        return false;
    };
    let Some(from) = page_position(viewer, PageId(id)) else {
        return false;
    };
    let cards = &viewer.organize.cards;
    // A grid that has drifted from the model has no position to move a card
    // to; a rebuild is the only honest answer, and `fill_missing_thumbnails`
    // is where it happens.
    if cards.len() != page_count(viewer) {
        return false;
    }
    let slot = slot_at(&card_rects(&viewer.organize.grid), x, y);
    let Some(to) = destination(from, slot, cards.len()) else {
        return false;
    };

    if !move_page(viewer, from, to) {
        return false;
    }
    reorder_cards(viewer, from, to);
    true
}

/// Where the page carrying `id` sits in `Document.pages` right now.
///
/// Read from the model rather than from `cards` on purpose: the model is what
/// [`move_page`] indexes into, so resolving the drag's payload anywhere else
/// would leave the command aimed at a position nothing had agreed on.
fn page_position(viewer: &Viewer, id: PageId) -> Option<usize> {
    let state = viewer.state.borrow();
    let model = state.session.as_ref()?.document_model.as_ref()?;
    model.pages.iter().position(|page| page.id == id)
}

fn page_count(viewer: &Viewer) -> usize {
    let state = viewer.state.borrow();
    state
        .session
        .as_ref()
        .and_then(|session| session.document_model.as_ref())
        .map_or(0, |model| model.pages.len())
}

/// One grid cell's place in the `FlowBox`, in the same coordinates a drop
/// reports its pointer in.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CardRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Every card's cell, in `cards` order — which is also the order the grid
/// lays them out in, since [`super::sort_position`] is what sorts it.
fn card_rects(grid: &FlowBox) -> Vec<CardRect> {
    let mut rects = Vec::new();
    let mut index = 0;
    while let Some(child) = grid.child_at_index(index) {
        let allocation = child.allocation();
        rects.push(CardRect {
            x: f64::from(allocation.x()),
            y: f64::from(allocation.y()),
            width: f64::from(allocation.width()),
            height: f64::from(allocation.height()),
        });
        index += 1;
    }
    rects
}

/// The insertion slot the pointer is over: `k` means "before the page at
/// `k`", `cells.len()` means "after the last page".
///
/// Rows are walked in layout order, so the first cell whose bottom edge is
/// below the pointer identifies the pointer's row, and the walk stops at the
/// end of that row — without that stop, a pointer resting in the right half
/// of a row's last card would be claimed again by the left half of the card
/// below it. The two readings are the same slot anyway ("after this row's
/// last" and "before the next row's first" are one position), which is what
/// makes the column gaps and the row gaps resolve without a special case.
///
/// A pointer below every row — the empty space under a part-filled last row —
/// is the "after the last card" slot, and is the reason a drop past the final
/// card is not simply dropped on the floor.
fn slot_at(cells: &[CardRect], x: f64, y: f64) -> usize {
    let Some(first) = cells.iter().position(|cell| y < cell.y + cell.height) else {
        return cells.len();
    };
    let row = cells[first].y;
    let mut slot = first;
    for (index, cell) in cells.iter().enumerate().skip(first) {
        if cell.y > row {
            break;
        }
        if x < cell.x + cell.width / 2.0 {
            return index;
        }
        slot = index + 1;
    }
    slot
}

/// Where the page at `from` must be re-inserted for it to land in `slot` —
/// the `to` of `Command::MovePage`, which re-inserts into a page list one
/// shorter than the one the slot was read against.
///
/// `None` for the two slots that would not move it: its own, and the one
/// immediately after it.
fn destination(from: usize, slot: usize, len: usize) -> Option<usize> {
    if from >= len || slot > len || slot == from || slot == from + 1 {
        return None;
    }
    Some(if slot > from { slot - 1 } else { slot })
}

/// Paints the drop accent on the edge of the card the page would land beside,
/// and takes it off the card that wore it before.
fn highlight_slot(viewer: &Viewer, highlighted: &Rc<RefCell<Option<GtkBox>>>, x: f64, y: f64) {
    let cards = viewer.organize.cards.snapshot();
    let slot = slot_at(&card_rects(&viewer.organize.grid), x, y);
    let Some((card, class)) = (match cards.get(slot) {
        Some(card) => Some((card, DROP_BEFORE)),
        // Past the last card: the accent goes on the trailing edge of the one
        // the page would land after.
        None => cards.last().map(|card| (card, DROP_AFTER)),
    }) else {
        clear_highlight(highlighted);
        return;
    };

    let mut previous = highlighted.borrow_mut();
    if let Some(previous) = previous.as_ref() {
        if previous == &card.root && card.root.has_css_class(class) {
            return;
        }
        previous.remove_css_class(DROP_BEFORE);
        previous.remove_css_class(DROP_AFTER);
    }
    card.root.remove_css_class(DROP_BEFORE);
    card.root.remove_css_class(DROP_AFTER);
    card.root.add_css_class(class);
    *previous = Some(card.root.clone());
}

fn clear_highlight(highlighted: &Rc<RefCell<Option<GtkBox>>>) {
    if let Some(card) = highlighted.borrow_mut().take() {
        card.remove_css_class(DROP_BEFORE);
        card.remove_css_class(DROP_AFTER);
    }
}

/// Moves the card at `from` to `to`, mirroring what `Command::MovePage` just
/// did to `Document.pages` (see `state::Cards::move_card` for the mirroring
/// itself).
///
/// Nothing is re-rendered and no widget changes parent. `cards` is the sort
/// key (see `super::super::build_organize_panel`), so reordering the vector
/// *is* the reorder, and `invalidate_sort` is what tells the grid to read it
/// again. Each card keeps the `Picture` it was built with, which matters
/// twice over: the thumbnail it already painted is still that page's
/// thumbnail, and a render still in flight is still aimed at the card it was
/// started for.
///
/// The obvious alternative — pulling the card out of the grid and inserting
/// it at the new index — is not available. `gtk_flow_box_remove` on this GTK4
/// build does not release a widget's parent: not synchronously inside this
/// callback, and not a full main-loop iteration later via
/// `glib::idle_add_local_once` either (both tried, reproduced with 2 cards,
/// from=0 to=1 — moving a card to become the *last* one). Any `insert` or
/// `append` of that widget back into the same grid then fails
/// `gtk_flow_box_child_set_child`'s assertion and silently drops the card.
/// Sorting never asks the grid to move anything, so it never meets that bug.
fn reorder_cards(viewer: &Viewer, from: usize, to: usize) {
    viewer.organize.cards.move_card(from, to);
    viewer.organize.grid.invalidate_sort();
    renumber(&viewer.organize.cards);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three cards per row, 100 wide and 50 tall, with 10px of spacing —
    /// close enough to the real `FlowBox` layout that the gaps between
    /// columns and between rows are both real gaps.
    fn cells(count: usize) -> Vec<CardRect> {
        (0..count)
            .map(|index| CardRect {
                x: (index % 3) as f64 * 110.0,
                y: (index / 3) as f64 * 60.0,
                width: 100.0,
                height: 50.0,
            })
            .collect()
    }

    #[test]
    fn the_half_of_a_card_the_pointer_is_over_picks_the_side_it_lands_on() {
        let cells = cells(3);

        assert_eq!(slot_at(&cells, 10.0, 25.0), 0, "left half of the first");
        assert_eq!(slot_at(&cells, 90.0, 25.0), 1, "right half of the first");
        assert_eq!(slot_at(&cells, 120.0, 25.0), 1, "left half of the second");
        assert_eq!(slot_at(&cells, 200.0, 25.0), 2, "right half of the second");
    }

    #[test]
    fn the_space_between_two_columns_resolves_to_the_slot_between_them() {
        let cells = cells(3);

        // x = 105 is in neither card: it is the column spacing. Both
        // neighbours agree on what that position means.
        assert_eq!(slot_at(&cells, 105.0, 25.0), 1);
    }

    #[test]
    fn a_row_s_trailing_edge_and_the_next_row_s_leading_edge_are_one_slot() {
        let cells = cells(6);

        assert_eq!(slot_at(&cells, 290.0, 25.0), 3, "after the first row");
        assert_eq!(slot_at(&cells, 10.0, 85.0), 3, "before the second row");
        // The row spacing itself belongs to the row below it, which is the
        // same slot again.
        assert_eq!(slot_at(&cells, 10.0, 55.0), 3);
    }

    #[test]
    fn the_empty_space_past_the_last_card_is_the_end_of_the_list() {
        let cells = cells(4);

        assert_eq!(slot_at(&cells, 200.0, 85.0), 4, "beside the last card");
        assert_eq!(slot_at(&cells, 10.0, 400.0), 4, "well below every row");
        assert_eq!(slot_at(&[], 10.0, 10.0), 0, "an empty grid");
    }

    #[test]
    fn a_slot_becomes_the_move_s_destination_in_the_shortened_page_list() {
        assert_eq!(destination(0, 3, 3), Some(2), "to the very end");
        assert_eq!(destination(2, 0, 3), Some(0), "to the very front");
        assert_eq!(destination(0, 2, 3), Some(1));
        assert_eq!(destination(2, 1, 3), Some(1));
    }

    #[test]
    fn the_two_slots_that_would_not_move_a_page_are_refused() {
        assert_eq!(destination(1, 1, 3), None, "its own slot");
        assert_eq!(destination(1, 2, 3), None, "the slot just after it");
    }

    #[test]
    fn a_slot_or_page_outside_the_list_is_refused() {
        assert_eq!(destination(0, 4, 3), None);
        assert_eq!(destination(3, 0, 3), None);
    }
}
