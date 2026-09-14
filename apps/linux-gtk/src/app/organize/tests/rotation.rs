//! The Pages view's two quarter-turns: what a click records, what it costs
//! the grid, and the one permission it asks that its neighbours in the footer
//! do not.
//!
//! Split out of [`super::history`] and [`super::refusals`] rather than added
//! to either, because a rotation is the odd one out on both counts. Every
//! other operation on this screen changes the page *list* — so it forces
//! `pdf-save`'s full-rewrite writer, and so it invalidates nothing about how
//! a surviving page looks. A turn does neither: it stays on the incremental
//! writer, and it makes exactly one card a picture of an angle its page no
//! longer has.

use super::*;
use crate::app::organize::command::{model, rotate_page};
use crate::app::state::PageAssemblyAccess;
use pdf_document::Rotation;

/// The recorded angle of each model page, in `Document.pages` order.
fn rotations(viewer: &Viewer) -> Vec<Rotation> {
    session(viewer)
        .document_model
        .as_ref()
        .unwrap()
        .pages
        .iter()
        .map(|page| page.rotation)
        .collect()
}

#[gtk::test]
fn gtk_ui_the_two_footer_buttons_turn_their_page_each_way() {
    with_organize(|viewer| {
        rotate_right_button(viewer, 1).emit_clicked();
        assert_eq!(
            rotations(viewer),
            [Rotation::None, Rotation::Clockwise90, Rotation::None],
            "the clicked card's page turns, and only it"
        );
        assert_eq!(viewer.status.text().as_str(), "Rotated page 2 right.");

        rotate_left_button(viewer, 1).emit_clicked();
        assert_eq!(
            rotations(viewer),
            [Rotation::None; 3],
            "the other button is the inverse gesture, not a second forward one"
        );
        assert_eq!(viewer.status.text().as_str(), "Rotated page 2 left.");

        // A left turn from zero is three quarters, not minus one: the angle
        // the model keeps is absolute even though the command carries a delta.
        rotate_left_button(viewer, 0).emit_clicked();
        assert_eq!(
            rotations(viewer),
            [Rotation::Clockwise270, Rotation::None, Rotation::None]
        );

        let state = viewer.state.borrow();
        let session = state.session.as_ref().unwrap();
        assert_eq!(session.edit_revision, 3);
        assert!(session.unsaved_to_disk);
    });
}

/// Four clockwise clicks are four undo steps back through 270, 180 and 90 —
/// not one step that collapses at zero. The command records the gesture the
/// user made, so the history has to give each one back.
#[gtk::test]
fn gtk_ui_every_quarter_turn_is_its_own_undo_step() {
    with_organize(|viewer| {
        let undo = history_button(viewer, "Undo");
        let redo = history_button(viewer, "Redo");
        assert!(!undo.is_sensitive());

        for _ in 0..4 {
            rotate_right_button(viewer, 0).emit_clicked();
        }
        assert_eq!(rotations(viewer)[0], Rotation::None, "four turns is a lap");
        assert!(undo.is_sensitive());

        for expected in [
            Rotation::Clockwise270,
            Rotation::Clockwise180,
            Rotation::Clockwise90,
            Rotation::None,
        ] {
            undo.emit_clicked();
            assert_eq!(rotations(viewer)[0], expected);
        }
        assert!(!undo.is_sensitive() && redo.is_sensitive());

        redo.emit_clicked();
        assert_eq!(rotations(viewer)[0], Rotation::Clockwise90);
        // And the grid is untouched by any of it: a turn moves no page.
        assert_grid(viewer, &[0, 1, 2]);
    });
}

