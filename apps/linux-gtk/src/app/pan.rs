//! Hand-tool panning: hold Space and drag with the primary button, or drag
//! with the middle button, to move the document under the pointer.
//!
//! Input only. The pan writes the scroller's adjustments and nothing else, so
//! layout, rendering and the reading position follow the same path a
//! scrollbar drag takes.

mod math;

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gdk, glib, EventControllerKey, EventControllerMotion, EventSequenceState, GestureDrag,
    PropagationPhase,
};

use super::annotations;
use super::state::Viewer;

/// Shown while Space is held over the document.
const GRAB: &str = "grab";
/// Shown while the document is being dragged.
const GRABBING: &str = "grabbing";

pub(super) fn connect(window: &gtk::ApplicationWindow, page_area: &gtk::Overlay, viewer: &Viewer) {
    let hand = Rc::new(Hand::new(viewer));

    // Whether Space should arm the hand at all: only with the pointer over the
    // document, so Space still activates a focused toolbar button elsewhere.
    let hover = EventControllerMotion::new();
    hover.set_propagation_phase(PropagationPhase::Capture);
    page_area.add_controller(hover.clone());

    // On the window, in the capture phase: the focused widget would otherwise
    // see Space first, and a focused button would activate on it. Text fields
    // are left alone by `press_space` itself, so they still get every Space.
    let keys = EventControllerKey::new();
    keys.set_propagation_phase(PropagationPhase::Capture);
    keys.connect_key_pressed({
        let hand = hand.clone();
        let page_area = page_area.clone();
        move |_, key, _, _| {
            if key != gdk::Key::space {
                return glib::Propagation::Proceed;
            }
            hand.press_space(hover.contains_pointer() && page_area.is_mapped())
        }
    });
    keys.connect_key_released({
        let hand = hand.clone();
        move |_, key, _, _| {
            if key == gdk::Key::space {
                hand.release_space();
            }
        }
    });
    window.add_controller(keys);

    // A Space release delivered to another window never reaches `keys`.
    window.connect_is_active_notify({
        let hand = hand.clone();
        move |window| {
            if !window.is_active() {
                hand.reset();
            }
        }
    });

    // Any button, decided per press in `begin`. Capture phase on the
    // scroller's parent, so a claimed press never reaches the page overlays'
    // own gestures (annotation placement, selection, content edit, forms).
    let drag = GestureDrag::new();
    drag.set_button(0);
    drag.set_propagation_phase(PropagationPhase::Capture);
    drag.connect_drag_begin({
        let hand = hand.clone();
        let page_area = page_area.clone();
        move |gesture, x, y| {
            // GTK hands the press an implicit grab on the deepest widget under
            // the pointer, and while it lasts the cursor lookup stops at that
            // widget, so the scroller's cursor alone would never show.
            let grabbed = page_area.pick(x, y, gtk::PickFlags::DEFAULT);
            let state = if hand.begin(gesture.current_button(), grabbed) {
                EventSequenceState::Claimed
            } else {
                EventSequenceState::Denied
            };
            gesture.set_state(state);
        }
    });
    drag.connect_drag_update({
        let hand = hand.clone();
        move |_, dx, dy| hand.update(dx, dy)
    });
    drag.connect_drag_end({
        let hand = hand.clone();
        move |_, _, _| hand.end()
    });
    drag.connect_cancel(move |_, _| hand.end());
    page_area.add_controller(drag);
}

/// The hand tool's state for one viewer.
struct Hand {
    viewer: Viewer,
    state: RefCell<State>,
}

#[derive(Default)]
struct State {
    /// Space is held and armed the hand.
    space: bool,
    drag: Option<Drag>,
    /// The scroller's own cursor, kept while the hand's is shown.
    saved_cursor: Option<Option<gdk::Cursor>>,
    /// The widget holding the press's implicit grab, with its own cursor,
    /// while it shows the closed hand.
    grabbed: Option<(gtk::Widget, Option<gdk::Cursor>)>,
}

#[derive(Clone, Copy)]
struct Drag {
    /// Adjustment values (horizontal, vertical) when the drag began.
    start: (f64, f64),
    /// Started with the primary button, so it lives only while Space does.
    needs_space: bool,
    /// False once Space was let go mid-drag: the press stays claimed until
    /// the button comes up, so the rest of it cannot reach the page tools,
    /// but it no longer moves the document.
    live: bool,
}

impl Hand {
    fn new(viewer: &Viewer) -> Self {
        Self {
            viewer: viewer.clone(),
            state: RefCell::new(State::default()),
        }
    }

