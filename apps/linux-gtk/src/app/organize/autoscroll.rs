//! Auto-scroll at the edges of the Organize screen's scrollers while a page
//! or a block is being dragged (checklist §10).
//!
//! ## Why a drag needs this at all
//!
//! Both views are taller than their viewport on any document worth
//! organizing, and a drag holds the pointer grab for its whole duration: the
//! scrollbar cannot be reached, and the wheel is not delivered to the
//! scroller underneath. So a destination that is off-screen when the gesture
//! starts is a destination the gesture cannot reach — dragging page 40 to the
//! front meant dropping it as far up as the viewport allowed, letting go,
//! scrolling, and picking it up again. The grid's own drop handling was
//! already correct for every slot; what was missing was a way to *reach* the
//! slot you wanted.
//!
//! ## Why a `DropControllerMotion` and not the existing drop targets
//!
//! The obvious hook is [`super::grid::drop`]'s `connect_motion`, which is
//! already tracking the pointer. Two things argue against it. Its `y` is in
//! `FlowBox` coordinates — the full scrollable height, not the visible box —
//! so "near the top edge" would have to be reconstructed from the vertical
//! adjustment on every event. And the Documents view has no equivalent hook
//! to share: its drop targets are the slim gaps between cards
//! (`super::documents::gap`), each of which only hears about the few pixels
//! it occupies.
//!
//! [`DropControllerMotion`] answers both at once. It tracks a drag over a
//! widget *without* being a drop target, so attaching it to the
//! `ScrolledWindow` reports the pointer in the viewport's own coordinates and
//! leaves every existing target untouched. The same call then serves both
//! views, and neither `drop.rs` nor `gap.rs` changes by a character.
//!
//! ## Why the speed ramps instead of being one constant
//!
//! A single speed has to be either too slow to cross a long document or too
//! fast to stop on the row you want. Ramping with depth makes the edge itself
//! the throttle: rest the pointer just inside the zone to creep a row at a
//! time, push it against the edge to travel. That is the behaviour every file
//! manager and list view has trained the hand for, which is the whole point —
//! the gesture should not need explaining.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{glib, DropControllerMotion, ScrolledWindow};

/// How deep the hot zone at each edge is, at most — see [`edge_depth`] for
/// the "at most".
const EDGE_PX: f64 = 56.0;
/// What one tick moves when the pointer has only just entered a hot zone.
/// Deliberately a fraction of a row: the shallow end of the zone is for
/// nudging the next row into view, not for travelling.
const MIN_STEP_PX: f64 = 2.0;
/// What one tick moves when the pointer is pinned against the very edge. A
/// page card is `grid::CARD_HEIGHT_PX` (180) tall, so this is a row roughly
/// every eleven ticks — a fifth of a second — and crossing a hundred-page
/// grid is a couple of seconds rather than a chore.
const MAX_STEP_PX: f64 = 16.0;
/// One frame at 60Hz. The step constants are per tick, so this is what turns
/// them into a speed: `MAX_STEP_PX` works out at about 1000 px/s.
const TICK: Duration = Duration::from_millis(16);

/// Gives `scroll` an edge that scrolls while a drag hovers over it.
///
/// Called once per scroller at build time. Nothing here is view-specific: the
/// Pages grid and the Documents list get the same treatment from the same
/// call, because what the pointer is dragging is the drop targets' business
/// and not this module's.
pub(super) fn connect(scroll: &ScrolledWindow) {
    // What the current pointer position asks for, in pixels per tick, read by
    // whichever timer happens to be running. Shared rather than passed in, so
    // that a pointer moving between the two hot zones — or out of them —
    // redirects the timer already ticking instead of racing a second one
    // against it.
    let step = Rc::new(Cell::new(0.0));
    let ticking = Rc::new(Cell::new(false));

    let motion = DropControllerMotion::new();
    motion.connect_motion({
        let scroll = scroll.clone();
        let step = step.clone();
        let ticking = ticking.clone();
        move |_, _, y| {
            step.set(step_for(y, f64::from(scroll.height())));
            arm(&scroll, &step, &ticking);
        }
    });
    // Both the drag leaving the scroller and the drop landing on it end the
    // gesture as far as this is concerned, and `leave` covers both: GTK emits
    // it when the pointer goes, and again when the drag finishes.
    motion.connect_leave({
        let step = step.clone();
        move |_| step.set(0.0)
    });
    scroll.add_controller(motion);
}

