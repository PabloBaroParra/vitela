//! Every reason the Organize screen turns a page operation down, in one
//! place.
//!
//! Split out of the grid and history tests beside them: those walk the happy
//! path through the widgets, this walks the funnel's refusals in order —
//! content-edit permission, assembly permission, encryption rewritability, an
//! out-of-range index, and a missing model — checking after each that the
//! cards, the history and the dirty flag are exactly as they were.

use super::*;
// Named explicitly rather than left to the glob above: `super` reaches these
// only by re-exporting its own glob of `organize`, and a reader of this file
// should not have to reconstruct that chain to find where they live.
use crate::app::organize::{delete_page, model, move_page};
use crate::app::state::{ContentEditAccess, PageAssemblyAccess};

/// An encrypted document opened the only way this shell can open one today:
/// with a single password. A PDF's second password cannot be derived from the
/// first, so no full rewrite of it can reproduce its encryption.
fn one_password_security() -> pdf_document::SecurityContext {
    pdf_document::SecurityContext {
        handler: pdf_document::SecurityHandler::Aes128,
        credential: pdf_document::Credential::User,
        credentials: pdf_document::EncryptionCredentials::user("only-the-user-password"),
        permissions: pdf_document::Permissions(0xFFFF_FFFC),
    }
}

#[gtk::test]
fn gtk_ui_refused_and_failed_commands_leave_cards_and_history_untouched() {
    with_organize(|viewer| {
        let cards = viewer.organize.cards.borrow().clone();
        session(viewer).content_edit_access = ContentEditAccess::Forbidden;
        delete_button(viewer, 1).emit_clicked();
        assert!(!drop_on(viewer, 0, 2));
        assert_grid(viewer, &[0, 1, 2]);
        session(viewer).content_edit_access = ContentEditAccess::Allowed;
        // The assembly permission is a separate bit: a document may grant
        // content edits and still forbid changing which pages it has.
        session(viewer).page_assembly_access = PageAssemblyAccess::Forbidden;
        delete_button(viewer, 1).emit_clicked();
        assert!(!drop_on(viewer, 0, 2));
        assert_grid(viewer, &[0, 1, 2]);
        session(viewer).page_assembly_access = PageAssemblyAccess::Allowed;
        // Third refusal, and a different question again: this document grants
        // assembly, but it was opened with only one of its two passwords, so
        // the full rewrite a page change forces could never reproduce its
        // encryption (checklist section 5 item 5).
        {
            let mut session = session(viewer);
            model(&mut session).unwrap().security = Some(one_password_security());
        }
        delete_button(viewer, 1).emit_clicked();
        assert!(!drop_on(viewer, 0, 2));
        assert_grid(viewer, &[0, 1, 2]);
        {
            let mut session = session(viewer);
            model(&mut session).unwrap().security = None;
        }
        assert!(!delete_page(viewer, 9));
        assert!(!move_page(viewer, 0, 9));
        // A stale card must also survive a missing model, not just permissions.
        session(viewer).document_model = None;
        delete_button(viewer, 1).emit_clicked();
        assert!(!drop_on(viewer, 0, 2));
        assert_eq!(*viewer.organize.cards.borrow(), cards);
        let state = viewer.state.borrow();
        let session = state.session.as_ref().unwrap();
        assert_eq!(session.edit_revision, 0);
        assert!(!session.unsaved_to_disk);
        assert!(!viewer.undo_action.is_enabled());
    });
}
