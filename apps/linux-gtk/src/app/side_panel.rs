//! Collapsible side columns: what a column's width means on either side of a
//! divider, the slot that lets a column fold away without leaving the widget
//! tree, and the wiring that keeps a divider drag and a toolbar toggle saying
//! the same thing.
//!
//! Used by `build_ui` for both side columns — the page thumbnails on the left
//! and the tools panel on the right.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Orientation, Paned, Stack, ToggleButton};

/// The slot page holding the real column.
const OPEN: &str = "open";
/// The slot page holding nothing, whose minimum width is zero.
const COLLAPSED: &str = "collapsed";

/// Which side of a `Paned` a collapsible column sits on. The two cases differ
/// only in how a divider position turns into that column's width.
#[derive(Clone, Copy)]
pub(crate) enum Column {
    /// The column is the `Paned`'s start child, so the position *is* its width.
    Start,
    /// The column is the end child, so it gets whatever the position leaves over.
    End,
}

impl Column {
    /// The column's own width, from the divider position and the `Paned`'s
    /// total width.
    ///
    /// Plain numbers rather than a `&Paned` so the arithmetic — the part a
    /// sign error would silently invert — can be tested without a realized
    /// window to measure.
    pub(crate) fn width(self, position: i32, total: i32) -> i32 {
        match self {
            Column::Start => position,
            Column::End => total - position,
        }
    }

    /// The inverse: the divider position that gives the column `width`.
    fn position_for(self, width: i32, total: i32) -> i32 {
        match self {
            Column::Start => width,
            Column::End => (total - width).max(0),
        }
    }
}

/// Wraps a side column in the slot that lets it collapse.
///
/// The obvious way to collapse a column is `set_visible(false)` on it, and it
/// is wrong in a way that only shows up in the hand rather than in a test:
/// **GTK4 drops a `Paned`'s separator as soon as one of its children is
/// hidden.** So the gesture that puts the column away — dragging the divider
/// out to the edge — is also the gesture that can no longer bring it back.
/// The column still reopens from its toolbar toggle, and every assertion
/// about it still passes, but the user's hand is on the divider, not on the
/// toolbar, and from there the column is simply gone.
///
/// So the slot stays visible and swaps to an empty page instead. The divider
/// survives with a real column behind it, the canvas still gets all but a few
/// pixels, and pulling the divider back in is once again how you reopen the
/// column — the gesture is its own inverse, which is the only reason a user
/// would expect it to work.
///
/// `hhomogeneous(false)` is load-bearing: a `Stack` otherwise measures the
/// widest page it holds, which would be the panel itself, and the empty
/// page's zero would never reach the `Paned`.
pub(crate) fn collapsible(panel: &impl IsA<gtk::Widget>) -> Stack {
    let slot = Stack::new();
    slot.set_hhomogeneous(false);
    slot.set_vhomogeneous(false);
    slot.add_named(panel, Some(OPEN));
    slot.add_named(&GtkBox::new(Orientation::Vertical, 0), Some(COLLAPSED));
    slot.set_visible_child_name(OPEN);
    slot
}

