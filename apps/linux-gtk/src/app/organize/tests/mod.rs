//! Built-window behavior with thumbnail requests captured at the renderer boundary.

use std::cell::RefCell;

use gtk::{gdk_pixbuf, Picture};

use super::command::{apply_command, command, model, move_page};
use super::grid::populate_grid;
use super::*;
use crate::app::home::EDITOR_PAGE;
use crate::app::state::DocumentSession;
use crate::app::test_fixtures::{a_highlight, model_session};
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;
use pdf_document::{
    AnnotationId, Command, Document, ImportedDocumentId, Orientation as PageOrientation, Page,
    PageId, PageSize,
};

thread_local! {
    static THUMBNAILS: RefCell<Option<Vec<(u32, Picture)>>> = const { RefCell::new(None) };
}

pub(super) fn capture_thumbnail(index: u32, picture: &Picture) -> bool {
    THUMBNAILS.with_borrow_mut(|requests| {
        let Some(requests) = requests else {
            return false;
        };
        requests.push((index, picture.clone()));
        // Stands in for the render landing. The pixel is meaningless; what
        // matters is that the card now *has* a paintable, because
        // `fill_missing_thumbnails` reads exactly that to tell a card it must
        // render from one it must leave alone. Without it every card in a
        // test would look like a placeholder for ever.
        picture.set_pixbuf(Some(
            &gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, true, 8, 1, 1)
                .expect("a 1x1 pixbuf"),
        ));
        true
    })
}

/// How many thumbnail renders have been asked for since the capture was
/// armed. The point of most assertions below is that this does *not* move.
fn render_count() -> usize {
    THUMBNAILS.with_borrow(|requests| requests.as_ref().unwrap().len())
}

fn with_organize(test: impl FnOnce(&Viewer)) {
    let built = built_ui();
    let mut document = Document::blank();
    document.pages = (0..3)
        .map(|id| {
            Page::base(
                PageId(id),
                id,
                PageSize::A4,
                PageOrientation::Portrait,
                pdf_document::Rotation::None,
            )
        })
        .collect();
    built.viewer.state.borrow_mut().session = Some(model_session(document));
    THUMBNAILS.set(Some(Vec::new()));
    show(&built.viewer);
    built.window.present();
    // Teardown runs on the unwind path too. `#[gtk::test]` bodies all execute
    // on one shared main thread (`gtk::test_synced`), so `THUMBNAILS` is not
    // private to this test: a panic that skipped the reset would leave the
    // capture armed and swallow every later test's real thumbnail render,
    // turning one failure into a cascade of unrelated ones. Clearing the
    // session before closing also keeps the window clean, so the close never
    // raises the unsaved-changes prompt.
    let _teardown = Teardown(&built);
    test(&built.viewer);
}

struct Teardown<'a>(&'a BuiltUi);

impl Drop for Teardown<'_> {
    fn drop(&mut self) {
        THUMBNAILS.set(None);
        self.0.viewer.state.borrow_mut().session = None;
        self.0.window.close();
    }
}

fn history_button(viewer: &Viewer, label: &str) -> Button {
    let header = viewer.organize.save_button.parent().unwrap();
    let mut child = header.first_child();
    while let Some(widget) = child {
        match widget.downcast_ref::<Button>() {
            Some(button) if button.label().as_deref() == Some(label) => return button.clone(),
            _ => {}
        }
        child = widget.next_sibling();
    }
    panic!("missing {label} button beside Save");
}

fn delete_button(viewer: &Viewer, index: usize) -> Button {
    let footer = viewer.organize.cards.snapshot()[index]
        .root
        .last_child()
        .unwrap();
    footer.last_child().unwrap().downcast().unwrap()
}

fn session(viewer: &Viewer) -> std::cell::RefMut<'_, DocumentSession> {
    std::cell::RefMut::map(viewer.state.borrow_mut(), |state| {
        state.session.as_mut().unwrap()
    })
}

