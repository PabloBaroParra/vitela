//! Native Ctrl+wheel input; layout and rendering remain on the toolbar zoom path.

mod anchor;

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gdk, glib, EventControllerMotion, EventControllerScroll, EventControllerScrollFlags,
    PropagationPhase,
};

use super::state::Viewer;

pub(super) fn connect(page_area: &gtk::Overlay, viewer: &Viewer) {
    let pending = anchor::connect(page_area, viewer);
    let pointer = track_pointer(&viewer.scroll);
    let scroll = EventControllerScroll::new(
        EventControllerScrollFlags::BOTH_AXES | EventControllerScrollFlags::DISCRETE,
    );
    // Capture on the viewport's parent, before ScrolledWindow consumes the wheel.
    scroll.set_propagation_phase(PropagationPhase::Capture);
    scroll.connect_scroll({
        let viewer = viewer.clone();
        move |controller, _, dy| {
            let pointer = pointer.get();
            handle_at(
                &viewer,
                &pending,
                controller.current_event_state(),
                dy,
                pointer,
            )
        }
    });
    page_area.add_controller(scroll);
}

/// The pointer, in `scroll`'s own coordinates, as the anchor expects it.
///
/// Not read off the wheel event: GDK's scroll event type has no position
/// accessor (`gdk_scroll_event_info` leaves `get_position` unset), so
/// `gdk::Event::position` is always `None` for it. Reading it there left every
/// wheel step unanchored, zooming from the viewport's top-left corner — the
/// very behaviour anchoring was meant to end.
fn track_pointer(scroll: &gtk::ScrolledWindow) -> Rc<Cell<Option<(f32, f32)>>> {
    let pointer = Rc::new(Cell::new(None));
    let motion = EventControllerMotion::new();
    // Capture, so motion over the pages reaches it before any page handler.
    motion.set_propagation_phase(PropagationPhase::Capture);
    let set = {
        let pointer = pointer.clone();
        move |_: &EventControllerMotion, x: f64, y: f64| pointer.set(Some((x as f32, y as f32)))
    };
    motion.connect_enter(set.clone());
    motion.connect_motion(set);
    motion.connect_leave({
        let pointer = pointer.clone();
        move |_| pointer.set(None)
    });
    scroll.add_controller(motion);
    pointer
}

fn handle_at(
    viewer: &Viewer,
    pending: &Cell<Option<anchor::Anchor>>,
    modifiers: gdk::ModifierType,
    dy: f64,
    pointer: Option<(f32, f32)>,
) -> glib::Propagation {
    if !modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        pending.set(None);
        return glib::Propagation::Proceed;
    }
    let saved = pending.take().filter(|saved| saved.is_current(viewer));
    let saved = saved
        .filter(|saved| Some(saved.viewport) == pointer)
        .or_else(|| pointer.and_then(|point| anchor::Anchor::capture(viewer, point)));
    let result = handle_scroll(
        viewer,
        modifiers,
        dy,
        saved.and_then(|anchor| anchor.factor(viewer)),
    );
    if result == glib::Propagation::Stop {
        if let Some(mut saved) = saved {
            saved.retarget(viewer);
            pending.set(Some(saved));
            viewer.scroll.queue_allocate();
        }
    }
    result
}

fn handle_scroll(
    viewer: &Viewer,
    modifiers: gdk::ModifierType,
    dy: f64,
    factor: Option<f64>,
) -> glib::Propagation {
    if !modifiers.contains(gdk::ModifierType::CONTROL_MASK) || !dy.is_finite() || dy == 0.0 {
        return glib::Propagation::Proceed;
    }
    let has_pages = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| !session.pages.is_empty());
    if !has_pages {
        return glib::Propagation::Proceed;
    }
    super::step_zoom_from(
        viewer,
        dy < 0.0,
        factor.unwrap_or_else(|| super::current_zoom_factor(viewer)),
    );
    // Consume even at the zoom limits, rather than also scrolling the document.
    glib::Propagation::Stop
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{layout::Zoom, test_fixtures, ui_tests::built_ui};

    #[gtk::test]
    fn gtk_ui_wheel_zoom_reads_the_pointer_from_motion_over_the_document() {
        let built = built_ui();
        let controllers = built.viewer.scroll.observe_controllers();
        let motion = (0..controllers.n_items())
            .find_map(|index| {
                controllers
                    .item(index)?
                    .downcast::<EventControllerMotion>()
                    .ok()
            })
            .expect("the scroller must track the pointer; wheel events carry no position");
        assert_eq!(motion.propagation_phase(), PropagationPhase::Capture);
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_wheel_zoom_without_control_and_horizontal_wheel_keep_normal_scrolling() {
        let built = built_ui();
        let controllers = built.viewer.scroll.parent().unwrap().observe_controllers();
        let controller = (0..controllers.n_items())
            .find_map(|index| {
                controllers
                    .item(index)?
                    .downcast::<EventControllerScroll>()
                    .ok()
            })
            .expect("the document viewport must have its wheel controller");
        assert_eq!(controller.propagation_phase(), PropagationPhase::Capture);
        assert!(controller
            .flags()
            .contains(EventControllerScrollFlags::DISCRETE));
        assert_eq!(
            handle_scroll(&built.viewer, gdk::ModifierType::empty(), -1.0, None),
            glib::Propagation::Proceed
        );
        assert_eq!(
            handle_scroll(&built.viewer, gdk::ModifierType::CONTROL_MASK, 0.0, None),
            glib::Propagation::Proceed
        );
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_wheel_zoom_ignores_missing_and_pageless_documents() {
        let built = built_ui();
        for session in [
            None,
            Some(test_fixtures::model_session(
                pdf_document::Document::default(),
            )),
        ] {
            built.viewer.state.borrow_mut().session = session;
            assert_eq!(
                handle_scroll(&built.viewer, gdk::ModifierType::CONTROL_MASK, -1.0, None),
                glib::Propagation::Proceed
            );
        }
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_wheel_zoom_reuses_toolbar_steps_readout_and_limits_without_editing() {
        let built = built_ui();
        let mut session = test_fixtures::model_session(pdf_document::Document::blank());
        session
            .pages
            .push(test_fixtures::page_slot(gtk::Overlay::new()));
        session.page_heights.push(842);
        built.viewer.state.borrow_mut().session = Some(session);

        for (start, dy, expected) in [
            (1.0, -1.0, 1.25),
            (1.0, 1.0, 0.75),
            (8.0, -1.0, 8.0),
            (0.10, 1.0, 0.10),
        ] {
            super::super::layout::set_zoom(&built.viewer, Zoom::Custom(start));
            assert_eq!(
                handle_scroll(&built.viewer, gdk::ModifierType::empty(), dy, None),
                glib::Propagation::Proceed
            );
            assert!(
                built.viewer.state.borrow().session.as_ref().unwrap().zoom == Zoom::Custom(start)
            );
            assert_eq!(
                handle_scroll(&built.viewer, gdk::ModifierType::CONTROL_MASK, dy, None),
                glib::Propagation::Stop
            );
            let state = built.viewer.state.borrow();
            let session = state.session.as_ref().unwrap();
            assert!(session.zoom == Zoom::Custom(expected));
            assert_eq!(
                built.viewer.zoom_label.text(),
                format!("{:.0}%", expected * 100.0)
            );
            assert!(!session
                .document_model
                .as_ref()
                .unwrap()
                .pending_edits
                .can_undo());
        }
        built.window.close();
    }
}