/// **A turn costs exactly one pdfium render, not a grid full of them.**
///
/// This is the whole reason `cache::Thumbnails::forget_page` exists beside
/// `invalidate`. Emptying the cache — the honest answer for a content edit,
/// which can repaint anything — would put a rebuild of every card behind
/// every single click, which on a large assembly is the ~0.9 s of blocked
/// main loop checklist §11 exists to have removed.
#[gtk::test]
fn gtk_ui_a_turn_re_renders_only_the_page_that_turned() {
    with_organize(|viewer| {
        let rendered_on_open = render_count();
        let turned = viewer.organize.cards.snapshot()[1].clone();
        let neighbour = viewer.organize.cards.snapshot()[2].clone();

        rotate_right_button(viewer, 1).emit_clicked();

        // Dropped straight away, before the reopen: the card has to stop
        // painting the `Pixbuf` it owns, or nothing downstream can tell it
        // apart from a card that is already correct.
        assert!(
            turned.picture.paintable().is_none(),
            "the turned page's card must go back to a placeholder"
        );
        assert!(
            !cached_thumbnail(viewer, &turned.picture, turned.id),
            "and its cached pixels must go with it"
        );
        assert!(
            neighbour.picture.paintable().is_some()
                && cached_thumbnail(viewer, &neighbour.picture, neighbour.id),
            "the page beside it did not turn, so its picture is still current"
        );

        refresh_after_reopen(viewer);

        assert_eq!(
            render_count(),
            rendered_on_open + 1,
            "exactly the turned card is rendered again"
        );
        assert!(turned.picture.paintable().is_some());
        assert_grid(viewer, &[0, 1, 2]);
    });
}

/// The narrower funnel, stated as the one case where it is visible: this
/// document grants assembly but was opened with a single password, so no full
/// rewrite of it could reproduce its encryption. Moving or deleting a page is
/// refused for exactly that reason ([`super::refusals`]) — a turn is not,
/// because it stays on the incremental writer, which re-encrypts from lopdf's
/// own retained state (`docs/batch-pdf-assembly.md` section 5).
#[gtk::test]
fn gtk_ui_a_turn_survives_a_document_that_can_never_be_rewritten() {
    with_organize(|viewer| {
        {
            let mut session = session(viewer);
            model(&mut session).unwrap().security = Some(one_password_security());
        }

        // Its neighbours in the same footer, and the drag, are all refused.
        delete_button(viewer, 1).emit_clicked();
        assert!(!drop_on(viewer, 0, 2));
        assert_grid(viewer, &[0, 1, 2]);
        assert_eq!(rotations(viewer), [Rotation::None; 3]);

        rotate_right_button(viewer, 1).emit_clicked();

        assert_eq!(rotations(viewer)[1], Rotation::Clockwise90);
        assert_eq!(viewer.status.text().as_str(), "Rotated page 2 right.");
    });
}

/// The permission a turn does ask, and the two failures that are not
/// permissions at all.
///
/// Assembly is the bit PDF 1.7 table 22 names for "insert, **rotate**, or
/// delete pages", so a document that withholds it refuses a turn as flatly as
/// it refuses a move.
#[gtk::test]
fn gtk_ui_a_refused_turn_records_nothing_at_all() {
    with_organize(|viewer| {
        session(viewer).page_assembly_access = PageAssemblyAccess::Forbidden;

        rotate_right_button(viewer, 1).emit_clicked();

        assert_eq!(rotations(viewer), [Rotation::None; 3]);
        assert_eq!(
            viewer.status.text().as_str(),
            "This document does not permit adding, removing or reordering its pages."
        );

        session(viewer).page_assembly_access = PageAssemblyAccess::Allowed;
        // A page the model no longer holds is a refusal too, and not a
        // recorded no-op: `EditLog::apply` accepts a `RotatePage` naming any
        // id at all and quietly turns nothing, so the funnel has to be the
        // one that says no.
        assert!(!rotate_page(viewer, PageId(99), 90));
        assert_eq!(viewer.status.text().as_str(), "Page no longer exists.");

        session(viewer).document_model = None;
        assert!(!rotate_page(viewer, PageId(0), 90));

        let state = viewer.state.borrow();
        let session = state.session.as_ref().unwrap();
        assert_eq!(session.edit_revision, 0);
        assert!(!session.unsaved_to_disk);
        assert!(!viewer.undo_action.is_enabled());
    });
}