/// Starts the tick that walks the adjustment, unless one is already running
/// or there is nothing for it to do.
///
/// The guard is what keeps a drag crossing the hot zone from stacking one
/// timer per motion event: `ticking` is the timer's own lifetime, set here and
/// cleared by the tick that decides to stop.
fn arm(scroll: &ScrolledWindow, step: &Rc<Cell<f64>>, ticking: &Rc<Cell<bool>>) {
    if ticking.get() || step.get() == 0.0 {
        return;
    }
    ticking.set(true);
    glib::timeout_add_local(TICK, {
        let scroll = scroll.clone();
        let step = step.clone();
        let ticking = ticking.clone();
        move || {
            if scroll_by(&scroll, step.get()) {
                return glib::ControlFlow::Continue;
            }
            ticking.set(false);
            glib::ControlFlow::Break
        }
    });
}

/// How deep the hot zone is on a viewport `height` tall.
///
/// A third of the viewport, capped at [`EDGE_PX`]. The cap is what makes the
/// zone a constant on any normal window; the third is what stops the two
/// zones meeting in the middle on a short one — see this module's tests for
/// why an overlap would be worse than no auto-scroll at all.
fn edge_depth(height: f64) -> f64 {
    EDGE_PX.min(height / 3.0)
}

/// What one tick should move the scroller, given the pointer at `y` in a
/// viewport `height` tall: negative towards the start of the document,
/// positive towards its end, zero anywhere between the two hot zones.
fn step_for(y: f64, height: f64) -> f64 {
    let edge = edge_depth(height);
    if edge <= 0.0 {
        return 0.0;
    }
    if y < edge {
        -speed(edge - y, edge)
    } else if y > height - edge {
        speed(y - (height - edge), edge)
    } else {
        0.0
    }
}

/// The ramp: [`MIN_STEP_PX`] at the shallow end of a hot zone, growing to
/// [`MAX_STEP_PX`] at the edge itself and no further — a `depth` past `edge`
/// is a pointer that has left the widget, which is still just "as fast as it
/// goes".
fn speed(depth: f64, edge: f64) -> f64 {
    let ratio = (depth / edge).clamp(0.0, 1.0);
    MIN_STEP_PX + (MAX_STEP_PX - MIN_STEP_PX) * ratio
}