    /// Space went down — or repeated. `over_viewer`: the pointer is over the
    /// document. Stop swallows the key so nothing else acts on it.
    fn press_space(&self, over_viewer: bool) -> glib::Propagation {
        // Checked on every press, autorepeats included: Space typed into a
        // text field is text, even if the hand was armed a moment ago.
        if annotations::text_input_has_focus(&self.viewer) {
            self.reset();
            return glib::Propagation::Proceed;
        }
        {
            let mut state = self.state.borrow_mut();
            // A repeat keeps the hand armed even if the pointer drifted off.
            if !state.space {
                if !over_viewer || !has_pages(&self.viewer) {
                    return glib::Propagation::Proceed;
                }
                state.space = true;
            }
        }
        self.show_cursor();
        glib::Propagation::Stop
    }

    fn release_space(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.space = false;
            if let Some(drag) = state.drag.as_mut().filter(|drag| drag.needs_space) {
                drag.live = false;
            }
        }
        self.show_cursor();
    }

    /// A button went down over the document. True claims the press.
    /// `grabbed`: the widget the press landed on, which holds the grab.
    fn begin(&self, button: u32, grabbed: Option<gtk::Widget>) -> bool {
        let space = self.state.borrow().space;
        if !math::grabs(button, space) || !has_pages(&self.viewer) {
            return false;
        }
        let scroll = &self.viewer.scroll;
        {
            let mut state = self.state.borrow_mut();
            state.drag = Some(Drag {
                start: (scroll.hadjustment().value(), scroll.vadjustment().value()),
                needs_space: button != math::MIDDLE,
                live: true,
            });
            // The scroller's cursor is already handled on its own.
            state.grabbed = grabbed
                .filter(|widget| widget != scroll.upcast_ref::<gtk::Widget>())
                .map(|widget| {
                    let own = widget.cursor();
                    (widget, own)
                });
        }
        self.show_cursor();
        true
    }

    /// The pointer moved `dx`, `dy` since the press.
    fn update(&self, dx: f64, dy: f64) {
        let Some(drag) = self.state.borrow().drag.filter(|drag| drag.live) else {
            return;
        };
        let scroll = &self.viewer.scroll;
        for (adjustment, start, offset) in [
            (scroll.hadjustment(), drag.start.0, dx),
            (scroll.vadjustment(), drag.start.1, dy),
        ] {
            adjustment.set_value(math::panned(start, offset, bounds(&adjustment)));
        }
    }

    /// The button came up, or the press was taken away.
    fn end(&self) {
        self.state.borrow_mut().drag = None;
        self.show_cursor();
    }

    /// Drop everything: the window lost focus, or a text field took Space.
    fn reset(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.space = false;
            state.drag = None;
        }
        self.show_cursor();
    }

    /// Shows the cursor the current state calls for, restoring the scroller's
    /// own one once the hand has nothing to show.
    fn show_cursor(&self) {
        let mut state = self.state.borrow_mut();
        let name = if state.drag.is_some_and(|drag| drag.live) {
            Some(GRABBING)
        } else if state.space {
            Some(GRAB)
        } else {
            None
        };
        let scroll = &self.viewer.scroll;
        match name {
            Some(name) => {
                if state.saved_cursor.is_none() {
                    state.saved_cursor = Some(scroll.cursor());
                }
                scroll.set_cursor_from_name(Some(name));
            }
            None => {
                if let Some(saved) = state.saved_cursor.take() {
                    scroll.set_cursor(saved.as_ref());
                }
            }
        }
        // Only the closed hand needs the grab holder; once the drag is over,
        // or has stopped panning, it gets its own cursor back for good.
        if name == Some(GRABBING) {
            if let Some((widget, _)) = &state.grabbed {
                widget.set_cursor_from_name(Some(GRABBING));
            }
        } else if let Some((widget, own)) = state.grabbed.take() {
            widget.set_cursor(own.as_ref());
        }
    }
}

fn bounds(adjustment: &gtk::Adjustment) -> math::Bounds {
    math::Bounds {
        lower: adjustment.lower(),
        upper: adjustment.upper(),
        page_size: adjustment.page_size(),
    }
}

/// The same guard the wheel zoom uses: nothing to pan without pages.
fn has_pages(viewer: &Viewer) -> bool {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| !session.pages.is_empty())
}

#[cfg(test)]
mod tests {
    use super::math::{MIDDLE, PRIMARY};
    use super::*;
    use crate::app::{test_fixtures, ui_tests::built_ui, BuiltUi};

