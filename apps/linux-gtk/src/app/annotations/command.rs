//! The one door from a UI action to the document's undoable command log.
//!
//! Every annotation mutation in this shell goes through [`command`], and the
//! history actions that replay them live here too — undo is the same log seen
//! from the other end.

use gtk::prelude::*;
use pdf_document::{AnnotationId, Command, Document, FormFieldId};

use crate::app::document::refresh_after_content_edit;
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
    let outcome = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(document) = session.document_model.as_mut() else {
            return;
        };
        // Peeked *before* stepping: `EditLog::peek_undo`/`peek_redo` are
        // read-only (T-163), so this looks at the command about to move
        // without consuming it — `step_history` still has to see it fresh a
        // moment later to actually apply the inverse.
        let is_content_edit = if undo {
            document.pending_edits.peek_undo()
        } else {
            document.pending_edits.peek_redo()
        }
        .is_some_and(Command::is_content_edit);

        match step_history(document, session.selected_annotation, undo) {
            Some(surviving) => {
                session.selected_annotation = surviving;
                // The log is shared: an undo/redo step here may just as
                // easily have moved a form-field command (T-141) as an
                // annotation one, and `step_history` only ever resolves
                // `selected_annotation`. Without this, undoing an
                // `AddFormField` leaves `selected_form_field` pointing at an
                // id `document.form_fields` no longer holds — the style
                // inspector stays sensitive on a field that is gone, and the
                // next restyle attempt fails with "no longer exists" instead
                // of the inspector simply going blank. Same reasoning as
                // `step_history`'s own filter, just for the other selection.
                session.selected_form_field =
                    surviving_form_field(document, session.selected_form_field);
                session.edit_revision += 1;
                // Unconditional, content edit or not. A content command's
                // refresh (`document::refresh_after_content_edit`) does
                // re-assert this on the session it installs, but only when
                // it succeeds — and a step that has already moved the log is
                // an unsaved change whether or not the preview caught up
                // with it.
                session.unsaved_to_disk = true;
                Some(is_content_edit)
            }
            None => None,
        }
    };

    let Some(is_content_edit) = outcome else {
        return;
    };

    // Runs for both kinds and before the branch below: the log has already
    // moved, so Undo/Redo sensitivity is stale right now. A content edit's
    // refresh re-runs this once its reopen lands, but it may also fail — and
    // a failed refresh must not leave the toolbar describing a history that
    // no longer exists.
    update_annotation_controls(viewer);
    // The forms inspector's sensitivity and displayed style depend on
    // `selected_form_field`, which the step above may just have cleared —
    // see that assignment's own comment.
    crate::app::forms::update_forms_controls(viewer);
    selection::redraw(viewer);

    // Only a full refresh shows the real result of undoing/redoing a content
    // edit (T-163, decision 6) — an annotation's overlay already painted the
    // truth in the `redraw` above without one.
    if is_content_edit {
        refresh_after_content_edit(viewer, if undo { "Edit undone." } else { "Edit redone." });
    } else {
        viewer.status.set_text(if undo {
            "Edit undone. Changes are pending save."
        } else {
            "Edit redone. Changes are pending save."
        });
    }
}

/// Replays one step of `document`'s edit history and reports the selection
/// that survives it.
///
/// `None` means the log had nothing to replay in that direction, and the
/// caller must leave the session alone — no revision bump, no repaint.
///
/// `Some(surviving)` carries the selection after the step, which is `None`
/// when the step took the selected annotation out of the document: a
/// selection that outlived its annotation would aim the next edit at an id
/// the document no longer holds.
///
/// Split out of [`history`] because everything above it is widget work and
/// everything here is not — this half is the part worth testing.
fn step_history(
    document: &mut Document,
    selected: Option<AnnotationId>,
    undo: bool,
) -> Option<Option<AnnotationId>> {
    // Moved out and put back for the same reason as `apply_command`: the log
    // lives inside the document that `undo`/`redo` need to borrow mutably.
    let mut log = std::mem::take(&mut document.pending_edits);
    let changed = if undo {
        log.undo(document)
    } else {
        log.redo(document)
    };
    document.pending_edits = log;
    changed.then(|| selected.filter(|id| document.annotations.get(*id).is_some()))
}

