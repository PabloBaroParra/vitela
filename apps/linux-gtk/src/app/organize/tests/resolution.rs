//! How a canvas page index resolves to a page while the model and the open
//! pdfium handle disagree.
//!
//! Split out of the grid/history tests beside them: those exercise widgets,
//! these pin the index contract every other feature reads through — see
//! `crate::app::state::DocumentSession::backend_pages`.

use super::*;

/// A recorded page op reorders `Document.pages` while the open pdfium handle
/// still holds the pre-op order, so the two disagree until the preview
/// refresh lands. Everything that turns a canvas page index into a page —
/// hit-testing, drawing, content-edit parsing — has to follow the *handle*
/// during that window, not the model.
#[gtk::test]
fn gtk_ui_a_recorded_move_does_not_move_the_backend_pages_until_the_refresh_lands() {
    with_organize(|viewer| {
        assert!(drop_on(viewer, 2, 0));
        assert_grid(viewer, &[2, 0, 1]);

        {
            let session = session(viewer);
            // The model now starts with page 2, but the handle was never
            // reopened: canvas position 0 is still the page it always was.
            assert_eq!(session.backend_page_id(0), Some(PageId(0)));
            assert_eq!(
                crate::app::content_edit::base_page(&session, 0),
                Some(PageId(0)),
                "a canvas index must resolve through the open handle, not the model order"
            );
        }

        // Landing the refresh is what puts the two back in agreement —
        // `document::restore_edit_state` re-installs this from the model the
        // reopened bytes were written from.
        {
            let mut session = session(viewer);
            let reopened: Vec<PageId> = session
                .document_model
                .as_ref()
                .unwrap()
                .pages
                .iter()
                .map(|page| page.id)
                .collect();
            session.backend_pages = reopened;
        }

        let session = session(viewer);
        assert_eq!(session.backend_page_id(0), Some(PageId(2)));
        assert_eq!(
            crate::app::content_edit::base_page(&session, 0),
            Some(PageId(2))
        );
    });
}

/// A page with no page in the base document — a blank one inserted this
/// session, and later an imported one — has nothing there to parse or probe
/// against, so content-edit refuses it instead of addressing whatever
/// unrelated page sits at the same number.
#[gtk::test]
fn gtk_ui_a_page_with_no_base_page_resolves_to_nothing() {
    with_organize(|viewer| {
        assert!(command(viewer, |session| {
            let document = model(session)?;
            let page = Page::blank(PageId(3), PageSize::A4, PageOrientation::Portrait);
            apply_command(document, Command::insert_page(0, page));
            Ok("Inserted page.".into())
        }));

        let mut session = session(viewer);
        let reopened: Vec<PageId> = session
            .document_model
            .as_ref()
            .unwrap()
            .pages
            .iter()
            .map(|page| page.id)
            .collect();
        session.backend_pages = reopened;

        assert_eq!(session.backend_page_id(0), Some(PageId(3)));
        assert_eq!(crate::app::content_edit::base_page(&session, 0), None);
        assert_eq!(
            crate::app::content_edit::base_page(&session, 1),
            Some(PageId(0))
        );
    });
}
