//! The Organize screen's edits: the permission funnel every page op goes
//! through, and the `Command::MovePage`/`RemovePage` recordings themselves.
//!
//! The twin of [`super::grid`] — that half owns the widgets, this one owns
//! what reaches `Document.pages`. Mirrors `annotations::command` and
//! `forms::command`, which split the same way for the same reason.

use pdf_document::{Command, Document};

use crate::app::state::{DocumentSession, Viewer, CONTENT_MODEL_UNAVAILABLE};

use super::NO_DOCUMENT;

pub(super) fn command(
    viewer: &Viewer,
    operation: impl FnOnce(&mut DocumentSession) -> Result<String, String>,
) -> bool {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return false;
    }
    // Every operation that reaches this funnel changes the page list, which
    // is the PDF document-assembly permission (`/P` bit 11) and not the
    // modify-contents bit checked just above. A document can grant one and
    // withhold the other, so asking only the first would repaginate a file
    // that forbids exactly that — see `pdf_manip::document_assembly_is_allowed`.
    if let Some(refusal) = viewer.page_assembly_refusal() {
        viewer.status.set_text(refusal);
        return false;
    }
    // Permission granted is not the same as result writable. Moving or
    // deleting a page changes the page set, which puts the save on the
    // full-rewrite writer, and an encrypted document opened with only one of
    // its two passwords cannot be re-encrypted at all. Asked here rather than
    // at the save it would fail: the refresh below saves a snapshot the same
    // way, so an edit that can never be written would not merely wait to fail
    // — it would take the preview down with it on the very next operation.
    if let Some(refusal) = viewer.full_rewrite_refusal() {
        viewer.status.set_text(refusal);
        return false;
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
            crate::app::annotations::update_annotation_controls(viewer);
            // A page op moves `Document.pages` out from under the open pdfium
            // handle, which still holds the pre-op order. Materializing it now
            // — the same in-memory save+reopen a content edit runs — is what
            // puts the two back in agreement; without it every page index the
            // canvas produces would keep naming the old page, and every id
            // resolved through `DocumentSession::backend_pages` would name the
            // new one. Bails out on its own (leaving the message above intact)
            // when there is no `save_backing` to replay against.
            crate::app::document::refresh_preview(viewer, message);
            true
        }
        Err(error) => {
            viewer.status.set_text(&error);
            false
        }
    }
}

pub(super) fn model(session: &mut DocumentSession) -> Result<&mut Document, String> {
    session
        .document_model
        .as_mut()
        .ok_or_else(|| CONTENT_MODEL_UNAVAILABLE.to_string())
}

/// Returns `false` when `EditLog::apply` rejected the command, leaving both
/// the document and the log untouched. Every caller turns that into an `Err`
/// rather than dropping it: a rejected command records nothing, so reporting
/// success would leave the status line — and the undo stack — describing an
/// edit that never happened.
pub(super) fn apply_command(document: &mut Document, command: Command) -> bool {
    let mut log = std::mem::take(&mut document.pending_edits);
    let applied = log.apply(document, command);
    document.pending_edits = log;
    applied
}

pub(super) fn move_page(viewer: &Viewer, from: usize, to: usize) -> bool {
    if from == to {
        return false;
    }
    command(viewer, |session| {
        let document = model(session)?;
        if from >= document.pages.len() || to >= document.pages.len() {
            return Err("Invalid page position.".to_string());
        }
        if !apply_command(document, Command::MovePage { from, to }) {
            return Err("Could not move the page.".to_string());
        }
        Ok(format!("Moved page {} to position {}.", from + 1, to + 1))
    })
}

pub(super) fn delete_page(viewer: &Viewer, index: usize) -> bool {
    command(viewer, |session| {
        let document = model(session)?;
        // `Command::remove_page`, not a `RemovePage` literal: it captures the
        // annotations and form fields anchored to the page as well, so undo
        // brings the page back with what was drawn on it, and the removal
        // cannot strand an annotation on a page id `pdf-save` will refuse to
        // write (which would leave the document unsaveable, not just untidy).
        let removal = Command::remove_page(document, index)
            .ok_or_else(|| "Page no longer exists.".to_string())?;
        if !apply_command(document, removal) {
            return Err("Could not delete the page.".to_string());
        }
        Ok(format!("Deleted page {}.", index + 1))
    })
}
