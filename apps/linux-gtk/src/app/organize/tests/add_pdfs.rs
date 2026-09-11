//! The Organize header's "Add PDFs" control: where it sits, what cancelling
//! leaves behind, and the single history step a multi-file import records.
//!
//! Split out of [`super`], which keeps the fixtures these share. Named for
//! the button rather than for `organize::import`, which it drives: a test
//! module called `import` would shadow that one for every sibling here,
//! since they all reach it as `super::import`.

use super::*;

#[gtk::test]
fn gtk_ui_header_exposes_add_pdfs_before_save() {
    with_organize(|viewer| {
        assert_eq!(
            viewer.organize.add_pdfs_button.label().as_deref(),
            Some("Add PDFs")
        );
        assert_eq!(
            viewer.organize.add_pdfs_button.next_sibling(),
            Some(viewer.organize.import_progress.clone().upcast())
        );
        assert_eq!(
            viewer.organize.cancel_import_button.next_sibling(),
            Some(viewer.organize.save_button.clone().upcast())
        );
        assert!(!viewer.organize.import_progress.is_visible());
        assert!(!viewer.organize.cancel_import_button.is_visible());
    });
}

#[gtk::test]
fn gtk_ui_cancel_import_restores_controls_without_dirtying_the_session() {
    with_organize(|viewer| {
        let cancellation = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        viewer.state.borrow_mut().import_cancellation = Some(cancellation.clone());
        viewer.organize.add_pdfs_button.set_sensitive(false);
        viewer.organize.import_progress.set_visible(true);
        viewer.organize.cancel_import_button.set_visible(true);

        viewer.organize.cancel_import_button.emit_clicked();

        assert!(cancellation.load(std::sync::atomic::Ordering::Acquire));
        assert!(viewer.organize.add_pdfs_button.is_sensitive());
        assert!(!viewer.organize.import_progress.is_visible());
        assert!(!viewer.organize.cancel_import_button.is_visible());
        assert_eq!(session(viewer).edit_revision, 0);
        assert!(!session(viewer).unsaved_to_disk);
    });
}

#[gtk::test]
fn gtk_ui_document_change_actively_cancels_import() {
    with_organize(|viewer| {
        let cancellation = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        viewer.state.borrow_mut().import_cancellation = Some(cancellation.clone());
        viewer.organize.add_pdfs_button.set_sensitive(false);
        viewer.organize.import_progress.set_visible(true);
        viewer.organize.cancel_import_button.set_visible(true);

        document_changed(viewer);

        assert!(cancellation.load(std::sync::atomic::Ordering::Acquire));
        assert!(viewer.state.borrow().import_cancellation.is_none());
        assert!(viewer.organize.add_pdfs_button.is_sensitive());
        assert!(!viewer.organize.import_progress.is_visible());
        assert!(!viewer.organize.cancel_import_button.is_visible());
    });
}

#[gtk::test]
fn gtk_ui_multi_source_import_is_one_dirty_history_step() {
    with_organize(|viewer| {
        let token = {
            let state = viewer.state.borrow();
            let session = state.session.as_ref().unwrap();
            crate::app::state::SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            }
        };
        let sources = [7, 8]
            .into_iter()
            .map(|id| crate::app::state::ImportedSource {
                id: ImportedDocumentId(id),
                document: pdf_manip::LopdfDocument::from_lopdf(lopdf::Document::new()),
                name: format!("source-{id}.pdf"),
            })
            .collect();
        let pages = vec![
            Page::imported(
                PageId(3),
                ImportedDocumentId(7),
                0,
                PageSize::A4,
                PageOrientation::Portrait,
                pdf_document::Rotation::None,
            ),
            Page::imported(
                PageId(4),
                ImportedDocumentId(8),
                0,
                PageSize::A4,
                PageOrientation::Portrait,
                pdf_document::Rotation::None,
            ),
        ];

        super::import::apply_prepared(viewer, token, sources, pages);

        assert_grid(viewer, &[0, 1, 2, 3, 4]);
        {
            let session = session(viewer);
            assert_eq!(session.imported_sources.len(), 2);
            assert_eq!(session.edit_revision, 1);
            assert!(session.unsaved_to_disk);
            assert!(session
                .document_model
                .as_ref()
                .unwrap()
                .pending_edits
                .can_undo());
        }
        viewer.undo_action.activate(None);
        assert_grid(viewer, &[0, 1, 2]);
        assert!(!session(viewer)
            .document_model
            .as_ref()
            .unwrap()
            .pending_edits
            .can_undo());
    });
}
