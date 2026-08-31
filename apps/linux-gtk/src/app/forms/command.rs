//! The one door from a UI action to the document's undoable command log, for
//! form fields (T-141) — the forms twin of `annotations::command`.
//!
//! Unlike the page-content commands in `content_edit`, every form-field
//! `Command` variant applies directly to `Document.form_fields`
//! (`Command::is_content_edit` excludes all of them) and
//! `move_field`/`resize_field`/`restyle_field` are infallible
//! (`pdf-form::ops`'s own doc) — so, like an annotation command, there is no
//! validate-before-record probe and no save→reopen→re-render cycle after
//! recording, only a plain redraw.

use pdf_document::{Command, Document};

use crate::app::selection;
use crate::app::state::{DocumentSession, Viewer, ANNOTATION_MODEL_UNAVAILABLE};

use super::toolbar::update_forms_controls;

const NO_DOCUMENT: &str = "Open a PDF before editing form fields.";

/// The permission gate for a *structural* form-field edit — placing,
/// moving, resizing, or restyling a field, as opposed to filling one in
/// (T-142's job, not this module's).
///
/// ISO 32000-1 Table 22 bit 6 ("Add or modify text annotations, fill in
/// interactive form fields") only covers filling in an *existing* field on
/// its own; the same bit's text continues "…and, if bit 4 is also set,
/// create or modify interactive form fields (including signature fields)".
/// A document can grant bit 6 without bit 4 — real and legal, and this
/// crate's `annotation_editing_is_allowed`/`content_editing_is_allowed`
/// already deliberately keep those two bits independent
/// (`core/pdf-manip/src/security.rs`) — so gating structural edits on the
/// annotate bit alone would let this shell create/move/resize/restyle
/// fields a document never actually authorized. Every entry point in this
/// module that records a structural command must call this, not
/// `Viewer::annotation_editing_refusal` directly.
pub(super) fn structural_edit_refusal(viewer: &Viewer) -> Option<&'static str> {
    viewer
        .annotation_editing_refusal()
        .or_else(|| viewer.content_edit_refusal())
}

/// Runs one form-field command against the open document, then reports the
/// outcome and repaints.
pub(super) fn command(
    viewer: &Viewer,
    operation: impl FnOnce(&mut DocumentSession) -> Result<String, String>,
) {
    if let Some(refusal) = structural_edit_refusal(viewer) {
        viewer.status.set_text(refusal);
        return;
    }
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
                session.unsaved_to_disk = true;
            }
            viewer.status.set_text(&message);
        }
        Err(error) => viewer.status.set_text(&error),
    }
    update_forms_controls(viewer);
    selection::redraw(viewer);
}

/// The editable model for the open document — mirrors
/// `annotations::command::model`. Reports `ANNOTATION_MODEL_UNAVAILABLE`
/// rather than a forms-specific message: this path is unreachable in
/// practice, since [`structural_edit_refusal`] already requires the
/// annotate permission (and with it the built model it depends on) before
/// [`command`] ever calls an operation that reaches here.
pub(super) fn model(session: &mut DocumentSession) -> Result<&mut Document, String> {
    session
        .document_model
        .as_mut()
        .ok_or_else(|| ANNOTATION_MODEL_UNAVAILABLE.to_string())
}

/// Records `command` in the document's own `EditLog` — mirrors
/// `annotations::command::apply_command`.
pub(super) fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}
