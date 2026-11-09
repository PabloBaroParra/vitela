//! Native Ctrl+wheel input; layout and rendering remain on the toolbar zoom path.

use gtk::prelude::*;
use gtk::{gdk, glib, EventControllerScroll, EventControllerScrollFlags, PropagationPhase};

use super::state::Viewer;

pub(super) fn connect(page_area: &gtk::Overlay, viewer: &Viewer) {
    let scroll = EventControllerScroll::new(
        EventControllerScrollFlags::BOTH_AXES | EventControllerScrollFlags::DISCRETE,
    );
    // Capture on the viewport's parent, before ScrolledWindow consumes the wheel.
    scroll.set_propagation_phase(PropagationPhase::Capture);
    scroll.connect_scroll({
        let viewer = viewer.clone();
        move |controller, _, dy| handle_scroll(&viewer, controller.current_event_state(), dy)
    });
    page_area.add_controller(scroll);
}

fn handle_scroll(viewer: &Viewer, modifiers: gdk::ModifierType, dy: f64) -> glib::Propagation {
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
    super::step_zoom(viewer, dy < 0.0);
    // Consume even at the zoom limits, rather than also scrolling the document.
    glib::Propagation::Stop
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{layout::Zoom, test_fixtures, ui_tests::built_ui};

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
            handle_scroll(&built.viewer, gdk::ModifierType::empty(), -1.0),
            glib::Propagation::Proceed
        );
        assert_eq!(
            handle_scroll(&built.viewer, gdk::ModifierType::CONTROL_MASK, 0.0),
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
                handle_scroll(&built.viewer, gdk::ModifierType::CONTROL_MASK, -1.0),
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
                handle_scroll(&built.viewer, gdk::ModifierType::empty(), dy),
                glib::Propagation::Proceed
            );
            assert!(
                built.viewer.state.borrow().session.as_ref().unwrap().zoom == Zoom::Custom(start)
            );
            assert_eq!(
                handle_scroll(&built.viewer, gdk::ModifierType::CONTROL_MASK, dy),
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
