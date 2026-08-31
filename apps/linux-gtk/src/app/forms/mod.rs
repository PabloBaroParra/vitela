//! Form-field editing mode (T-141, Batch 20): place a new field
//! (text/checkbox/radio group/dropdown) on the canvas, drag it or a corner
//! handle to move/resize it, and restyle the selected one's font, size, and
//! color — the form-field twin of `content_edit`, wired directly to
//! `core/pdf-form` with the same bypass-`pdf-ffi` posture `annotations` and
//! `content_edit` already have.
//!
//! Unlike a text run or an image (`content_edit`'s page content), a form
//! field is addressable document state: `Document.form_fields` is a real
//! `FormFieldSet` keyed by `FormFieldId`, already populated at open time from
//! any AcroForm the PDF carries (`pdf_save::document_from_lopdf`, T-137/
//! T-138). That makes this module's command plumbing closer to
//! `annotations` than to `content_edit`: `move_field`/`resize_field`/
//! `restyle_field` are infallible (`pdf-form::ops`'s own doc), so there is no
//! validate-before-record probe, and no form-field `Command` variant is a
//! content edit (`Command::is_content_edit`), so a plain `selection::redraw`
//! after recording is enough — no `document::refresh_after_content_edit`
//! save→reopen cycle.
//!
//! Split by responsibility, mirroring `annotations`:
//!
//! - [`toolbar`] builds the mode toggle, the four placement toggles, and the
//!   style inspector, and owns every rule about when a control is sensitive.
//! - [`command`] is the single door to the document's undoable `EditLog`.
//! - [`builder`] turns a finished placement gesture into a `FormField`.
//! - [`geometry`] is the placement/drag/handle pointer maths — pure
//!   functions, the form-field twin of `annotations::geometry`.
//! - [`gesture`] runs the press/move/release lifecycles: placing a new field
//!   with an armed kind, and dragging an existing one by its body or a
//!   corner handle.
//! - [`style`] restyles the selected field's font, size, and color.
//!
//! This module owns the mode toggle and the gesture dispatch that decides
//! whether a page click is a form-field edit at all, and — while it is —
//! whether that click places a new field or targets an existing one.

mod builder;
mod command;
pub(crate) mod geometry;
mod gesture;
mod style;
mod toolbar;

use gtk::prelude::*;

use crate::app::selection::redraw;
use crate::app::state::{FieldKind, Viewer};

pub(crate) use gesture::{begin_field_drag, begin_placement, extend_field_drag, extend_placement};
pub(crate) use toolbar::{build_forms_content, connect_forms_toolbar, update_forms_controls};

/// Reported when the selected field is gone by the time an operation acting
/// on it runs — the forms twin of `annotations::SELECTION_GONE`.
const SELECTION_GONE: &str = "The selected form field no longer exists.";

pub(crate) fn mode_is_active(viewer: &Viewer) -> bool {
    viewer.state.borrow().form_edit_mode
}

/// Turns forms-edit mode on or off.
///
/// Mutually exclusive with content-edit mode and an armed annotation tool,
/// in every direction: arming this disarms both of them, and each of them
/// disarms this when it arms — the same three-way dance
/// `content_edit::set_mode`/`annotations::toolbar::arm_tool` already run for
/// each other, extended by one more mode. Every setter's early-return guard
/// (below, and in the other two) is what keeps that dance from looping: only
/// *activation* cross-disarms the other two, so a deactivation call can never
/// re-enter this function.
pub(crate) fn set_mode(viewer: &Viewer, active: bool) {
    {
        let mut state = viewer.state.borrow_mut();
        if state.form_edit_mode == active {
            return;
        }
        state.form_edit_mode = active;
    }
    viewer.forms.mode.set_active(active);
    if active {
        crate::app::annotations::disarm(viewer);
        crate::app::content_edit::set_mode(viewer, false);
        // A selection made before this mode was armed means nothing inside
        // it — mirrors `content_edit::set_mode`'s own clear of a stale text
        // selection, for the same reason: this mode never selects text, and
        // leaving one painted over the page would read as state the next
        // click is about to act on.
        if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
            session.selection = None;
        }
        viewer.status.set_text(
            "Edit forms armed — pick a field type to place, or click an existing field to select it.",
        );
    } else {
        {
            let mut state = viewer.state.borrow_mut();
            state.form_field_kind = None;
            if let Some(session) = state.session.as_mut() {
                session.form_field_drag = None;
                session.form_placement = None;
                session.selected_form_field = None;
            }
        }
        for (_, button) in &viewer.forms.place {
            button.set_active(false);
        }
        viewer.status.set_text("Edit forms disarmed.");
    }
    update_forms_controls(viewer);
    redraw(viewer);
}

/// Arms or disarms which kind of field the next click places — the forms
/// twin of `content_edit::set_insert_mode`. Arming a kind implies the mode
/// itself is armed (`set_mode` is idempotent when already active, so this is
/// free once it is); `None` means an ordinary click targets an existing
/// field instead of creating one.
pub(crate) fn set_field_kind(viewer: &Viewer, kind: Option<FieldKind>) {
    {
        let mut state = viewer.state.borrow_mut();
        if state.form_field_kind == kind {
            return;
        }
        state.form_field_kind = kind;
    }
    if kind.is_some() {
        set_mode(viewer, true);
    }
    for (other, button) in &viewer.forms.place {
        button.set_active(Some(*other) == kind);
    }
    // `kind.label()`, not a second string table: `FieldKind::label()`
    // already names every variant for the toolbar buttons, and repeating
    // that mapping here would leave a fifth kind's arming message to drift
    // from its own button the moment someone adds one and forgets this match.
    viewer.status.set_text(&match kind {
        Some(kind) => format!("{} armed — drag on the page to place it.", kind.label()),
        None => "Edit forms armed — click an existing field to select it.".to_string(),
    });
    update_forms_controls(viewer);
    redraw(viewer);
}

/// Delegates a page press to the two form-field gestures: placing a new
/// field (when a kind is armed) first, then dragging an existing one —
/// mirrors `content_edit::begin_drag`'s own two-gesture dispatch order.
/// Returns whether either claimed the press.
pub(crate) fn begin_drag(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> bool {
    begin_placement(viewer, page_index, point) || begin_field_drag(viewer, page_index, point, reach)
}

/// Delegates a page drag-update to whichever gesture is in flight. Returns
/// whether one was.
pub(crate) fn extend_drag(viewer: &Viewer, point: (f64, f64)) -> bool {
    extend_placement(viewer, point) || extend_field_drag(viewer, point)
}

/// Resolves whichever gesture the press claimed, at release — mirrors
/// `selection.rs`'s own unconditional pairing of
/// `annotations::finish_placement`/`annotations::finish_annotation_drag`: at
/// most one of a placement and a field drag is ever in flight, so trying
/// both in sequence is safe and needs no branch of its own here.
pub(crate) fn finish_drag(viewer: &Viewer) {
    gesture::finish_placement(viewer);
    gesture::finish_field_drag(viewer);
}
