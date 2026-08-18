//! Content-edit mode: click an existing text run to retype it in place,
//! preserving its font, size, and position (T-161); select, move, resize,
//! delete, and replace-via-file-picker an existing page image (T-162); arm
//! "insert text"/"insert image" to add brand-new page content instead of
//! targeting whatever is already there (T-163).
//!
//! Wires `core/pdf-edit` (Batch 21's content-stream editor) into this shell
//! directly, the same bypass-`pdf-ffi` posture `annotations` already has.
//! Split by responsibility, mirroring `annotations`:
//!
//! - [`model`] loads a page's parsed content on first need, hit-tests a
//!   click against its text runs and images, and (T-163) picks an unused
//!   font/XObject resource name for something new to insert — pure
//!   functions, no GTK.
//! - [`geometry`] is the image drag/handle pointer maths — pure functions
//!   over rects and points, the image twin of `annotations::geometry`.
//! - [`command`] validates a text or image change (including an insertion)
//!   against the real `pdf-edit` call *before* recording it, then records it
//!   in the document's `EditLog`.
//! - [`editor`] is the inline `Entry` lifecycle for a text run: open (over an
//!   existing run, or blank for an insertion), commit, cancel.
//! - [`image`] is the select/move/resize/delete/replace/insert lifecycle for
//!   an image.
//!
//! This module owns the mode toggle, the two insert-kind toggles, and the
//! gesture dispatch that decides whether a page click is a content edit at
//! all, and which of an existing text run, an existing image, a new text
//! insertion, or a new image insertion it targets.
//!
//! Every commit that actually reaches the `EditLog` — from any of the above,
//! plus undo/redo of one — ends in
//! `document::refresh_after_content_edit` (T-163, batch decision 6): a
//! content edit changes what pdfium itself renders, so the canvas has to
//! show the real, reopened result, not a "pending save" status message over
//! a stale bitmap.

mod command;
mod editor;
pub(crate) mod geometry;
pub(crate) mod image;
mod model;

use gtk::prelude::*;
use gtk::{ApplicationWindow, GestureDrag, ToggleButton};

use crate::app::annotations;
use crate::app::selection::{pointer_to_pdf, redraw};
use crate::app::state::{ContentInsertKind, Viewer};

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

/// Builds the two "insert new content" toggle buttons (T-163) — the mode
/// toggle's siblings, not variants of it: `content_edit_button` decides
/// whether a click can touch content at all, these decide what a click
/// inside that mode *does*. Both start insensitive, same rule and same call
/// site (`update_controls`) as `content_edit_button` itself.
pub(crate) fn build_insert_toggles() -> (ToggleButton, ToggleButton) {
    let insert_text = ToggleButton::with_label("Insert text");
    insert_text.set_sensitive(false);
    let insert_image = ToggleButton::with_label("Insert image");
    insert_image.set_sensitive(false);
    (insert_text, insert_image)
}

/// Wires both insert toggles to [`set_insert_mode`], keeping at most one
/// active at a time.
pub(crate) fn connect_insert_toggles(viewer: &Viewer) {
    connect_insert_toggle(viewer, &viewer.insert_text_button, ContentInsertKind::Text);
    connect_insert_toggle(
        viewer,
        &viewer.insert_image_button,
        ContentInsertKind::Image,
    );
}