    fn with_page(built: &BuiltUi) {
        let mut session = test_fixtures::model_session(pdf_document::Document::blank());
        session
            .pages
            .push(test_fixtures::page_slot(gtk::Overlay::new()));
        session.page_heights.push(842);
        built.viewer.state.borrow_mut().session = Some(session);
        built
            .viewer
            .scroll
            .hadjustment()
            .configure(400.0, 0.0, 1000.0, 1.0, 10.0, 200.0);
        built
            .viewer
            .scroll
            .vadjustment()
            .configure(400.0, 0.0, 2000.0, 1.0, 10.0, 300.0);
    }

    fn cursor(built: &BuiltUi) -> Option<String> {
        built
            .viewer
            .scroll
            .cursor()
            .and_then(|cursor| cursor.name())
            .map(String::from)
    }

    fn values(built: &BuiltUi) -> (f64, f64) {
        (
            built.viewer.scroll.hadjustment().value(),
            built.viewer.scroll.vadjustment().value(),
        )
    }

    #[gtk::test]
    fn gtk_ui_pan_listens_in_the_capture_phase_on_any_button() {
        let built = built_ui();
        let page_area = built.viewer.scroll.parent().unwrap();
        let controllers = page_area.observe_controllers();
        let drag = (0..controllers.n_items())
            .find_map(|index| controllers.item(index)?.downcast::<GestureDrag>().ok())
            .expect("the document area must have the hand's drag gesture");
        assert_eq!(drag.propagation_phase(), PropagationPhase::Capture);
        assert_eq!(drag.button(), 0);

        let controllers = built.window.observe_controllers();
        let keys = (0..controllers.n_items())
            .filter_map(|index| {
                controllers
                    .item(index)?
                    .downcast::<EventControllerKey>()
                    .ok()
            })
            .find(|keys| keys.propagation_phase() == PropagationPhase::Capture)
            .expect("Space must be seen before the focused widget gets it");
        drop(keys);
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_space_over_the_document_arms_the_hand() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);

        assert_eq!(hand.press_space(true), glib::Propagation::Stop);
        assert_eq!(cursor(&built).as_deref(), Some(GRAB));
        // Key repeat sends more presses; they must not disturb the state.
        assert_eq!(hand.press_space(true), glib::Propagation::Stop);
        assert_eq!(cursor(&built).as_deref(), Some(GRAB));

