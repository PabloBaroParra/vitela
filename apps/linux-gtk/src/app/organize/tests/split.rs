//! The Organize header's "Split" control: where it sits, and that pressing it
//! reaches the chain rather than the page model.
//!
//! The chain's own refusals are `write::split`'s to test — it owns them and
//! tests them against a bare session. What belongs here is the half only this
//! screen can answer for: that the button exists in the header, in the right
//! place, and that it is wired to something that leaves the open document
//! completely alone.

use super::*;

#[gtk::test]
fn gtk_ui_split_sits_between_extract_and_save() {
    with_organize(|viewer| {
        assert_eq!(
            viewer.organize.split_button.label().as_deref(),
            Some("Split")
        );
        assert_eq!(
            viewer.organize.split_button.next_sibling(),
            Some(viewer.organize.save_button.clone().upcast())
        );
        // Beside Extract, because they are the same gesture in two shapes:
        // both read the open document and write new files somewhere else.
        assert_eq!(
            viewer.organize.extract_button.next_sibling(),
            Some(viewer.organize.split_button.clone().upcast())
        );
        // A header button, not a card one: a cut is a statement about the
        // whole document, not about the page it sits under.
        assert_eq!(
            viewer.organize.split_button.parent(),
            viewer.organize.save_button.parent()
        );
    });
}

#[gtk::test]
fn gtk_ui_split_is_described_by_what_it_produces() {
    with_organize(|viewer| {
        assert_eq!(
            viewer.organize.split_button.tooltip_text().as_deref(),
            Some("Cut this PDF into several new PDFs")
        );
    });
}

/// The property this shares with Extract and with nothing else on the screen.
///
/// Add PDFs, the per-card buttons and the drag all record a `Command` and
/// dirty the session. A split records nothing: it builds new files out of
/// *clones* of the model, so the page order, the undo history and the dirty
/// flag must all be exactly where they were. Driven through a refused path so
/// no modal is raised on the shared test main loop — the refusal returns
/// before the dialog, and what is asserted is what it left behind.
#[gtk::test]
fn gtk_ui_a_refused_split_leaves_the_document_untouched() {
    with_organize(|viewer| {
        session(viewer).text_access = crate::app::state::TextAccess::Forbidden;

        viewer.organize.split_button.emit_clicked();

        assert_grid(viewer, &[0, 1, 2]);
        assert_eq!(session(viewer).edit_revision, 0);
        assert!(!session(viewer).unsaved_to_disk);
        assert!(!viewer.undo_action.is_enabled());
        assert_eq!(
            viewer.status.text().as_str(),
            crate::app::state::TextAccess::Forbidden
                .refusal()
                .expect("a forbidden document refuses")
        );
    });
}