fn connect_insert_toggle(viewer: &Viewer, button: &ToggleButton, kind: ContentInsertKind) {
    button.connect_toggled({
        let viewer = viewer.clone();
        move |button| {
            if button.is_active() {
                set_insert_mode(&viewer, Some(kind));
                return;
            }
            // Read into an owned value and let the borrow end here — the
            // branch below re-borrows `viewer.state` inside
            // `set_insert_mode`, which would panic against a `Ref` still
            // held open by a `match`/`if` condition on the field directly.
            let armed = viewer.state.borrow().content_insert_mode;
            if armed == Some(kind) {
                set_insert_mode(&viewer, None);
            }
            // Otherwise this button was switched off as a side effect of the
            // *other* insert kind being armed inside `set_insert_mode`
            // (which un-toggles whichever button is not the new one) —
            // `content_insert_mode` has already moved on to that kind by the
            // time this fires, so there is nothing left here to clear.
        }
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
    drop(state);
    viewer.content_edit_button.set_sensitive(enabled);
    // Same rule as `content_edit_button` itself (T-163): whether a click can
    // insert new content depends on the document's permission, not on
    // whether content-edit mode happens to be toggled on right now —
    // `set_insert_mode` arms the parent mode as a side effect when needed,
    // so gating these on `content_edit_mode` here would be redundant with
    // that, not an additional safeguard.
    viewer.insert_text_button.set_sensitive(enabled);
    viewer.insert_image_button.set_sensitive(enabled);
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
/// also resolves (commits) whatever inline text editor is open and clears
/// any selected image (T-162), the same way switching documents or clicking
/// a different run/image does.
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
        clear_selected_image(viewer);
        // T-163: an armed insert kind cannot outlive the mode that makes it
        // reachable — a page click only reaches an insert path while
        // `content_edit_mode` is true (`selection.rs`'s gesture dispatch), so
        // leaving this armed here would make a toggle look active while
        // doing nothing on the next click.
        {
            let mut state = viewer.state.borrow_mut();
            state.content_insert_mode = None;
        }
        viewer.insert_text_button.set_active(false);
        viewer.insert_image_button.set_active(false);
        viewer.status.set_text("Edit content disarmed.");
    }
    crate::app::update_content_edit_controls(viewer);
    redraw(viewer);
}

/// Arms or disarms which kind of new content a content-edit-mode click
/// inserts (T-163). At most one of `insert_text_button`/`insert_image_button`
/// is active at a time.
///
/// Arming either kind implies content-edit mode itself is armed: a page
/// click only reaches this module's insert routing (`handle_drag_end`) while
/// `content_edit_mode` is true, so an insert button that toggled on without
/// also arming the parent mode would look armed and then do nothing on the
/// next click. `set_mode` is idempotent when already active (see its own
/// guard at the top), so this is free once content-edit mode is already on.
pub(crate) fn set_insert_mode(viewer: &Viewer, kind: Option<ContentInsertKind>) {
    {
        let mut state = viewer.state.borrow_mut();
        if state.content_insert_mode == kind {
            return;
        }
        state.content_insert_mode = kind;
    }

    if kind.is_some() {
        set_mode(viewer, true);
    }

    // Resolves whatever is already open/selected first — the same
    // precondition `editor::open_editor`/`image::begin_image_drag` already
    // enforce before claiming a click, so switching insert kinds mid-edit
    // never abandons one silently.
    editor::commit(viewer);
    clear_selected_image(viewer);

    viewer
        .insert_text_button
        .set_active(kind == Some(ContentInsertKind::Text));
    viewer
        .insert_image_button
        .set_active(kind == Some(ContentInsertKind::Image));

    viewer.status.set_text(match kind {
        Some(ContentInsertKind::Text) => {
            "Insert text armed — click the page to place a new text box."
        }
        Some(ContentInsertKind::Image) => {
            "Insert image armed — click the page to insert a picture."
        }
        None => "Edit content armed — click a text run to retype it.",
    });
    crate::app::update_content_edit_controls(viewer);
    redraw(viewer);
}

/// Clears whatever image is selected (and any drag in flight), if any —
/// the T-162 twin of `editor::commit`'s own resolve-and-close, but an image
/// selection has nothing to validate on the way out, only to drop.
fn clear_selected_image(viewer: &Viewer) {
    let mut state = viewer.state.borrow_mut();
    if let Some(session) = state.session.as_mut() {
        session.selected_image = None;
        session.image_drag = None;
    }
}

/// Delegates a page's drag-begin to the image gesture (T-162): an image
/// press is live-claimed here, at press time, rather than deferred to
/// `drag_end` the way a text-run click still is — mirrors
/// `annotations::gesture::begin_annotation_drag`'s own timing. Returns
/// whether an image claimed the press.
pub(crate) fn begin_drag(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> bool {
    image::begin_image_drag(viewer, page_index, point, reach)
}

/// Delegates a page's drag-update to the image gesture. Returns whether an
/// image drag was in flight.
pub(crate) fn extend_drag(viewer: &Viewer, point: (f64, f64)) -> bool {
    image::extend_image_drag(viewer, point)
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
///
/// `pub(crate)` rather than private (T-163): `document::refresh_after_content_edit`
/// calls this after every content-edit commit's save→reopen cycle, because
/// the reopened session's `PageSlot::content` caches start out empty again —
/// without a re-parse here, the outline would stay blank until the user
/// clicked a run, exactly the gap arming the mode for the first time already
/// avoids.
///
/// It re-parses the *preserved* `save_backing`, not the refreshed bytes on
/// screen, then layers the pending `EditLog` back on top via
/// `model::ensure_page_content`'s `pending` argument
/// (`model::overlay_pending_content`) — so content added, moved, retyped or
/// removed since the last disk save is both rendered *and* clickable,
/// closing the gap this doc used to describe. Reparsing from `save_backing`
/// rather than the refreshed bytes still matters: it is what the log was
/// recorded against, so hit-testing and validation keep agreeing with the
/// commands already queued.
///
/// A second edit on an item the overlay only shows because of a pending
/// command splits by kind. **Text** is retyped again freely: the edit folds
/// into the command already describing that run
/// (`command::pending_text_command_index`), so the log keeps exactly one
/// entry per run, still keyed to the snapshot the base document holds.
/// **Images** are still refused (`command::image_already_edited`) — a move
/// followed by a resize is two operations against two different geometries,
/// and the second would need a fresh bbox no live re-render exists to
/// re-read.
pub(crate) fn load_all_page_content(viewer: &Viewer) {
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
    let pending = session
        .document_model
        .as_ref()
        .map(|document| &document.pending_edits);
    for (index, page) in session.pages.iter_mut().enumerate() {
        let _ = model::ensure_page_content(&mut page.content, base, index, pending);
    }
}

/// The content-edit half of a page's drag gesture.
///
/// Tries [`image::finish_image_drag`] first (T-162): if an image drag was in
/// flight — claimed back at `begin_drag` — it validates and records
/// whatever the drag did (or nothing, for a click that only selected the
/// image) and this function stops, never touching the text-run editor
/// below. Only when no image drag existed does this fall through to the
/// original T-161 behaviour: a text-run edit has no live preview to paint
/// while the pointer is down, so it claims the whole gesture and resolves it
/// as a click here, on `drag_end`.
pub(crate) fn handle_drag_end(
    viewer: &Viewer,
    page_index: usize,
    gesture: &GestureDrag,
    offset_x: f64,
    offset_y: f64,
) {
    if image::finish_image_drag(viewer) {
        return;
    }

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

    // T-163: while an insert kind is armed, a click anywhere on the page
    // (that did not land on an existing image — `image::finish_image_drag`
    // above already returned for that case) composes brand-new content
    // instead of targeting whatever, if anything, is already at the point.
    // Deliberately skips `text_run_at` entirely rather than falling back to
    // it on a miss: while this sub-mode is armed, a click is *always* about
    // creating something new, never about retyping whatever happens to sit
    // under the pointer.
    //
    // Read into an owned value first and let the borrow end here: a `match`
    // scrutinee's temporaries live for the whole match (all arm bodies), so
    // matching `viewer.state.borrow().content_insert_mode` directly would
    // keep the `Ref` open while the arms below re-borrow `viewer.state`
    // through `editor::open_insert_editor`/`image::insert_at` — a
    // `BorrowMutError` panic waiting to happen.
    let insert_kind = viewer.state.borrow().content_insert_mode;
    match insert_kind {
        Some(ContentInsertKind::Text) => {
            editor::open_insert_editor(viewer, page_index, (x, y));
            return;
        }
        Some(ContentInsertKind::Image) => {
            let Some(window) = window_of(viewer) else {
                viewer
                    .status
                    .set_text("The application window is unavailable.");
                return;
            };
            image::insert_at(&window, viewer, page_index, (x, y));
            return;
        }
        None => {}
    }

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
        let pending = session
            .document_model
            .as_ref()
            .map(|document| &document.pending_edits);
        let Some(page) = session.pages.get_mut(page_index) else {
            return;
        };
        match model::ensure_page_content(&mut page.content, base, page_index, pending) {
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

/// Recovers the shell's top-level window from a widget that is always in the
/// tree once a document is open — `image::insert_at`'s file picker needs one
/// to parent itself against, and `handle_drag_end` has no window parameter
/// of its own to hand it. Mirrors `document::open_file`'s own recovery of
/// the window from `viewer.status.root()` for the same reason: a drop (there)
/// or a page click (here) has no direct window parameter either.
fn window_of(viewer: &Viewer) -> Option<ApplicationWindow> {
    viewer
        .status
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
}
