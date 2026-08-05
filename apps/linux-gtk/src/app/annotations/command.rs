//! The one door from a UI action to the document's undoable command log.
//!
//! Every annotation mutation in this shell goes through [`command`], and the
//! history actions that replay them live here too — undo is the same log seen
//! from the other end.

use gtk::prelude::*;
use pdf_document::{Command, Document};

use crate::app::selection;
use crate::app::state::{DocumentSession, Viewer, ANNOTATION_MODEL_UNAVAILABLE};

use super::toolbar::update_annotation_controls;

const NO_DOCUMENT: &str = "Open a PDF before editing annotations.";

/// Connects model-native history to the window actions and standard shortcuts.
pub(crate) fn connect_history_shortcuts(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    viewer.undo_action.connect_activate({
        let viewer = viewer.clone();
        move |_, _| undo(&viewer)
    });
    viewer.redo_action.connect_activate({
        let viewer = viewer.clone();
        move |_, _| redo(&viewer)
    });
    window.add_action(&viewer.undo_action);
    window.add_action(&viewer.redo_action);
    application.set_accels_for_action("win.undo", &["<Control>z"]);
    application.set_accels_for_action("win.redo", &["<Control>y"]);
}

fn undo(viewer: &Viewer) {
    history(viewer, true);
}

fn redo(viewer: &Viewer) {
    history(viewer, false);
}

fn history(viewer: &Viewer, undo: bool) {
    let changed = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(document) = session.document_model.as_mut() else {
            return;
        };
        let mut log = std::mem::take(&mut document.pending_edits);
        let changed = if undo {
            log.undo(document)
        } else {
            log.redo(document)
        };
        document.pending_edits = log;
        if changed {
            session.edit_revision += 1;
            if session
                .selected_annotation
                .is_some_and(|id| document.annotations.get(id).is_none())
            {
                session.selected_annotation = None;
            }
        }
        changed
    };
    if changed {
        viewer.status.set_text(if undo {
            "Edit undone. Changes are pending save."
        } else {
            "Edit redone. Changes are pending save."
        });
        update_annotation_controls(viewer);
        selection::redraw(viewer);
    }
}

/// Runs one annotation command against the open document, then reports the
/// outcome and repaints.
///
/// Every button shares this shape: refuse early when the document withholds
/// the permission, resolve the session, run the command, report. Keeping the
/// refusal in one place is what stops a future button from forgetting it.
pub(super) fn command(
    viewer: &Viewer,
    operation: impl FnOnce(&mut DocumentSession) -> Result<String, String>,
) {
    if let Some(refusal) = viewer.annotation_editing_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    // The borrow ends before the reporting below: both
    // `update_annotation_controls` and `redraw` borrow the state again.
    let result = {
        let mut state = viewer.state.borrow_mut();
        match state.session.as_mut() {
            Some(session) => operation(session),
            None => Err(NO_DOCUMENT.to_string()),
        }
    };
    match result {
        Ok(message) => {
            if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
                session.edit_revision += 1;
            }
            viewer.status.set_text(&message);
        }
        Err(error) => viewer.status.set_text(&error),
    }
    // Both outcomes, not just success: a rejected placement has already been
    // taken off the session, and its preview has to stop being painted.
    update_annotation_controls(viewer);
    selection::redraw(viewer);
}

/// The editable model for the open document.
///
/// Unreachable in practice — a session without a model reports
/// `AnnotationAccess::Unavailable`, which [`command`] refuses before it gets
/// here — but it reports the same message that refusal does, so the two can
/// never contradict each other if that ever stops being true.
pub(super) fn model(session: &mut DocumentSession) -> Result<&mut Document, String> {
    session
        .document_model
        .as_mut()
        .ok_or_else(|| ANNOTATION_MODEL_UNAVAILABLE.to_string())
}

/// Records `command` in the document's own `EditLog`.
///
/// The log lives *inside* the document it mutates, so it is moved out for the
/// duration of the call and put back afterwards — `EditLog::apply` needs a
/// `&mut Document` that cannot also be borrowed through the log.
pub(super) fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}