fn assert_grid(viewer: &Viewer, expected: &[u32]) {
    let session = session(viewer);
    let page_ids: Vec<PageId> = session
        .document_model
        .as_ref()
        .unwrap()
        .pages
        .iter()
        .map(|page| page.id)
        .collect();
    let ids: Vec<_> = page_ids.iter().map(|id| id.0).collect();
    assert_eq!(ids, expected);
    let grid = &viewer.organize.grid;
    let cards = viewer.organize.cards.snapshot();
    assert_eq!(cards.len(), expected.len());
    for (index, card) in cards.iter().enumerate() {
        assert_eq!(card.number.text(), (index + 1).to_string());
        let child = grid.child_at_index(index as i32).unwrap().child().unwrap();
        assert_eq!(&child, card.root.upcast_ref::<gtk::Widget>());
        let picture = card.picture.clone();
        // A card's thumbnail is asked for by the page's position in the *open
        // handle*, never by its id — the two stop being the same number the
        // moment a page op is recorded, and an imported page's id was never a
        // position at all. A page the handle does not hold (an insert whose
        // preview refresh has not landed) is not requested and keeps its
        // placeholder.
        let expected_backend = session.backend_index(page_ids[index]);
        THUMBNAILS.with_borrow(|requests| {
            let requested = requests
                .as_ref()
                .unwrap()
                .iter()
                .find(|(_, requested)| *requested == picture)
                .map(|(backend_index, _)| *backend_index as usize);
            assert_eq!(
                requested, expected_backend,
                "thumbnail must be requested by backend page position"
            );
        });
    }
    assert!(grid.child_at_index(expected.len() as i32).is_none());
}

fn drop_on(viewer: &Viewer, from: i32, to: i32) -> bool {
    let grid = &viewer.organize.grid;
    // Picking requires mapped widgets, not just an allocation.
    assert!(grid.is_mapped());
    grid.allocate(800, 600, -1, None);
    let target = grid.child_at_index(to).unwrap();
    let bounds = target.allocation();
    let x = bounds.x() + bounds.width() / 2;
    let y = bounds.y() + bounds.height() / 2;
    assert_eq!(grid.child_at_pos(x, y), Some(target));
    handle_drop(viewer, &from.to_value(), f64::from(x), f64::from(y))
}

#[gtk::test]
fn gtk_ui_delete_enables_bound_history_and_round_trips_the_grid() {
    with_organize(|viewer| {
        let undo = history_button(viewer, "Undo");
        let redo = history_button(viewer, "Redo");
        assert_eq!(undo.action_name().as_deref(), Some("win.undo"));
        assert_eq!(redo.action_name().as_deref(), Some("win.redo"));
        assert!(undo.get_visible() && redo.get_visible());
        assert!(!undo.is_sensitive() && !redo.is_sensitive());
        delete_button(viewer, 1).emit_clicked();
        assert!(viewer.undo_action.is_enabled() && undo.is_sensitive());
        assert_grid(viewer, &[0, 2]);
        undo.emit_clicked();
        assert_grid(viewer, &[0, 1, 2]);
        assert!(!undo.is_sensitive() && redo.is_sensitive());
        redo.emit_clicked();
        assert_grid(viewer, &[0, 2]);
        assert_eq!(
            viewer.view_stack.visible_child_name().as_deref(),
            Some(ORGANIZE_PAGE)
        );
        let state = viewer.state.borrow();
        let session = state.session.as_ref().unwrap();
        assert_eq!(session.edit_revision, 3);
        assert!(session.unsaved_to_disk);
        assert!(!state.preview_refresh_in_flight);
    });
}

