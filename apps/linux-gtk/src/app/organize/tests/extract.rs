//! The Organize header's "Extract" control: where it sits, and that pressing
//! it reaches the chain rather than the page model.
//!
//! The chain's own refusals are `write::extract`'s to test — it owns them and
//! tests them against a bare session. What belongs here is the half only this
//! screen can answer for: that the button exists in the header, in the right
//! place, and that it is wired to something that leaves the open document
//! completely alone.

use super::*;

#[gtk::test]
fn gtk_ui_extract_sits_between_the_import_group_and_split() {
    with_organize(|viewer| {
        assert_eq!(
            viewer.organize.extract_button.label().as_deref(),
            Some("Extract")
        );
        assert_eq!(
            viewer.organize.extract_button.next_sibling(),
            Some(viewer.organize.split_button.clone().upcast())
        );
        // A header button, not a card one: it acts on a selection that can
        // span the whole document rather than on the page it sits under.
        assert_eq!(
            viewer.organize.extract_button.parent(),
            viewer.organize.save_button.parent()
        );
    });
}

#[gtk::test]
fn gtk_ui_extract_is_described_by_what_it_produces() {
    with_organize(|viewer| {
        assert_eq!(
            viewer.organize.extract_button.tooltip_text().as_deref(),
            Some("Save chosen pages as a new PDF")
        );
    });
}

/// The property that separates this from every other control on the screen.
///
/// Add PDFs, the per-card buttons and the drag all record a `Command` and
/// dirty the session. An extraction records nothing: it builds a second file
/// out of a *clone* of the model, so the page order, the undo history and the
/// dirty flag must all be exactly where they were. Driven through a refused
/// path so no modal is raised on the shared test main loop — the refusal
/// returns before the dialog, and what is asserted is what it left behind.
#[gtk::test]
fn gtk_ui_a_refused_extraction_leaves_the_document_untouched() {
    with_organize(|viewer| {
        session(viewer).text_access = crate::app::state::TextAccess::Forbidden;

        viewer.organize.extract_button.emit_clicked();

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
