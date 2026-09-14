//! The Documents view's two block turns: one click, every page of one
//! document block, one undo step.
//!
//! The per-page twin is [`super::page`]; [`super`] holds what both are judged
//! by and why they sit together.

use super::super::documents::{
    card_buttons, cards, source, two_blocks, DELETE, ROTATE_LEFT, ROTATE_RIGHT,
};
use super::super::*;
use super::rotations;
use crate::app::organize::cache::ThumbnailKey;
use crate::app::organize::command::{model, rotate_block};
use crate::app::organize::documents::card::{COVER_HEIGHT_PX, COVER_WIDTH_PX};
use crate::app::state::PageAssemblyAccess;
use pdf_document::Rotation;

/// The Documents view's pair: one click turns every page of the block it
/// belongs to, and no page of any other block.
#[gtk::test]
fn gtk_ui_the_block_buttons_turn_every_page_of_their_document_each_way() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        card_buttons(&cards(viewer)[1])[ROTATE_RIGHT].emit_clicked();

        assert_eq!(
            rotations(viewer),
            [
                Rotation::None,
                Rotation::None,
                Rotation::Clockwise90,
                Rotation::Clockwise90,
                Rotation::Clockwise90
            ],
            "the whole imported block turns, and the base block does not"
        );
        assert_eq!(viewer.status.text().as_str(), "Rotated 3 pages right.");

        card_buttons(&cards(viewer)[1])[ROTATE_LEFT].emit_clicked();

        assert_eq!(
            rotations(viewer),
            [Rotation::None; 5],
            "the other button is the inverse gesture, not a second forward one"
        );
        assert_eq!(viewer.status.text().as_str(), "Rotated 3 pages left.");
    });
}

/// **One click, one press of Undo** — the whole reason `Command::RotatePages`
/// exists beside `RotatePage`. Recording one rotation per page would make a
/// twelve-page document twelve steps to take back.
#[gtk::test]
fn gtk_ui_turning_a_block_is_a_single_undo_step() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        card_buttons(&cards(viewer)[1])[ROTATE_RIGHT].emit_clicked();

        viewer.undo_action.activate(None);

        assert_eq!(
            rotations(viewer),
            [Rotation::None; 5],
            "one undo puts the whole block back"
        );
        assert!(!viewer.undo_action.is_enabled());

        viewer.redo_action.activate(None);

        assert_eq!(rotations(viewer)[2..], [Rotation::Clockwise90; 3]);
        // And no page moved through any of it: a turn is not a reorder.
        assert_eq!(page_ids_of(viewer), vec![0, 1, 2, 3, 4]);
    });
}

/// A block turn forgets its own pages' thumbnails at *both* sizes and leaves
/// every other page's alone — the same narrow answer the per-page turn gives,
/// widened to exactly the pages the click actually touched.
///
/// The blunt `invalidate_thumbnails` would have been correct and ruinous: on
/// a four-hundred-page assembly, turning a three-page block would put a
/// re-render of all four hundred cards behind the click.
#[gtk::test]
fn gtk_ui_turning_a_block_forgets_only_its_own_pages_thumbnails() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        // Visit the Pages view so every page is cached at that size too: the
        // block's pages have cards there as well, and each of them is just as
        // stale after the turn as the cover is.
        viewer.organize.pages_toggle.set_active(true);
        viewer.organize.documents_toggle.set_active(true);
        assert_eq!(
            viewer.organize.thumbnails.len(),
            7,
            "five page cards and two block covers"
        );

        card_buttons(&cards(viewer)[1])[ROTATE_RIGHT].emit_clicked();

        assert_eq!(
            viewer.organize.thumbnails.len(),
            3,
            "the three turned pages lose their page cards, and the block its cover"
        );
        assert!(
            cached_cover(viewer, PageId(0)),
            "the block that did not turn keeps its cover"
        );
        assert!(!cached_cover(viewer, PageId(2)));
    });
}

/// The narrow funnel again, asserted where its absence would be silent: this
/// document grants assembly but can never be rewritten, so the delete beside
/// the turns is refused and the turn is not.
#[gtk::test]
fn gtk_ui_a_block_turn_survives_a_document_that_can_never_be_rewritten() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        {
            let mut session = session(viewer);
            model(&mut session).unwrap().security = Some(one_password_security());
        }

        card_buttons(&cards(viewer)[1])[DELETE].emit_clicked();
        assert_eq!(page_ids_of(viewer), vec![0, 1, 2, 3, 4], "delete refused");

        card_buttons(&cards(viewer)[1])[ROTATE_RIGHT].emit_clicked();

        assert_eq!(rotations(viewer)[2..], [Rotation::Clockwise90; 3]);
        assert_eq!(viewer.status.text().as_str(), "Rotated 3 pages right.");
    });
}

/// What a block turn refuses, and what it must not leave behind when it does.
///
/// The run is resolved to `PageId`s *before* the command is recorded, so a
/// range the model does not hold is answered here rather than reaching
/// `Command::RotatePages` — which refuses the same case itself, and whose
/// refusal this shell must never have to rely on.
#[gtk::test]
fn gtk_ui_a_refused_block_turn_records_nothing_at_all() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        session(viewer).page_assembly_access = PageAssemblyAccess::Forbidden;

        card_buttons(&cards(viewer)[1])[ROTATE_RIGHT].emit_clicked();

        assert_eq!(rotations(viewer), [Rotation::None; 5]);
        assert_eq!(
            viewer.status.text().as_str(),
            "This document does not permit adding, removing or reordering its pages."
        );

        session(viewer).page_assembly_access = PageAssemblyAccess::Allowed;
        assert!(!rotate_block(viewer, 3, 9, 90), "past the end of the model");
        assert_eq!(
            viewer.status.text().as_str(),
            "Those pages no longer exist."
        );
        assert!(
            !rotate_block(viewer, 0, 0, 90),
            "an empty run turns nothing"
        );

        session(viewer).document_model = None;
        assert!(!rotate_block(viewer, 0, 2, 90));

        let state = viewer.state.borrow();
        let session = state.session.as_ref().unwrap();
        assert_eq!(session.edit_revision, 0);
        assert!(!session.unsaved_to_disk);
        assert!(!viewer.undo_action.is_enabled());
    });
}

/// Whether the cache holds `page`'s thumbnail at the size a block cover asks
/// for. The page-card twin is [`super::super::cached_thumbnail`]; the two keys
/// differ by size alone, which is exactly why a cover and a card cannot share
/// one entry.
fn cached_cover(viewer: &Viewer, page: PageId) -> bool {
    viewer
        .organize
        .thumbnails
        .get(&ThumbnailKey {
            page,
            width: COVER_WIDTH_PX,
            height: COVER_HEIGHT_PX,
            scale: 1,
        })
        .is_some()
}