/// Ties a collapsible column's slot to the toggle that reports whether it is
/// open, in both directions.
///
/// One rule runs the whole thing: **a column is open exactly when the divider
/// leaves it room to draw in.** Drag the divider past that and it folds; drag
/// back across it and it unfolds; the toggle is the same fact spelled out in
/// the toolbar, so clicking it and dragging cannot come to disagree.
///
/// That rule is also what makes the in-between state unreachable. A `Paned`
/// whose `shrink_*_child` is left at GTK's default `true` will happily
/// allocate a child *less* than its minimum, and GTK's answer to being handed
/// less than the minimum is not to shrink the contents — it is to cut them
/// off at the edge. Dragging the canvas/tools divider right used to do
/// exactly that: the panel's controls disappeared off the side of the window
/// a column at a time. Pinning `shrink_*_child` to `false` instead only jams
/// the handle, which leaves no gesture meaning "put this away" — and wanting
/// the canvas back is the only reason to drag that far. So the drag stays
/// free and lands on one of two honest states: a column wide enough to draw,
/// or no column.
///
/// Reopening from the toolbar restores the width the column last had while it
/// was open, rather than a fixed default, so putting a column away and
/// bringing it back does not quietly discard a layout the user set on purpose.
pub(crate) fn connect(paned: &Paned, column: Column, slot: &Stack, toggle: &ToggleButton) {
    let panel = slot
        .child_by_name(OPEN)
        .expect("a collapsible slot always holds the column it was built around");
    let last_open_width = Rc::new(Cell::new(0));

    paned.connect_position_notify({
        let panel = panel.clone();
        let toggle = toggle.clone();
        let last_open_width = last_open_width.clone();
        move |paned| {
            // Before the first allocation `width()` is zero, which would read
            // as "no room for anything" and fold both columns on the way up.
            if paned.width() <= 0 {
                return;
            }
            let width = column.width(paned.position(), paned.width());
            if width >= panel.measure(Orientation::Horizontal, -1).0 {
                last_open_width.set(width);
                // `set_active` only emits when the answer changes, so a drag
                // inside the open range is not a stream of redundant toggles.
                toggle.set_active(true);
                return;
            }
            toggle.set_active(false);
            // Snap the divider the rest of the way shut. Folding the slot
            // alone is not closing the column: the `Paned` still allocates it
            // whatever width the drag left behind, so the column becomes a
            // strip of empty panel between the canvas and the window edge —
            // which reads as a panel that broke, not one that closed.
            //
            // Doing this from inside `position-notify` is what makes it feel
            // like a snap rather than a jump. A drag in progress keeps setting
            // the position from the pointer, and this keeps pulling it back to
            // the edge, so the handle sticks shut while the pointer is in the
            // dead zone and lets go the moment it comes back out — the same
            // behaviour a code editor's sidebar has.
            let shut = column.position_for(0, paned.width());
            if paned.position() != shut {
                paned.set_position(shut);
            }
        }
    });

    toggle.connect_toggled({
        let paned = paned.clone();
        let slot = slot.clone();
        move |toggle| {
            let open = toggle.is_active();
            slot.set_visible_child_name(if open { OPEN } else { COLLAPSED });
            if !open {
                return;
            }
            // Reopening from the toolbar has to move the divider; reopening
            // by dragging back across the threshold already did, and moving
            // it again would yank the column out from under the pointer
            // mid-drag. Both arrive here, so the width itself decides which
            // one this is.
            let minimum = panel.measure(Orientation::Horizontal, -1).0;
            if column.width(paned.position(), paned.width()) >= minimum {
                return;
            }
            let width = last_open_width.get().max(minimum);
            paned.set_position(column.position_for(width, paned.width()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant reopening a column rests on: asking for a width and
    /// reading it back has to give the same number, or a column put away at
    /// 240px comes back somewhere else. The end child is where this can go
    /// wrong — its width runs backwards from the position.
    #[test]
    fn a_width_survives_the_round_trip_through_a_divider_position() {
        for column in [Column::Start, Column::End] {
            let position = column.position_for(240, 900);

            assert_eq!(column.width(position, 900), 240);
        }
    }

    /// The two columns read the same divider from opposite sides — the whole
    /// reason this is an enum and not one formula.
    #[test]
    fn the_two_columns_read_a_divider_from_opposite_sides() {
        assert_eq!(Column::Start.width(300, 900), 300);
        assert_eq!(Column::End.width(300, 900), 600);
    }

    /// A column wider than the `Paned` has nowhere to put the divider but the
    /// far edge; a negative position would be clamped by GTK anyway, and
    /// reads as "the other pane is wider than the window".
    #[test]
    fn an_oversized_end_column_pins_the_divider_at_the_start_edge() {
        assert_eq!(Column::End.position_for(1_200, 900), 0);
    }

    /// The property `set_visible(false)` cannot give: a collapsed column can
    /// still be squeezed to nothing, *and* the slot holding it stays visible,
    /// which is what keeps the `Paned`'s separator on screen to drag back.
    #[gtk::test]
    fn a_collapsed_slot_stays_visible_and_asks_for_no_width() {
        let slot = collapsible(&column_panel());

        let open = slot.measure(Orientation::Horizontal, -1).0;
        slot.set_visible_child_name(COLLAPSED);
        let collapsed = slot.measure(Orientation::Horizontal, -1).0;

        assert!(open > 0, "an open slot must ask for the column's width");
        assert_eq!(collapsed, 0, "a collapsed slot must ask for nothing");
        assert!(
            slot.is_visible(),
            "the slot itself must stay visible or the Paned drops its separator"
        );
    }

    /// A stand-in column with a real, non-trivial minimum width.
    fn column_panel() -> GtkBox {
        let panel = GtkBox::new(Orientation::Vertical, 0);
        panel.append(&gtk::Button::with_label("Previous annotation"));
        panel
    }

    /// Pumps the main loop until `ready` holds, or gives up after a deadline
    /// and lets the caller's assertion do the failing.
    ///
    /// A widget has no allocation to read until a frame has been laid out, so
    /// this has to let the frame clock run — but it drains without blocking
    /// and sleeps between passes rather than parking in a blocking
    /// `iteration`. A blocking wait for a condition that never comes true
    /// does not fail, it hangs, and a hung test says nothing about what
    /// broke.
    fn settle_until(ready: impl Fn() -> bool) {
        let context = gtk::glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if ready() {
                return;
            }
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// A realized `Paned` carrying one collapsible end column, plus its
    /// toggle. Returned with its window so the caller can close it.
    fn realized_shell() -> (gtk::Window, Paned, Stack, ToggleButton, i32) {
        let canvas = GtkBox::new(Orientation::Vertical, 0);
        canvas.set_hexpand(true);
        let panel = column_panel();
        let slot = collapsible(&panel);

        let paned = Paned::new(Orientation::Horizontal);
        paned.set_start_child(Some(&canvas));
        paned.set_end_child(Some(&slot));
        paned.set_resize_start_child(true);
        paned.set_resize_end_child(false);
        paned.set_position(600);

        let toggle = ToggleButton::new();
        toggle.set_active(true);
        connect(&paned, Column::End, &slot, &toggle);

        let window = gtk::Window::new();
        window.set_default_size(800, 600);
        window.set_child(Some(&paned));
        window.present();
        settle_until(|| paned.width() > 0);
        assert!(paned.width() > 0, "the paned never got an allocation");

        let minimum = panel.measure(Orientation::Horizontal, -1).0;
        (window, paned, slot, toggle, minimum)
    }

    /// Folding has to take the column's *width* with it, not just its
    /// contents. Leaving the divider where the drag stopped left an empty
    /// strip of panel between the canvas and the window edge — a column that
    /// looks broken rather than closed.
    ///
    /// Dragged to just inside the dead zone, one pixel short of what the
    /// column needs, so this fails if the snap is missing rather than only if
    /// the fold is.
    #[gtk::test]
    fn folding_snaps_the_divider_shut_instead_of_leaving_an_empty_strip() {
        let (window, paned, _slot, toggle, minimum) = realized_shell();

        paned.set_position(paned.width() - (minimum - 1));
        settle_until(|| !toggle.is_active());

        assert!(!toggle.is_active(), "a column too narrow to draw must fold");
        assert_eq!(
            Column::End.width(paned.position(), paned.width()),
            0,
            "a folded column must take no width at all"
        );

        window.close();
    }

    /// The regression the user actually hit: the column folded away on an
    /// over-drag and then could not be dragged back, because hiding a `Paned`
    /// child takes its separator with it. Dragging back in must reopen it —
    /// the gesture has to be its own inverse.
    #[gtk::test]
    fn dragging_the_divider_back_in_reopens_a_folded_column() {
        let (window, paned, slot, toggle, minimum) = realized_shell();

        // Out to the edge: fold.
        paned.set_position(paned.width());
        settle_until(|| !toggle.is_active());
        assert!(!toggle.is_active(), "over-dragging must fold the column");
        assert_eq!(slot.visible_child_name().as_deref(), Some(COLLAPSED));
        assert!(
            slot.is_visible(),
            "the slot must stay visible so the divider keeps its separator"
        );

        // And back in, by the same gesture.
        paned.set_position(paned.width() - minimum);
        settle_until(|| toggle.is_active());

        assert!(
            toggle.is_active(),
            "dragging back in must reopen the column"
        );
        assert_eq!(slot.visible_child_name().as_deref(), Some(OPEN));

        window.close();
    }

    #[gtk::test]
    fn a_folded_column_also_reopens_from_its_toggle_wide_enough_to_draw() {
        let (window, paned, slot, toggle, minimum) = realized_shell();

        paned.set_position(paned.width());
        settle_until(|| !toggle.is_active());
        toggle.set_active(true);
        settle_until(|| slot.visible_child_name().as_deref() == Some(OPEN));

        let reopened = Column::End.width(paned.position(), paned.width());
        assert!(
            reopened >= minimum,
            "reopened at {reopened}px but the column needs {minimum}px, so it is clipped again"
        );

        window.close();
    }
}
