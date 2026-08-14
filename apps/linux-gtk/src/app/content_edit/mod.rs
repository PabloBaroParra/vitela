//! Content-edit mode: click an existing text run to retype it in place,
//! preserving its font, size, and position (T-161).
//!
//! Wires `core/pdf-edit` (Batch 21's content-stream editor) into this shell
//! directly, the same bypass-`pdf-ffi` posture `annotations` already has.
//! Split by responsibility, mirroring `annotations`:
//!
//! - [`model`] loads a page's parsed content on first need and hit-tests a
//!   click against its text runs — pure functions, no GTK.
//! - [`command`] validates a replacement against the real font encoder
//!   *before* recording it, then records it in the document's `EditLog`.
//! - [`editor`] is the inline `Entry` lifecycle: open, commit, cancel.
//!
//! This module owns the mode toggle and the gesture dispatch that decides
//! whether a page click is a content edit at all.

mod command;
mod editor;
mod model;

use gtk::prelude::*;
use gtk::{GestureDrag, ToggleButton};

use crate::app::annotations;
use crate::app::selection::{pointer_to_pdf, redraw};
use crate::app::state::Viewer;

/// A drag shorter than this, in device pixels on either axis, is a click —
/// mirrors the annotation placement gesture's own click collapse
/// (`annotations::builder`'s "arrastre < 8pt"), in screen space rather than
/// PDF space because `GestureDrag` reports its offset in the former.
const CLICK_EPSILON_PX: f64 = 4.0;

/// Builds the mode toggle button. Starts insensitive, like every other
/// document-scoped control, until a document reports it permits content
/// changes — see `update_controls`.
pub(crate) fn build_toggle() -> ToggleButton {
    let button = ToggleButton::with_label("Edit content");
    button.set_sensitive(false);
    button
}

pub(crate) fn connect_toggle(viewer: &Viewer) {
    viewer.content_edit_button.connect_toggled({
        let viewer = viewer.clone();
        move |button| set_mode(&viewer, button.is_active())
    });
}

/// Refreshes the toggle's sensitivity from the open document's permission.
///
/// Unlike the annotation toolbar, nothing else here depends on a live
/// selection, so this only needs to run when the document itself changes —
/// see the call site in `document::show_document`.
pub(crate) fn update_controls(viewer: &Viewer) {
    let state = viewer.state.borrow();
    let enabled = state
        .session
        .as_ref()
        .is_some_and(|session| session.content_edit_access.refusal().is_none());
    viewer.content_edit_button.set_sensitive(enabled);
}

pub(crate) fn mode_is_active(viewer: &Viewer) -> bool {
    viewer.state.borrow().content_edit_mode
}

/// Turns content-edit mode on or off.
///
/// Mutually exclusive with an armed annotation tool in both directions:
/// turning this on disarms whatever tool was armed
/// (`annotations::toolbar::disarm`), and `arm_tool` calls back in here to
/// turn this off — one mode claims a page click at a time. Turning it off
/// also resolves (commits) whatever inline editor is open, the same way
/// switching documents or clicking a different run does.
pub(crate) fn set_mode(viewer: &Viewer, active: bool) {
    {
        let mut state = viewer.state.borrow_mut();
        if state.content_edit_mode == active {
            return;
        }
        state.content_edit_mode = active;
    }
    viewer.content_edit_button.set_active(active);
    if active {
        annotations::disarm(viewer);
        load_all_page_content(viewer);
        viewer
            .status
            .set_text("Edit content armed — click a text run to retype it.");
    } else {
        editor::commit(viewer);
        viewer.status.set_text("Edit content disarmed.");
    }
    redraw(viewer);
}

/// Eagerly parses every page's content once content-edit mode turns on, so
/// the composite-font outline (`selection::draw_highlights`) is visible on
/// first paint rather than only after the first click.
///
/// Every `PageSlot` already exists for the whole document the moment it
/// opens — rendering is virtualized separately, the slots are not — so this
/// is a bounded loop over pages already in memory, not a search. A page this
/// build cannot parse is silently left without outlines; the real refusal
/// still surfaces the moment its content is actually clicked
/// (`handle_drag_end`), so nothing is silently lost, only the proactive
/// outline for that one page.
fn load_all_page_content(viewer: &Viewer) {
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    let Some(base) = session
        .save_backing
        .as_ref()
        .map(|backing| backing.base.as_lopdf())
    else {
        return;
    };
    for (index, page) in session.pages.iter_mut().enumerate() {
        let _ = model::ensure_page_content(&mut page.content, base, index);
    }
}

/// The content-edit half of a page's drag gesture: claims the whole gesture
/// while the mode is on, and resolves it as a click on `drag_end` — content
/// edits have no live preview to paint while the pointer is still down, so
/// there is nothing useful to do before then.
pub(crate) fn handle_drag_end(
    viewer: &Viewer,
    page_index: usize,
    gesture: &GestureDrag,
    offset_x: f64,
    offset_y: f64,
) {
    if offset_x.abs() >= CLICK_EPSILON_PX || offset_y.abs() >= CLICK_EPSILON_PX {
        // A real drag in content-edit mode targets nothing — resolve whatever
        // was already open and stop, rather than leaving it stranded.
        editor::commit(viewer);
        return;
    }
    let Some((start_x, start_y)) = gesture.start_point() else {
        return;
    };
    let Some((x, y)) = pointer_to_pdf(viewer, page_index, start_x + offset_x, start_y + offset_y)
    else {
        return;
    };

    let run = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(base) = session
            .save_backing
            .as_ref()
            .map(|backing| backing.base.as_lopdf())
        else {
            return;
        };
        let Some(page) = session.pages.get_mut(page_index) else {
            return;
        };
        match model::ensure_page_content(&mut page.content, base, page_index) {
            Ok(content) => model::text_run_at(content, (x as f32, y as f32)).cloned(),
            Err(error) => {
                drop(state);
                viewer.status.set_text(&error.to_string());
                return;
            }
        }
    };

    match run {
        Some(run) => editor::open_editor(viewer, page_index, run),
        // An empty-space click resolves whatever was open rather than
        // opening nothing and leaving it stranded.
        None => editor::commit(viewer),
    }
}