/// Moves `scroll` by `step`, clamped to the content it actually has, and
/// reports whether that changed anything.
///
/// The return value is the tick's stop condition rather than a courtesy: a
/// pointer held against the top edge of a document already scrolled to the
/// top asks for a step every frame for as long as the drag lasts, and
/// answering "nothing moved" is how the timer learns to stand down instead of
/// burning a wakeup per frame on an adjustment that cannot change.
fn scroll_by(scroll: &ScrolledWindow, step: f64) -> bool {
    if step == 0.0 {
        return false;
    }
    let adjustment = scroll.vadjustment();
    // The last position that still shows a full viewport. `max` guards the
    // case where the content is shorter than the viewport and the ceiling
    // would otherwise fall below the floor.
    let ceiling = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
    let target = (adjustment.value() + step).clamp(adjustment.lower(), ceiling);
    if target == adjustment.value() {
        return false;
    }
    adjustment.set_value(target);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::{Adjustment, PolicyType};

    /// A comfortable viewport: more than three times [`EDGE_PX`] tall, so
    /// both hot zones get their full depth and the band between them is the
    /// bulk of the screen.
    const HEIGHT: f64 = 600.0;

    #[test]
    fn the_middle_of_the_viewport_does_not_scroll() {
        assert_eq!(step_for(HEIGHT / 2.0, HEIGHT), 0.0, "dead centre");
        assert_eq!(
            step_for(EDGE_PX + 1.0, HEIGHT),
            0.0,
            "just below the top zone"
        );
        assert_eq!(
            step_for(HEIGHT - EDGE_PX - 1.0, HEIGHT),
            0.0,
            "just above the bottom zone"
        );
    }

    #[test]
    fn each_edge_scrolls_towards_the_content_the_pointer_is_reaching_for() {
        assert!(step_for(4.0, HEIGHT) < 0.0, "the top edge scrolls up");
        assert!(
            step_for(HEIGHT - 4.0, HEIGHT) > 0.0,
            "the bottom edge scrolls down"
        );
    }

    #[test]
    fn the_deeper_into_an_edge_the_pointer_is_the_faster_it_scrolls() {
        assert!(
            step_for(2.0, HEIGHT).abs() > step_for(EDGE_PX - 2.0, HEIGHT).abs(),
            "the top zone ramps up towards the edge"
        );
        assert!(
            step_for(HEIGHT - 2.0, HEIGHT) > step_for(HEIGHT - EDGE_PX + 2.0, HEIGHT),
            "and so does the bottom one"
        );
    }

    /// A drag can leave the scroller entirely — the pointer is over the
    /// window's header, or off the bottom of the screen — and the last
    /// motion event before it does reports a `y` outside the allocation.
    /// That is still "as fast as it goes", not faster.
    #[test]
    fn a_pointer_past_an_edge_scrolls_no_faster_than_one_resting_on_it() {
        assert_eq!(step_for(-50.0, HEIGHT), step_for(0.0, HEIGHT));
        assert_eq!(step_for(HEIGHT + 50.0, HEIGHT), step_for(HEIGHT, HEIGHT));
    }

    /// With a fixed hot-zone depth, a short viewport would have its two zones
    /// overlap, and every position on it would be an edge — the grid would
    /// scroll wherever the pointer went, including while it was being held
    /// still over the slot the user actually wanted.
    #[test]
    fn a_viewport_too_short_for_two_full_hot_zones_keeps_a_neutral_middle() {
        let height = 90.0;
        assert!(
            edge_depth(height) < EDGE_PX,
            "the zones must give way before they meet"
        );

        assert_eq!(step_for(height / 2.0, height), 0.0);
        assert!(step_for(2.0, height) < 0.0);
        assert!(step_for(height - 2.0, height) > 0.0);
    }

    #[test]
    fn a_viewport_with_no_height_yet_never_scrolls() {
        assert_eq!(step_for(0.0, 0.0), 0.0);
        assert_eq!(step_for(10.0, 0.0), 0.0);
    }

    /// A scroller holding 2000px of content in a 600px viewport, parked in
    /// the middle — the state a drag that needs to reach either end starts
    /// from.
    fn a_scroller() -> ScrolledWindow {
        let scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .build();
        scroll.set_vadjustment(Some(&Adjustment::new(
            500.0, 0.0, 2000.0, 10.0, 100.0, HEIGHT,
        )));
        scroll
    }

    #[gtk::test]
    fn gtk_ui_a_step_moves_the_scroller_by_that_many_pixels() {
        let scroll = a_scroller();

        assert!(scroll_by(&scroll, -30.0), "it moved");
        assert_eq!(scroll.vadjustment().value(), 470.0);

        assert!(scroll_by(&scroll, 80.0));
        assert_eq!(scroll.vadjustment().value(), 550.0);
    }

    /// The timer keeps ticking while the pointer sits in a hot zone, so the
    /// end of the content is reached long before the drag is released. It
    /// must stop there rather than walk the adjustment past its own range.
    #[gtk::test]
    fn gtk_ui_a_step_stops_at_the_end_of_the_content_it_is_walking_towards() {
        let scroll = a_scroller();

        assert!(scroll_by(&scroll, -600.0), "the first step still moves");
        assert_eq!(scroll.vadjustment().value(), 0.0, "clamped at the top");
        assert!(
            !scroll_by(&scroll, -600.0),
            "and the next one has nowhere to go"
        );

        assert!(scroll_by(&scroll, 5000.0));
        assert_eq!(
            scroll.vadjustment().value(),
            2000.0 - HEIGHT,
            "the last screenful, not the last pixel"
        );
        assert!(!scroll_by(&scroll, 5000.0));
    }

    /// What the tick sees once the drag has left the scroller: nothing to do,
    /// and — since the driver reads this to decide whether to keep ticking —
    /// the signal that the timer can stop.
    #[gtk::test]
    fn gtk_ui_a_neutral_step_moves_nothing_and_reports_it() {
        let scroll = a_scroller();

        assert!(!scroll_by(&scroll, 0.0));
        assert_eq!(scroll.vadjustment().value(), 500.0);
    }
}
