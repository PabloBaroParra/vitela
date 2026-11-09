//! GTK allocation-bound cursor anchoring. The pending wheel gesture owns the
//! anchor; another zoom setting, document replacement and unmap invalidate it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib, graphene};

use crate::app::layout::{reading_position, Zoom};
use crate::app::state::Viewer;

#[derive(Clone, Copy)]
pub(super) struct Anchor {
    session_id: u64,
    zoom: Zoom,
    page: usize,
    fraction: (f32, f32),
    pub(super) viewport: (f32, f32),
}

impl Anchor {
    pub(super) fn capture(viewer: &Viewer, viewport: (f32, f32)) -> Option<Self> {
        let state = viewer.state.borrow();
        let session = state.session.as_ref()?;
        let page = reading_position(
            &session.page_heights,
            viewer.scroll.vadjustment().value() + f64::from(viewport.1),
        )
        .page_index;
        let picture = &session.pages.get(page)?.picture;
        if picture.width() <= 0 || picture.height() <= 0 {
            return None;
        }
        let local = viewer
            .scroll
            .compute_point(picture, &graphene::Point::new(viewport.0, viewport.1))?;
        Some(Self {
            session_id: state.session_id,
            zoom: session.zoom,
            page,
            fraction: (
                local.x() / picture.width() as f32,
                local.y() / picture.height() as f32,
            ),
            viewport,
        })
    }

    pub(super) fn is_current(self, viewer: &Viewer) -> bool {
        // Allocation can advance the render generation without changing zoom.
        // Guard the requested setting, not that transient render generation.
        let state = viewer.state.borrow();
        state.session_id == self.session_id
            && state
                .session
                .as_ref()
                .is_some_and(|session| session.zoom == self.zoom)
    }

    pub(super) fn retarget(&mut self, viewer: &Viewer) {
        if let Some(session) = viewer.state.borrow().session.as_ref() {
            self.zoom = session.zoom;
        }
    }

    pub(super) fn factor(self, viewer: &Viewer) -> Option<f64> {
        viewer
            .state
            .borrow()
            .session
            .as_ref()?
            .pages
            .get(self.page)
            .map(|page| page.budget.factor)
    }

    fn restore(self, viewer: &Viewer) {
        if !self.is_current(viewer) {
            return;
        }
        let position = {
            let state = viewer.state.borrow();
            let Some(page) = state
                .session
                .as_ref()
                .and_then(|session| session.pages.get(self.page))
            else {
                return;
            };
            page.picture.compute_point(
                &viewer.scroll,
                &graphene::Point::new(
                    self.fraction.0 * page.picture.width() as f32,
                    self.fraction.1 * page.picture.height() as f32,
                ),
            )
        };
        if let Some(position) = position {
            // Native coordinates include centering, mixed page sizes and gaps.
            // End the state borrow before set_value emits viewport updates.
            let horizontal = viewer.scroll.hadjustment();
            let vertical = viewer.scroll.vadjustment();
            horizontal.set_value(horizontal.value() + f64::from(position.x() - self.viewport.0));
            vertical.set_value(vertical.value() + f64::from(position.y() - self.viewport.1));
        }
    }
}

pub(super) fn event_point(viewer: &Viewer, event: &gdk::Event) -> Option<(f32, f32)> {
    let (x, y) = event.position()?;
    let native = viewer.scroll.native()?;
    let (dx, dy) = native.surface_transform();
    let point = native.compute_point(
        &viewer.scroll,
        &graphene::Point::new((x + dx) as f32, (y + dy) as f32),
    )?;
    Some((point.x(), point.y()))
}

pub(super) fn connect(area: &gtk::Overlay, viewer: &Viewer) -> Rc<Cell<Option<Anchor>>> {
    let pending = Rc::new(Cell::new(None::<Anchor>));
    let connection = Rc::new(RefCell::new(
        None::<(gdk::FrameClock, glib::SignalHandlerId)>,
    ));
    area.connect_map({
        let pending = pending.clone();
        let connection = connection.clone();
        let viewer = viewer.clone();
        move |area| {
            let Some(clock) = area.frame_clock() else {
                return;
            };
            let pending = pending.clone();
            let viewer = viewer.clone();
            // Tick callbacks run BEFORE allocation. Restore after GTK's layout
            // handlers so both the new page boxes and scroll limits are valid.
            let id = clock.connect_local("layout", true, move |_| {
                if let Some(anchor) = pending.take() {
                    anchor.restore(&viewer);
                }
                None
            });
            connection.replace(Some((clock, id)));
        }
    });
    area.connect_unmap({
        let pending = pending.clone();
        move |_| {
            pending.set(None);
            if let Some((clock, id)) = connection.take() {
                clock.disconnect(id);
            }
        }
    });
    pending
}

#[cfg(test)]
#[path = "anchor_tests.rs"]
mod tests;
