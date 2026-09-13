//! What the Pages grid does when the page list changes under it: a delete, a
//! move, and every undo or redo that reaches the screen while it is on show.
//!
//! Split out of [`super`], which keeps the fixtures these share. The
//! recurring subject is what a change must *not* cost — a move re-renders no
//! thumbnail, a reopen renders only the cards that never got one, and a
//! hidden grid waits until it is shown.

use super::*;

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
        assert_eq!(
            rendered_on_open, 4,
            "opening renders the Documents view's one block cover, then every page"
        );

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
/// goes stale. Only a card that never got one has work left to do, and even
/// that one asks pdfium for nothing while the cache still holds its page.
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

        // Stands in for the one case that has work to do: a card whose render
        // was still in flight when the handle was swapped, and so was
        // dropped.
        let blank = || {
            viewer.organize.cards.snapshot()[1]
                .picture
                .set_paintable(gdk::Paintable::NONE)
        };
        blank();
        refresh_after_reopen(viewer);
        assert_eq!(
            render_count(),
            rendered_on_open,
            "a page the cache still holds is repainted from it, not re-rendered"
        );
        assert!(
            viewer.organize.cards.snapshot()[1]
                .picture
                .paintable()
                .is_some(),
            "and it really is repainted"
        );

        // With the cache emptied — a fresh document, or more pages than it
        // can hold — the same card is the one and only render.
        viewer.organize.thumbnails.clear();
        blank();
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
        viewer.organize.pages_toggle.set_active(true);
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
