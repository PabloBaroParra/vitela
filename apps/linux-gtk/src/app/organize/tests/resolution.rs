//! How a canvas page index resolves to a page while the model and the open
//! pdfium handle disagree.
//!
//! Split out of the grid/history tests beside them: those exercise widgets,
//! these pin the index contract every other feature reads through — see
//! `crate::app::state::DocumentSession::backend_pages`.

use super::*;

/// The fixture session carries no `save_backing` (`test_fixtures::
/// model_session`), and a probe needs one — it is the document a base page
/// resolves into. Three pages, matching the three the fixture models.
fn base_backing() -> crate::app::state::SaveBacking {
    crate::app::state::SaveBacking {
        base: pdf_manip::LopdfDocument::from_lopdf(gen_fixtures::build_multi_page_document(
            3, "base",
        )),
        original_bytes: Vec::new(),
        password: None,
    }
}

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
                crate::app::content_edit::content_page(&session, 0),
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
            crate::app::content_edit::content_page(&session, 0),
            Some(PageId(2))
        );
    });
}

/// A blank page inserted this session has no object anywhere until a save
/// materializes it, so there is nothing to parse and nothing a probe could
/// run against — content-edit refuses it instead of addressing whatever
/// unrelated page sits at the same number.
#[gtk::test]
fn gtk_ui_a_blank_page_resolves_to_nothing() {
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
        assert_eq!(crate::app::content_edit::content_page(&session, 0), None);
        assert_eq!(
            crate::app::content_edit::content_page(&session, 1),
            Some(PageId(0))
        );
    });
}

/// An imported page **does** resolve, unlike a blank one: its bytes are in
/// the PDF it was imported from, which `content_edit::page_probe` reaches
/// through the page's own `PageOrigin` (batch PDF assembly §6). Resolving it
/// positionally against the base document — what content editing used to
/// do — could not have reached it at all: its id was allocated past every
/// base page's.
#[gtk::test]
fn gtk_ui_an_imported_page_probes_against_the_source_it_came_from() {
    with_organize(|viewer| {
        let token = {
            let state = viewer.state.borrow();
            let session = state.session.as_ref().unwrap();
            crate::app::state::SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            }
        };
        let source =
            pdf_manip::LopdfDocument::from_lopdf(gen_fixtures::build_multi_line_page_document(&[
                "imported line",
            ]));
        let sources = vec![crate::app::state::ImportedSource {
            id: ImportedDocumentId(7),
            document: source,
        }];
        let pages = vec![Page::imported(
            PageId(3),
            ImportedDocumentId(7),
            0,
            PageSize::A4,
            PageOrientation::Portrait,
            pdf_document::Rotation::None,
        )];
        super::import::apply_prepared(viewer, token, sources, pages);

        let mut session = session(viewer);
        session.save_backing = Some(base_backing());
        let reopened: Vec<PageId> = session
            .document_model
            .as_ref()
            .unwrap()
            .pages
            .iter()
            .map(|page| page.id)
            .collect();
        session.backend_pages = reopened;
        let canvas_index = session
            .backend_pages
            .iter()
            .position(|id| *id == PageId(3))
            .expect("the imported page is on the canvas");

        assert_eq!(
            crate::app::content_edit::content_page(&session, canvas_index),
            Some(PageId(3)),
            "an imported page is content-editable"
        );

        let base = &session.save_backing.as_ref().unwrap().base;
        let document = session.document_model.as_ref().unwrap();
        let probe = crate::app::content_edit::page_probe(
            document,
            base,
            &session.imported_sources,
            PageId(3),
        )
        .expect("the imported page resolves to its source");
        let text: Vec<String> =
            pdf_edit::read_page_object_content(probe.document, probe.object, PageId(3))
                .expect("the source page parses")
                .text_runs
                .into_iter()
                .map(|run| run.text)
                .collect();
        assert_eq!(
            text,
            vec!["imported line".to_string()],
            "the probe must reach the source's bytes, not a base page"
        );
    });
}

/// The refusal that replaces the old positional one: a page the session has
/// no source registered for gets no probe, so no command is ever validated
/// against the wrong document.
#[gtk::test]
fn gtk_ui_an_imported_page_with_no_registered_source_gets_no_probe() {
    with_organize(|viewer| {
        assert!(command(viewer, |session| {
            let document = model(session)?;
            document.pages.push(Page::imported(
                PageId(9),
                ImportedDocumentId(42),
                0,
                PageSize::A4,
                PageOrientation::Portrait,
                pdf_document::Rotation::None,
            ));
            Ok("Inserted page.".into())
        }));

        let mut session = session(viewer);
        session.save_backing = Some(base_backing());
        let base = &session.save_backing.as_ref().unwrap().base;
        let document = session.document_model.as_ref().unwrap();
        assert!(crate::app::content_edit::page_probe(
            document,
            base,
            &session.imported_sources,
            PageId(9),
        )
        .is_none());
    });
}