        hand.release_space();
        assert_eq!(cursor(&built), None);
        assert!(
            !hand.begin(PRIMARY, None),
            "one release disarms after any repeat"
        );
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_space_away_from_the_document_is_left_alone() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);

        assert_eq!(hand.press_space(false), glib::Propagation::Proceed);
        assert_eq!(cursor(&built), None);
        assert!(!hand.begin(PRIMARY, None));
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_space_is_never_taken_from_a_focused_text_field() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);
        let field = gtk::Entry::new();
        built.viewer.pages.append(&field);
        assert!(field.grab_focus(), "the test window takes focus");

        assert_eq!(hand.press_space(true), glib::Propagation::Proceed);
        assert_eq!(cursor(&built), None);
        assert!(!hand.begin(PRIMARY, None));

        let text = gtk::TextView::new();
        built.viewer.pages.append(&text);
        assert!(text.grab_focus());
        assert_eq!(hand.press_space(true), glib::Propagation::Proceed);
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_focus_moving_into_a_text_field_hands_space_back() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);
        assert_eq!(hand.press_space(true), glib::Propagation::Stop);

        let field = gtk::Entry::new();
        built.viewer.pages.append(&field);
        assert!(field.grab_focus());
        // The next autorepeat press now belongs to the field.
        assert_eq!(hand.press_space(true), glib::Propagation::Proceed);
        assert_eq!(cursor(&built), None);
        assert!(!hand.begin(PRIMARY, None));
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_ignores_missing_and_pageless_documents() {
        let built = built_ui();
        for session in [
            None,
            Some(test_fixtures::model_session(
                pdf_document::Document::default(),
            )),
        ] {
            built.viewer.state.borrow_mut().session = session;
            let hand = Hand::new(&built.viewer);
            assert_eq!(hand.press_space(true), glib::Propagation::Proceed);
            assert!(!hand.begin(MIDDLE, None));
            assert_eq!(cursor(&built), None);
        }
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_primary_press_without_space_reaches_the_tools() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);

        assert!(!hand.begin(PRIMARY, None));
        hand.update(50.0, 50.0);
        assert_eq!(values(&built), (400.0, 400.0));
        assert_eq!(cursor(&built), None);
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_space_and_primary_drag_moves_both_axes() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);
        hand.press_space(true);

        assert!(hand.begin(PRIMARY, None));
        assert_eq!(cursor(&built).as_deref(), Some(GRABBING));
        hand.update(30.0, -40.0);
        assert_eq!(values(&built), (370.0, 440.0));
        // Offsets are from the press, not from the previous update.
        hand.update(60.0, -80.0);
        assert_eq!(values(&built), (340.0, 480.0));
        // Clamped to the adjustments' ends.
        hand.update(5000.0, -5000.0);
        assert_eq!(values(&built), (0.0, 1700.0));

        hand.end();
        assert_eq!(cursor(&built).as_deref(), Some(GRAB));
        hand.release_space();
        assert_eq!(cursor(&built), None);
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_middle_drag_needs_no_space() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);

        assert!(hand.begin(MIDDLE, None));
        assert_eq!(cursor(&built).as_deref(), Some(GRABBING));
        hand.update(-25.0, 25.0);
        assert_eq!(values(&built), (425.0, 375.0));
        hand.end();
        assert_eq!(cursor(&built), None);
        assert!(
            !hand.begin(3, None),
            "the secondary button stays with the tools"
        );
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_releasing_space_mid_drag_stops_the_pan() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);
        hand.press_space(true);
        assert!(hand.begin(PRIMARY, None));
        hand.update(10.0, 10.0);

        hand.release_space();
        assert_eq!(cursor(&built), None);
        hand.update(100.0, 100.0);
        assert_eq!(values(&built), (390.0, 390.0));
        hand.end();
        assert_eq!(cursor(&built), None);
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_releasing_space_mid_middle_drag_keeps_panning() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);
        hand.press_space(true);
        assert!(hand.begin(MIDDLE, None));

        hand.release_space();
        hand.update(10.0, 10.0);
        assert_eq!(values(&built), (390.0, 390.0));
        assert_eq!(cursor(&built).as_deref(), Some(GRABBING));
        hand.end();
        assert_eq!(cursor(&built), None);
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_losing_focus_clears_everything() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);
        hand.press_space(true);
        assert!(hand.begin(PRIMARY, None));

        hand.reset();
        assert_eq!(cursor(&built), None);
        hand.update(50.0, 50.0);
        assert_eq!(values(&built), (400.0, 400.0));
        assert!(!hand.begin(PRIMARY, None), "Space is no longer held");
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_pan_restores_the_cursor_it_replaced() {
        let built = built_ui();
        with_page(&built);
        built.viewer.scroll.set_cursor_from_name(Some("crosshair"));
        let hand = Hand::new(&built.viewer);

        hand.press_space(true);
        assert!(hand.begin(PRIMARY, None));
        hand.end();
        hand.release_space();
        assert_eq!(cursor(&built).as_deref(), Some("crosshair"));
        built.window.close();
    }

    fn name_of(widget: &impl IsA<gtk::Widget>) -> Option<String> {
        widget
            .cursor()
            .and_then(|cursor| cursor.name())
            .map(String::from)
    }

    #[gtk::test]
    fn gtk_ui_pan_closed_hand_also_shows_on_the_widget_holding_the_press() {
        let built = built_ui();
        with_page(&built);
        let hand = Hand::new(&built.viewer);
        // The cursor lookup stops at the implicit grab's widget, so the
        // closed hand must be on the page the press landed on too.
        let page = gtk::Picture::new();
        page.set_cursor_from_name(Some("text"));
        built.viewer.pages.append(&page);

        hand.press_space(true);
        assert_eq!(
            name_of(&page).as_deref(),
            Some("text"),
            "Space alone keeps it"
        );
        assert!(hand.begin(PRIMARY, Some(page.clone().upcast())));
        assert_eq!(name_of(&page).as_deref(), Some(GRABBING));
        hand.end();
        assert_eq!(name_of(&page).as_deref(), Some("text"));

        // Space let go mid-drag: no more panning, so no more closed hand.
        assert!(hand.begin(PRIMARY, Some(page.clone().upcast())));
        hand.release_space();
        assert_eq!(name_of(&page).as_deref(), Some("text"));
        hand.end();
        assert_eq!(name_of(&page).as_deref(), Some("text"));

        assert!(hand.begin(MIDDLE, Some(page.clone().upcast())));
        hand.reset();
        assert_eq!(name_of(&page).as_deref(), Some("text"));
        built.window.close();
    }
}
