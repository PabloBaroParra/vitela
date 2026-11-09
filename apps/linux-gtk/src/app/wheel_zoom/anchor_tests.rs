use super::*;
use crate::app::{
    layout::{self, Zoom},
    test_fixtures,
    ui_tests::built_ui,
};
use std::time::{Duration, Instant};

fn wait_until(condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() && Instant::now() < deadline {
        let context = glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    condition()
}

fn prepared() -> (crate::app::BuiltUi, Rc<Cell<Option<Anchor>>>) {
    let built = built_ui();
    let viewer = &built.viewer;
    let area = viewer
        .scroll
        .parent()
        .unwrap()
        .downcast::<gtk::Overlay>()
        .unwrap();
    let pending = connect(&area, viewer);
    let mut session = test_fixtures::model_session(pdf_document::Document::blank());
    for (width, height) in [(595., 842.), (842., 595.)] {
        let overlay = gtk::Overlay::new();
        let mut page = test_fixtures::page_slot(overlay);
        page.width_pt = width;
        page.height_pt = height;
        page.overlay.set_child(Some(&page.picture));
        page.overlay.set_halign(gtk::Align::Center);
        viewer.pages.append(&page.overlay);
        session.pages.push(page);
        session.page_heights.push(height as i32);
    }
    viewer.state.borrow_mut().session = Some(session);
    viewer
        .view_stack
        .set_visible_child_name(crate::app::home::EDITOR_PAGE);
    built.window.present();
    layout::set_zoom(viewer, Zoom::Custom(1.5));
    assert!(
        wait_until(|| viewer.scroll.is_mapped() && viewer.scroll.vadjustment().upper() > 2000.),
        "initial allocation"
    );
    viewer.scroll.hadjustment().set_value(150.);
    let top = layout::page_top(
        &viewer.state.borrow().session.as_ref().unwrap().page_heights,
        1,
    );
    viewer.scroll.vadjustment().set_value(top - 70.);
    assert!(
        wait_until(|| {
            let state = viewer.state.borrow();
            let page = &state.session.as_ref().unwrap().pages[1].picture;
            page.compute_point(&viewer.scroll, &graphene::Point::zero())
                .is_some_and(|origin| (origin.y() - 70.).abs() < 2.)
        }),
        "initial scroll"
    );
    (built, pending)
}

fn landed(viewer: &Viewer, anchor: Anchor) -> (f32, f32) {
    let state = viewer.state.borrow();
    let page = &state.session.as_ref().unwrap().pages[anchor.page].picture;
    let position = page
        .compute_point(
            &viewer.scroll,
            &graphene::Point::new(
                anchor.fraction.0 * page.width() as f32,
                anchor.fraction.1 * page.height() as f32,
            ),
        )
        .unwrap();
    (position.x(), position.y())
}

#[gtk::test]
fn gtk_ui_wheel_zoom_keeps_the_cursor_point_on_both_axes_after_allocation() {
    let (built, pending) = prepared();
    let viewer = &built.viewer;
    let pointer = (180., 160.);
    for dy in [-1., -1., 1., 1.] {
        let anchor = Anchor::capture(viewer, pointer).unwrap();
        assert_eq!(
            anchor.page, 1,
            "the cursor, not the viewport top, chooses the page"
        );
        super::super::handle_at(
            viewer,
            &pending,
            gdk::ModifierType::CONTROL_MASK,
            dy,
            Some(pointer),
        );
        assert!(
            wait_until(|| {
                let position = landed(viewer, anchor);
                pending.get().is_none()
                    && (position.0 - pointer.0).abs() < 2.
                    && (position.1 - pointer.1).abs() < 2.
            }),
            "cursor drift after {dy}: {:?}; pending {}; offsets ({}, {})",
            landed(viewer, anchor),
            pending.get().is_some(),
            viewer.scroll.hadjustment().value(),
            viewer.scroll.vadjustment().value()
        );
    }
    built.window.close();
    assert!(pending.get().is_none());
}

#[gtk::test]
fn gtk_ui_wheel_zoom_coalesces_steps_before_the_next_layout() {
    let (built, pending) = prepared();
    let viewer = &built.viewer;
    let pointer = (180., 160.);
    let anchor = Anchor::capture(viewer, pointer).unwrap();
    for _ in 0..3 {
        super::super::handle_at(
            viewer,
            &pending,
            gdk::ModifierType::CONTROL_MASK,
            -1.,
            Some(pointer),
        );
    }
    assert!(
        wait_until(|| {
            let position = landed(viewer, anchor);
            pending.get().is_none()
                && (position.0 - pointer.0).abs() < 2.
                && (position.1 - pointer.1).abs() < 2.
        }),
        "batched anchor drift: {:?}",
        landed(viewer, anchor)
    );
    built.window.close();
}

#[gtk::test]
fn gtk_ui_wheel_zoom_steps_from_the_cursor_page_in_a_mixed_width_fit() {
    let (built, pending) = prepared();
    let viewer = &built.viewer;
    viewer.state.borrow_mut().session.as_mut().unwrap().pages[1].width_pt = 1684.;
    layout::set_zoom(viewer, Zoom::FitWidth);
    assert!(wait_until(|| {
        let state = viewer.state.borrow();
        let picture = &state.session.as_ref().unwrap().pages[1].picture;
        picture.width() == picture.width_request()
    }));
    let anchor = {
        let state = viewer.state.borrow();
        let picture = &state.session.as_ref().unwrap().pages[1].picture;
        let point = picture
            .compute_point(&viewer.scroll, &graphene::Point::new(100., 40.))
            .unwrap();
        drop(state);
        Anchor::capture(viewer, (point.x(), point.y())).unwrap()
    };
    assert_eq!(anchor.page, 1);
    let factor = anchor.factor(viewer).unwrap();
    super::super::handle_at(
        viewer,
        &pending,
        gdk::ModifierType::CONTROL_MASK,
        1.,
        Some(anchor.viewport),
    );
    let state = viewer.state.borrow();
    let Zoom::Custom(after) = state.session.as_ref().unwrap().zoom else {
        panic!("wheel must leave fit mode");
    };
    assert!(
        after < factor,
        "wheel down must zoom out on the page under the cursor"
    );
    drop(state);
    built.window.close();
}

#[gtk::test]
fn gtk_ui_wheel_zoom_discards_an_anchor_after_replacement_or_another_zoom() {
    let (built, _) = prepared();
    let viewer = &built.viewer;
    let anchor = Anchor::capture(viewer, (180., 160.)).unwrap();
    let offsets = (
        viewer.scroll.hadjustment().value(),
        viewer.scroll.vadjustment().value(),
    );
    viewer.state.borrow_mut().session_id += 1;
    anchor.restore(viewer);
    assert_eq!(
        offsets,
        (
            viewer.scroll.hadjustment().value(),
            viewer.scroll.vadjustment().value()
        )
    );
    viewer.state.borrow_mut().session_id -= 1;
    viewer.state.borrow_mut().session.as_mut().unwrap().zoom = Zoom::Custom(2.);
    anchor.restore(viewer);
    assert_eq!(
        offsets,
        (
            viewer.scroll.hadjustment().value(),
            viewer.scroll.vadjustment().value()
        )
    );
    built.window.close();
}
