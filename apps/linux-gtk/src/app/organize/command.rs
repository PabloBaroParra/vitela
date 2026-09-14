//! The Organize screen's edits: the permission funnels every page op goes
//! through, and the `Command::MovePage`/`RemovePage`/`RotatePage` recordings
//! themselves.
//!
//! The twin of [`super::grid`] — that half owns the widgets, this one owns
//! what reaches `Document.pages`. Mirrors `annotations::command` and
//! `forms::command`, which split the same way for the same reason.
//!
//! ## Why there are two funnels and not one
//!
//! Every operation here needs the PDF document-assembly permission, and every
//! operation here ends the same way — record, mark dirty, re-materialize the
//! preview. What differs is the *middle*: [`command`] serves the operations
//! that change which pages the document has or what order they are in, and
//! those force `pdf-save`'s full-rewrite writer; [`rotation_command`] serves
//! the one operation that does neither.
//!
//! That difference is a permission, not a nicety. A quarter-turn stays on the
//! incremental writer, which re-encrypts from lopdf's own retained state and
//! needs no password of ours, so asking [`Viewer::full_rewrite_refusal`] there
//! would refuse a rotation that saves perfectly — see that method's own doc
//! and `docs/batch-pdf-assembly.md` section 5. It is also why a rotation does
//! not ask `content_edit_refusal`: `pdf_manip::document_assembly_is_allowed`
//! already accepts *either* `/P` bit 11 or the modify-contents bit, so asking
//! the narrower question first would invent a restriction on a document that
//! granted assembly alone.

use pdf_document::{Command, Document, PageId};

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
    commit(viewer, operation)
}

/// The narrower funnel: the assembly permission and nothing else, for the one
/// operation that changes a page's angle without changing the page list. See
/// this module's header for why the other two gates are deliberately absent.
fn rotation_command(
    viewer: &Viewer,
    operation: impl FnOnce(&mut DocumentSession) -> Result<String, String>,
) -> bool {
    if let Some(refusal) = viewer.page_assembly_refusal() {
        viewer.status.set_text(refusal);
        return false;
    }
    commit(viewer, operation)
}

/// What both funnels do once their gates are clear: run `operation` against
/// the session, and on success mark the document edited and re-materialize
/// the preview from the model.
fn commit(
    viewer: &Viewer,
    operation: impl FnOnce(&mut DocumentSession) -> Result<String, String>,
) -> bool {
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
            crate::app::write::refresh_preview(viewer, message);
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

/// Moves the `count` pages starting at `from` so the run begins at `to`,
/// recording one `Command::MovePages` — the Documents view's drag, and its
/// Move up/Move down buttons.
///
/// One command, not `count` `MovePage`s: the checklist asks a block move to
/// be a single undo step, and a run of per-page moves would also leave the
/// document in an interleaved half-moved state if one of them were rejected
/// partway through.
pub(super) fn move_block(viewer: &Viewer, from: usize, count: usize, to: usize) -> bool {
    if from == to || count == 0 {
        return false;
    }
    command(viewer, |session| {
        let document = model(session)?;
        if !apply_command(document, Command::MovePages { from, count, to }) {
            return Err("Could not move the document.".to_string());
        }
        Ok(format!(
            "Moved {count} page{} to position {}.",
            if count == 1 { "" } else { "s" },
            to + 1
        ))
    })
}

/// Deletes the `count` pages starting at `index` as one undoable step.
pub(super) fn delete_block(viewer: &Viewer, index: usize, count: usize) -> bool {
    command(viewer, |session| {
        let document = model(session)?;
        // `Command::remove_pages` for the same reason `delete_page` uses
        // `Command::remove_page`: it captures the annotations and form fields
        // anchored to *every* page of the run, so undo brings the block back
        // with what was drawn on it and the removal cannot strand an
        // annotation on a page id `pdf-save` would then refuse to write.
        let removal = Command::remove_pages(document, index, count)
            .ok_or_else(|| "Those pages no longer exist.".to_string())?;
        if !apply_command(document, removal) {
            return Err("Could not delete the document.".to_string());
        }
        Ok(format!(
            "Deleted {count} page{}.",
            if count == 1 { "" } else { "s" }
        ))
    })
}

/// A quarter-turn in either direction: `-90` anticlockwise, `90` clockwise.
///
/// Takes the page's **id** and not its grid position, for the same reason the
/// drag payload does (see `super::grid`'s header): a card's position is only
/// current until something else moves, and the Undo and Redo buttons sit in
/// this screen's own header.
///
/// The angle in `Document.pages` is absolute and the command carries a delta,
/// so this records the user's gesture rather than a computed target — which is
/// what lets four clockwise clicks undo back through 270, 180 and 90 instead
/// of collapsing into one step at zero.
pub(super) fn rotate_page(viewer: &Viewer, page: PageId, delta_degrees: i32) -> bool {
    let rotated = rotation_command(viewer, |session| {
        let document = model(session)?;
        // Validated here rather than left to `EditLog::apply`, which is the
        // one command that cannot report this for itself: `RotatePage` on an
        // id the document does not hold applies cleanly to nothing at all and
        // returns `true`, so recording it would put a step in the undo stack
        // that changes nothing and a message on the status line that lies.
        let position = document
            .pages
            .iter()
            .position(|candidate| candidate.id == page)
            .ok_or_else(|| "Page no longer exists.".to_string())?;
        if !apply_command(
            document,
            Command::RotatePage {
                page,
                delta_degrees,
            },
        ) {
            return Err("Could not rotate the page.".to_string());
        }
        Ok(format!(
            "Rotated page {} {}.",
            position + 1,
            if delta_degrees < 0 { "left" } else { "right" }
        ))
    });
    if rotated {
        // Before the reopen `commit` has just scheduled, and only for this
        // page: a turn changes what one page *looks like* without changing
        // which page it is, so its cached pixels still match a key that is
        // still correct. See `super::invalidate_page_thumbnail`.
        super::invalidate_page_thumbnail(viewer, page);
    }
    rotated
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