/// Deleting a page through the Organize screen must take that page's
/// annotations with it, and undo must bring both back.
///
/// Not cosmetic: an annotation left naming a deleted `PageId` is refused by
/// `pdf_save::attach_annotations`, so the orphan would make every later save
/// — and every preview refresh — fail until the user undid the delete.
#[gtk::test]
fn gtk_ui_delete_takes_the_pages_annotations_with_it_and_undo_restores_them() {
    with_organize(|viewer| {
        {
            let mut session = session(viewer);
            let document = model(&mut session).unwrap();
            document.annotations.insert(a_highlight(1, PageId(1)));
            document.annotations.insert(a_highlight(2, PageId(2)));
        }

        delete_button(viewer, 1).emit_clicked();
        {
            let mut session = session(viewer);
            let document = model(&mut session).unwrap();
            assert_eq!(
                document
                    .annotations
                    .iter()
                    .map(|a| a.id)
                    .collect::<Vec<_>>(),
                vec![AnnotationId(2)],
                "only the annotation on the deleted page should be gone"
            );
        }

        history_button(viewer, "Undo").emit_clicked();
        let mut session = session(viewer);
        let document = model(&mut session).unwrap();
        assert_eq!(
            document
                .annotations
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            vec![AnnotationId(1), AnnotationId(2)],
            "undo must restore the annotation at the position it held"
        );
    });
}

#[gtk::test]
fn gtk_ui_move_round_trips_order_labels_and_thumbnail_requests() {
    with_organize(|viewer| {
        assert!(drop_on(viewer, 0, 2));
        assert!(history_button(viewer, "Undo").is_sensitive());
        assert_grid(viewer, &[1, 2, 0]);
        viewer.undo_action.activate(None);
        assert_grid(viewer, &[0, 1, 2]);
        viewer.redo_action.activate(None);
        assert_grid(viewer, &[1, 2, 0]);
        delete_button(viewer, 0).emit_clicked();
        assert_grid(viewer, &[2, 0]);
    });
}

/// A move is a permutation of pages that already exist, so it must cost
/// nothing to draw. The grid used to be torn down and rebuilt on every drop,
/// which meant one pdfium render per page per move — and then the preview
/// refresh did it all a second time.
#[gtk::test]
fn gtk_ui_a_move_reorders_the_grid_without_rendering_anything_again() {
    with_organize(|viewer| {
        let rendered_on_open = render_count();
        assert_eq!(rendered_on_open, 3, "opening the screen renders every page");

        assert!(drop_on(viewer, 0, 2));

        assert_eq!(
            render_count(),
            rendered_on_open,
            "reordering pages must not re-render a single thumbnail"
        );
        assert_grid(viewer, &[1, 2, 0]);
    });
}

/// The cards must survive the move as the same widgets, in the new order.
/// That is what lets the thumbnails stay: a rebuilt card starts blank.
#[gtk::test]
fn gtk_ui_a_move_carries_the_same_card_widgets_into_their_new_positions() {
    with_organize(|viewer| {
        let before = viewer.organize.cards.snapshot();

        assert!(drop_on(viewer, 0, 2));

        let after = viewer.organize.cards.snapshot();
        let mut expected = before.clone();
        let moved = expected.remove(0);
        expected.insert(2, moved);
        assert_eq!(
            after, expected,
            "a move is a remove-then-insert of the card"
        );
        for card in after.iter() {
            assert!(
                card.picture.paintable().is_some(),
                "a moved card keeps the thumbnail it already had"
            );
        }
    });
}

/// The reopen behind a preview refresh swaps the pdfium handle, but a
/// thumbnail already painted is a `Pixbuf` the card owns — nothing about it
/// goes stale. Only a card that never got one has work left to do.
#[gtk::test]
fn gtk_ui_a_reopen_only_renders_the_cards_that_never_got_a_thumbnail() {
    with_organize(|viewer| {
        let rendered_on_open = render_count();

        refresh_after_reopen(viewer);
        assert_eq!(
            render_count(),
            rendered_on_open,
            "a fully painted grid needs nothing from a reopen"
        );

        // Stands in for the one case that does: a card whose render was still
        // in flight when the handle was swapped, and so was dropped.
        viewer.organize.cards.snapshot()[1]
            .picture
            .set_paintable(gdk::Paintable::NONE);
        refresh_after_reopen(viewer);

        assert_eq!(
            render_count(),
            rendered_on_open + 1,
            "exactly the unpainted card is rendered"
        );
        assert_grid(viewer, &[0, 1, 2]);
    });
}

