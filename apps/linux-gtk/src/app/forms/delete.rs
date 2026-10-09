//! Deleting the selected form field — the Linux twin of Windows'
//! `MainWindow.FormDelete`. One `RemoveFormField` removes the field and its
//! widgets as a single undo step; the command carries the whole field, so
//! undo puts back exactly what was there.
//!
//! Gated like every other edit to a field's definition
//! (`command::structural_edit_refusal`): deleting a field is "modify
//! interactive form fields", not filling one in.

use pdf_document::Command;

use crate::app::state::{DocumentSession, Viewer};

use super::command::{apply_command, command, model};
use super::SELECTION_GONE;

/// Deletes the selected field, if any. The selection goes with it: a field
/// that no longer exists cannot stay selected, and leaving the id behind
/// would let the style inspector act on whatever undo brings back.
pub(super) fn delete_selected(viewer: &Viewer) {
    let Some(id) = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.selected_form_field)
    else {
        return;
    };
    command(viewer, move |session| remove_field(session, id));
}

fn remove_field(
    session: &mut DocumentSession,
    id: pdf_document::FormFieldId,
) -> Result<String, String> {
    let document = model(session)?;
    let field = document
        .form_fields
        .get(id)
        .cloned()
        .ok_or_else(|| SELECTION_GONE.to_string())?;
    apply_command(document, Command::RemoveFormField(field));
    session.selected_form_field = None;
    Ok("Field deleted. Save to keep the change, or undo to bring it back.".to_string())
}

#[cfg(test)]
mod tests {
    use pdf_document::{Document, FormFieldId};

    use super::*;
    use crate::app::test_fixtures::{a_form_field, model_session};

    fn session_with_selected_field() -> DocumentSession {
        let mut document = Document::default();
        document.form_fields.insert(a_form_field(7));
        let mut session = model_session(document);
        session.selected_form_field = Some(FormFieldId(7));
        session
    }

    #[test]
    fn removing_a_field_drops_it_from_the_model_and_clears_the_selection() {
        let mut session = session_with_selected_field();

        remove_field(&mut session, FormFieldId(7)).expect("the field exists");

        let document = session.document_model.as_ref().unwrap();
        assert!(document.form_fields.get(FormFieldId(7)).is_none());
        assert_eq!(session.selected_form_field, None);
    }

    #[test]
    fn undo_brings_the_deleted_field_back_whole() {
        let mut session = session_with_selected_field();
        let before = a_form_field(7);

        remove_field(&mut session, FormFieldId(7)).expect("the field exists");
        let document = session.document_model.as_mut().unwrap();
        let mut log = std::mem::take(&mut document.pending_edits);
        assert!(log.undo(document));
        document.pending_edits = log;

        assert_eq!(document.form_fields.get(FormFieldId(7)), Some(&before));
    }

    #[test]
    fn a_vanished_field_is_refused_without_recording_anything() {
        let mut session = session_with_selected_field();

        let error = remove_field(&mut session, FormFieldId(99)).unwrap_err();

        assert_eq!(error, SELECTION_GONE);
        let document = session.document_model.as_ref().unwrap();
        assert!(document.form_fields.get(FormFieldId(7)).is_some());
        assert!(!document.pending_edits.can_undo());
        assert_eq!(session.selected_form_field, Some(FormFieldId(7)));
    }
}