/// The form-field selection that survives an undo/redo step already applied
/// to `document` — the T-141 twin of `step_history`'s own annotation filter,
/// pulled out separately because the log is shared: whichever kind of
/// command the step actually replayed, both selections need re-checking
/// against the document it left behind, not just the one `step_history`
/// already knows about.
fn surviving_form_field(document: &Document, selected: Option<FormFieldId>) -> Option<FormFieldId> {
    selected.filter(|id| document.form_fields.get(*id).is_some())
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
                // Annotation edits never go through `refresh_after_content_edit`'s
                // reopen, so this is the only place that marks them unsaved.
                session.unsaved_to_disk = true;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::annotations::test_support::{built, drag};
    use crate::app::state::Tool;
    use pdf_document::{Annotation, AnnotationId};

    /// A real built annotation, re-identified so a test can hold two.
    fn annotation(id: u64) -> Annotation {
        let mut annotation = built(&drag(Tool::Highlight, (10.0, 10.0), (60.0, 40.0)));
        annotation.id = AnnotationId(id);
        annotation
    }

    /// A document whose log already records adding `annotation` — seeded
    /// through `apply_command`, so the tests replay what the shell records.
    fn with_added(annotation: &Annotation) -> Document {
        let mut document = Document::blank();
        apply_command(&mut document, Command::AddAnnotation(annotation.clone()));
        document
    }

    #[test]
    fn undo_then_redo_round_trips_the_recorded_command() {
        let annotation = annotation(1);
        let mut document = with_added(&annotation);

        assert!(step_history(&mut document, None, true).is_some());
        assert!(document.annotations.is_empty());

        assert!(step_history(&mut document, None, false).is_some());
        assert_eq!(document.annotations.get(AnnotationId(1)), Some(&annotation));
    }

    #[test]
    fn undo_on_an_empty_log_reports_no_step() {
        let mut document = Document::blank();

        assert_eq!(step_history(&mut document, None, true), None);
    }

    #[test]
    fn redo_without_a_prior_undo_reports_no_step() {
        let mut document = with_added(&annotation(1));

        assert_eq!(step_history(&mut document, None, false), None);
    }

    /// The log lives inside the document it mutates and is moved out for the
    /// call — if it were not put back, the next step would find nothing.
    #[test]
    fn the_log_stays_on_the_document_across_a_step() {
        let mut document = with_added(&annotation(1));

        step_history(&mut document, None, true);

        assert!(document.pending_edits.can_redo());
        assert!(!document.pending_edits.can_undo());
    }

    #[test]
    fn undoing_an_add_drops_the_selection_it_pointed_at() {
        let annotation = annotation(1);
        let mut document = with_added(&annotation);

        let surviving = step_history(&mut document, Some(annotation.id), true);

        // A selection that outlived its annotation would let the next edit
        // target an id the document no longer holds.
        assert_eq!(surviving, Some(None));
    }

    #[test]
    fn redoing_a_removal_drops_the_selection_it_pointed_at() {
        let annotation = annotation(7);
        let mut document = Document::blank();
        document.annotations.insert(annotation.clone());
        apply_command(&mut document, Command::RemoveAnnotation(annotation.clone()));
        step_history(&mut document, None, true);

        let surviving = step_history(&mut document, Some(annotation.id), false);

        assert_eq!(surviving, Some(None));
    }

    #[test]
    fn undoing_a_removal_keeps_the_restored_selection() {
        let annotation = annotation(7);
        let mut document = Document::blank();
        document.annotations.insert(annotation.clone());
        apply_command(&mut document, Command::RemoveAnnotation(annotation.clone()));

        let surviving = step_history(&mut document, Some(annotation.id), true);

        assert_eq!(surviving, Some(Some(AnnotationId(7))));
        assert_eq!(document.annotations.get(AnnotationId(7)), Some(&annotation));
    }

    #[test]
    fn a_step_elsewhere_leaves_an_untouched_selection_alone() {
        let kept = annotation(1);
        let undone = annotation(2);
        let mut document = Document::blank();
        document.annotations.insert(kept.clone());
        apply_command(&mut document, Command::AddAnnotation(undone.clone()));

        let surviving = step_history(&mut document, Some(kept.id), true);

        assert_eq!(surviving, Some(Some(AnnotationId(1))));
    }

    #[test]
    fn a_step_with_no_selection_reports_none_surviving() {
        let mut document = with_added(&annotation(1));

        assert_eq!(step_history(&mut document, None, true), Some(None));
    }

    fn a_form_field(id: u64) -> pdf_document::FormField {
        pdf_document::FormField {
            id: FormFieldId(id),
            page: pdf_document::PageId(0),
            name: format!("Text_{id}"),
            rect: pdf_document::Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            style: pdf_document::TextStyle {
                font: pdf_document::FontFamily::Helvetica,
                size_pt: 12.0,
                color: pdf_document::Color { r: 0, g: 0, b: 0 },
            },
            value: pdf_document::FieldValue::Text(String::new()),
            kind: pdf_document::FormFieldKind::Text {
                multiline: false,
                max_len: None,
            },
            origin: pdf_document::FieldOrigin::New,
        }
    }

    #[test]
    fn a_field_still_in_the_document_survives() {
        let mut document = Document::blank();
        document.form_fields.insert(a_form_field(1));

        assert_eq!(
            surviving_form_field(&document, Some(FormFieldId(1))),
            Some(FormFieldId(1))
        );
    }

    /// The bug this function exists to fix: undoing the `AddFormField` that
    /// created the selected field must drop the selection along with it, the
    /// same way `step_history` already does for an annotation — otherwise
    /// the style inspector stays sensitive on a field the document no longer
    /// holds.
    #[test]
    fn a_field_removed_by_the_step_no_longer_survives() {
        let document = Document::blank();

        assert_eq!(surviving_form_field(&document, Some(FormFieldId(1))), None);
    }

    #[test]
    fn no_selection_survives_as_no_selection() {
        let document = Document::blank();

        assert_eq!(surviving_form_field(&document, None), None);
    }
}