/// Undoing a content edit repaints a page, and the Undo button sits in this
/// screen's own header — so the grid can be holding a card that is now a
/// picture of the wrong thing. That is the one case where the reopen must
/// rebuild rather than trust what is on screen.
#[gtk::test]
fn gtk_ui_an_invalidated_grid_re_renders_every_card_on_the_next_reopen() {
    with_organize(|viewer| {
        let rendered_on_open = render_count();

        invalidate_thumbnails(viewer);
        refresh_after_reopen(viewer);

        assert_eq!(
            render_count(),
            rendered_on_open + 3,
            "every card is rendered again, not just the blank ones"
        );
        assert!(
            !viewer.organize.thumbnails_stale.get(),
            "the rebuild is what the flag asked for, so it must clear it"
        );
        assert_grid(viewer, &[0, 1, 2]);
    });
}

/// A hidden grid has nothing to keep current — the trip back through `show`
/// rebuilds it from scratch anyway.
#[gtk::test]
fn gtk_ui_a_reopen_leaves_a_hidden_grid_alone() {
    with_organize(|viewer| {
        let rendered_on_open = render_count();
        viewer.view_stack.set_visible_child_name(EDITOR_PAGE);

        invalidate_thumbnails(viewer);
        refresh_after_reopen(viewer);

        assert_eq!(render_count(), rendered_on_open);
    });
}

#[gtk::test]
fn gtk_ui_no_op_move_preserves_clean_state_and_redo() {
    with_organize(|viewer| {
        assert!(!move_page(viewer, 1, 1));
        assert!(!drop_on(viewer, 1, 1));
        {
            let mut session = session(viewer);
            assert_eq!(session.edit_revision, 0);
            assert!(!session.unsaved_to_disk);
            assert!(!model(&mut session).unwrap().pending_edits.can_undo());
        }
        assert!(drop_on(viewer, 0, 2));
        viewer.undo_action.activate(None);
        assert!(!move_page(viewer, 1, 1));
        assert!(viewer.redo_action.is_enabled());
        assert_eq!(session(viewer).edit_revision, 2);
        viewer.redo_action.activate(None);
        assert_grid(viewer, &[1, 2, 0]);
    });
}

#[gtk::test]
fn gtk_ui_non_structural_history_does_not_rebuild_and_hidden_grid_waits_for_show() {
    with_organize(|viewer| {
        assert!(drop_on(viewer, 0, 2));
        let cards = viewer.organize.cards.snapshot();
        {
            let mut session = session(viewer);
            let document = model(&mut session).unwrap();
            apply_command(
                document,
                Command::SetDocumentInfo {
                    before: Default::default(),
                    after: pdf_document::DocumentInfo {
                        title: Some("Organized document".into()),
                        ..Default::default()
                    },
                },
            );
        }
        viewer.undo_action.activate(None);
        viewer.redo_action.activate(None);
        assert_eq!(viewer.organize.cards.snapshot(), cards);
        viewer.undo_action.activate(None);
        viewer.view_stack.set_visible_child_name(EDITOR_PAGE);
        viewer.undo_action.activate(None);
        viewer.redo_action.activate(None);
        viewer.undo_action.activate(None);
        assert_eq!(viewer.organize.cards.snapshot(), cards);
        assert_eq!(
            viewer.view_stack.visible_child_name().as_deref(),
            Some(EDITOR_PAGE)
        );
        show(viewer);
        assert_grid(viewer, &[0, 1, 2]);
    });
}

#[gtk::test]
fn gtk_ui_insert_page_history_rebuilds_in_both_directions() {
    with_organize(|viewer| {
        assert!(command(viewer, |session| {
            let document = model(session)?;
            let page = Page::blank(PageId(3), PageSize::A4, PageOrientation::Portrait);
            apply_command(document, Command::insert_page(1, page));
            Ok("Inserted page.".into())
        }));
        populate_grid(viewer);
        assert_grid(viewer, &[0, 3, 1, 2]);
        viewer.undo_action.activate(None);
        assert_grid(viewer, &[0, 1, 2]);
        viewer.redo_action.activate(None);
        assert_grid(viewer, &[0, 3, 1, 2]);
    });
}

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

mod refusals;
mod resolution;
