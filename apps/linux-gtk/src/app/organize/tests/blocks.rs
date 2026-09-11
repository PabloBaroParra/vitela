//! Moving and deleting a whole document block (checklist §9): the drag, the
//! keyboard buttons that stand in for it, and the single undo step each of
//! them records.
//!
//! The view itself — the selector, the cards, the drop positions — is
//! [`super::documents`]'s.

use gtk::prelude::*;

use super::super::documents::gap::drop_block;
use super::documents::{
    base_page, card_buttons, card_text, cards, children, imported_page, source, two_blocks,
};
use super::{page_ids_of, with_documents, with_organize};
use pdf_document::{Document, PageId};

#[gtk::test]
fn gtk_ui_dragging_a_block_moves_every_page_of_it_in_one_undo_step() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        // The imported block to the very front: slot 0.
        assert!(drop_block(viewer, PageId(2), 0));

        assert_eq!(page_ids_of(viewer), vec![2, 3, 4, 0, 1]);
        assert_eq!(
            card_text(&cards(viewer)[0]),
            ("report.pdf".to_owned(), "3 pages · 1–3".to_owned()),
            "the moved block's range must follow it"
        );

        viewer.undo_action.activate(None);

        assert_eq!(
            page_ids_of(viewer),
            vec![0, 1, 2, 3, 4],
            "one undo puts the whole block back"
        );
    });
}

#[gtk::test]
fn gtk_ui_dropping_a_block_where_it_already_is_changes_nothing() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        let revision = super::session(viewer).edit_revision;

        assert!(!drop_block(viewer, PageId(0), 0), "its own slot");
        assert!(!drop_block(viewer, PageId(0), 1), "the slot just after it");

        assert_eq!(page_ids_of(viewer), vec![0, 1, 2, 3, 4]);
        assert_eq!(super::session(viewer).edit_revision, revision);
        assert!(!viewer.undo_action.is_enabled());
    });
}

/// The keyboard path: the same two moves the drag offers, as buttons, with
/// the ones that would run off either end of the list made insensitive.
#[gtk::test]
fn gtk_ui_the_move_buttons_reorder_blocks_without_a_drag() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        let built = cards(viewer);
        let first = card_buttons(&built[0]);
        let last = card_buttons(&built[1]);
        assert!(!first[0].is_sensitive(), "the first block cannot move up");
        assert!(first[1].is_sensitive());
        assert!(last[0].is_sensitive());
        assert!(!last[1].is_sensitive(), "the last block cannot move down");

        last[0].emit_clicked();

        assert_eq!(page_ids_of(viewer), vec![2, 3, 4, 0, 1]);

        card_buttons(&cards(viewer)[0])[1].emit_clicked();

        assert_eq!(page_ids_of(viewer), vec![0, 1, 2, 3, 4]);
    });
}

#[gtk::test]
fn gtk_ui_every_block_control_carries_an_accessible_name() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        for (card, expected) in cards(viewer).iter().zip([
            [
                "Move up: base.pdf",
                "Move down: base.pdf",
                "Delete base.pdf",
            ],
            [
                "Move up: report.pdf",
                "Move down: report.pdf",
                "Delete report.pdf",
            ],
        ]) {
            for (button, name) in card_buttons(card).iter().zip(expected) {
                assert_eq!(button.tooltip_text().as_deref(), Some(name));
            }
        }
    });
}

#[gtk::test]
fn gtk_ui_deleting_a_block_removes_all_of_its_pages_in_one_undo_step() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        card_buttons(&cards(viewer)[1])[2].emit_clicked();

        assert_eq!(page_ids_of(viewer), vec![0, 1]);
        assert_eq!(cards(viewer).len(), 1);
        assert!(super::session(viewer).unsaved_to_disk);

        viewer.undo_action.activate(None);

        assert_eq!(page_ids_of(viewer), vec![0, 1, 2, 3, 4]);
        assert_eq!(cards(viewer).len(), 2);
    });
}

/// Deleting a block must take what is anchored to *every* page of it, the
/// same way the per-page delete does — an annotation left behind names a
/// page id `pdf-save` would refuse to write.
#[gtk::test]
fn gtk_ui_deleting_a_block_takes_its_annotations_and_undo_restores_them() {
    let mut document = two_blocks();
    document
        .annotations
        .insert(crate::app::test_fixtures::a_highlight(1, PageId(2)));
    document
        .annotations
        .insert(crate::app::test_fixtures::a_highlight(2, PageId(4)));
    document
        .annotations
        .insert(crate::app::test_fixtures::a_highlight(3, PageId(0)));

    with_documents(document, vec![source(7, "report.pdf")], |viewer| {
        card_buttons(&cards(viewer)[1])[2].emit_clicked();

        assert_eq!(
            super::session(viewer)
                .document_model
                .as_ref()
                .unwrap()
                .annotations
                .iter()
                .map(|annotation| annotation.id.0)
                .collect::<Vec<_>>(),
            vec![3],
            "only the annotation on a surviving page stays"
        );

        viewer.undo_action.activate(None);

        assert_eq!(
            super::session(viewer)
                .document_model
                .as_ref()
                .unwrap()
                .annotations
                .len(),
            3,
            "one undo brings the block back with what was drawn on it"
        );
    });
}

/// The blocks are derived from the page order, so a move made in the Pages
/// view is already reflected the next time the Documents view is shown —
/// there is no second order to keep in step.
#[gtk::test]
fn gtk_ui_returning_to_documents_shows_the_order_the_pages_view_left() {
    with_organize(|viewer| {
        super::session(viewer)
            .document_model
            .as_mut()
            .unwrap()
            .pages = vec![base_page(0), imported_page(1, 7), base_page(2)];

        viewer.organize.documents_toggle.set_active(true);

        let cards = cards(viewer);
        assert_eq!(cards.len(), 3, "a source that reappears starts a new block");
        assert_eq!(card_text(&cards[1]).1, "1 page · 2");
    });
}

/// A model the screen can show nothing for leaves an empty list rather than
/// a half-built one.
#[gtk::test]
fn gtk_ui_a_session_without_a_model_leaves_the_list_empty() {
    with_documents(Document::blank(), Vec::new(), |viewer| {
        super::session(viewer).document_model = None;
        super::super::documents::populate(viewer);

        assert!(children(viewer).is_empty());
    });
}

/// An imported page whose source is no longer registered still gets a card:
/// the block is in the page order either way, and a nameless one is far
/// better than a gap in the list.
#[gtk::test]
fn gtk_ui_an_unregistered_source_still_gets_a_named_card() {
    let mut document = Document::blank();
    document.pages = vec![base_page(0), imported_page(1, 9)];

    with_documents(document, Vec::new(), |viewer| {
        assert_eq!(card_text(&cards(viewer)[1]).0, "Imported PDF");
    });
}

/// The last drop position — "after every block" — is a real destination,
/// not an off-by-one that silently refuses.
#[gtk::test]
fn gtk_ui_a_block_can_be_dropped_after_the_last_one() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        assert!(drop_block(viewer, PageId(0), 2));

        assert_eq!(page_ids_of(viewer), vec![2, 3, 4, 0, 1]);
        assert_eq!(
            card_text(&cards(viewer)[1]),
            ("base.pdf".to_owned(), "2 pages · 4–5".to_owned())
        );
    });
}
