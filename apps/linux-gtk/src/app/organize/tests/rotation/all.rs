//! The whole-document turn the command palette runs: every page, one undo
//! step, every card's thumbnail forgotten.
//!
//! [`rotate_all_pages`] is the Documents view's block turn over every page,
//! so what is asserted here is what [`super::block`] asserts of a block,
//! widened to the whole document and seen from the Pages grid.

use super::super::*;
use super::rotations;
use crate::app::organize::command::rotate_all_pages;
use crate::app::state::PageAssemblyAccess;
use pdf_document::Rotation;

/// Every page turns, one press of Undo takes them all back, and nothing
/// moves.
#[gtk::test]
fn gtk_ui_turning_every_page_is_a_single_undo_step() {
    with_organize(|viewer| {
        assert!(rotate_all_pages(viewer, 90));
        assert_eq!(rotations(viewer), [Rotation::Clockwise90; 3]);
        assert_eq!(viewer.status.text().as_str(), "Rotated 3 pages right.");

        viewer.undo_action.activate(None);

        assert_eq!(rotations(viewer), [Rotation::None; 3]);
        assert!(!viewer.undo_action.is_enabled());
        assert_eq!(page_ids_of(viewer), vec![0, 1, 2]);
    });
}

/// Every card goes back to a placeholder and loses its cached pixels before
/// the reopen, and the reopen renders each of them again — the per-page
/// invalidation the block buttons use, over every page.
#[gtk::test]
fn gtk_ui_turning_every_page_re_renders_every_card() {
    with_organize(|viewer| {
        let rendered_on_open = render_count();
        assert_eq!(
            viewer.organize.thumbnails.len(),
            4,
            "three page cards and the block cover `show` opened on"
        );

        assert!(rotate_all_pages(viewer, -90));

        assert_eq!(rotations(viewer), [Rotation::Clockwise270; 3]);
        assert_eq!(
            viewer.organize.thumbnails.len(),
            0,
            "every page's cached pixels are forgotten, at both sizes"
        );
        for card in viewer.organize.cards.snapshot() {
            assert!(card.picture.paintable().is_none(), "card {:?}", card.id);
        }

        refresh_after_reopen(viewer);

        assert_eq!(render_count(), rendered_on_open + 3);
        assert_grid(viewer, &[0, 1, 2]);
    });
}

/// Refused by the same gate as the buttons, and nothing recorded.
#[gtk::test]
fn gtk_ui_a_refused_whole_document_turn_records_nothing() {
    with_organize(|viewer| {
        session(viewer).page_assembly_access = PageAssemblyAccess::Forbidden;

        assert!(!rotate_all_pages(viewer, 90));

        assert_eq!(rotations(viewer), [Rotation::None; 3]);
        assert_eq!(
            viewer.status.text().as_str(),
            "This document does not permit adding, removing or reordering its pages."
        );
        assert_eq!(session(viewer).edit_revision, 0);
        assert!(!viewer.undo_action.is_enabled());
    });
}
