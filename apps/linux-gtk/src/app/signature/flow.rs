//! The drawn-signature flow: which dialog opens, what each answer does, and when
//! a finished picture may be armed.
//!
//! The dialogs ([`super::pad`], [`super::offer`]) only collect answers; every
//! decision is here. Two guards run through all of it, both for the same
//! reason — the picture is made on a worker thread, and the user keeps the
//! window while it is:
//!
//! - the dialog must still be the tracked one (`ViewerState::signature.dialog`)
//!   — a pad the user cancelled, or one a second dialog replaced, arms nothing;
//! - the document must still be the one the pad was opened over — a picture
//!   that arrives after another document opened arms nothing.

use gtk::prelude::*;
use gtk::{gio, glib, Window};

use crate::app::state::Viewer;

use super::ink::{self, Stroke};
use super::png::render_png;
use super::{armed, current_session_id, offer, pad};

const NO_DOCUMENT: &str = "Open a PDF before drawing a signature.";
const UNRENDERABLE: &str = "The signature could not be turned into an image.";
const NOT_REMEMBERED: &str = "Your signature could not be remembered. Click a page to place it.";
const FORGOTTEN: &str = "Your saved signature was deleted from this computer.";

/// **Draw signature**: the remembered signature, or a blank pad when there is
/// none — and only where a stamp could be placed.
pub(super) fn open(viewer: &Viewer) {
    let Some(session_id) = current_session_id(viewer) else {
        viewer.status.set_text(NO_DOCUMENT);
        return;
    };
    if let Some(refusal) = viewer.annotation_editing_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    if has_dialog(viewer) {
        return;
    }
    let store = viewer.state.borrow().signature.store.clone();
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            // Read off the main thread: it is a file.
            let remembered = gio::spawn_blocking(move || store.load())
                .await
                .ok()
                .flatten();
            // A click that lands after another document opened, or while a
            // dialog is already open, shows nothing new.
            if current_session_id(&viewer) != Some(session_id) || has_dialog(&viewer) {
                return;
            }
            show_choice(&viewer, session_id, remembered);
        }
    });
}

/// The remembered signature's offer when there is one, the pad otherwise.
pub(super) fn show_choice(viewer: &Viewer, session_id: u64, remembered: Option<Vec<u8>>) {
    match remembered {
        Some(png) => {
            offer::open(viewer, session_id, png);
        }
        None => {
            pad::open(viewer, session_id);
        }
    }
}

/// The pad's **Use**: makes the picture off the main thread, arms it, and keeps
/// it on this computer when `remember` is set.
///
/// The pad stays open (insensitive) while the picture is made, so **Cancel**
/// — the window's close button — still means something until it arrives.
pub(super) fn use_drawn(
    viewer: &Viewer,
    dialog: &Window,
    session_id: u64,
    strokes: Vec<Stroke>,
    remember: bool,
) {
    dialog.set_sensitive(false);
    glib::spawn_future_local({
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        async move {
            let png = gio::spawn_blocking(move || render_png(&strokes, ink::STROKE_WIDTH))
                .await
                .ok()
                .flatten();
            // Cancelled, or replaced by another dialog, while it rendered.
            if !is_tracked(&viewer, &dialog) {
                return;
            }
            dismiss(&viewer, &dialog);
            let Some(png) = png else {
                viewer.status.set_text(UNRENDERABLE);
                return;
            };
            // `arm` refuses a picture drawn over a document that has since
            // been replaced, and says so; then there is nothing to remember
            // either — the user never got to place it.
            if !armed::arm(&viewer, session_id, png.clone()) {
                return;
            }
            if remember {
                keep(&viewer, png);
            }
        }
    });
}

/// Writes the signature to the store off the main thread. The signature is
/// already armed by now, so a failure only costs the next session its shortcut,
/// and the user is told so without losing the placement they just asked for.
fn keep(viewer: &Viewer, png: Vec<u8>) {
    let store = viewer.state.borrow().signature.store.clone();
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let saved = gio::spawn_blocking(move || store.save(&png))
                .await
                .unwrap_or(false);
            if !saved {
                viewer.status.set_text(NOT_REMEMBERED);
            }
        }
    });
}

/// The offer's **Use**: arms the remembered signature, no pad.
pub(super) fn use_saved(viewer: &Viewer, dialog: &Window, session_id: u64, png: Vec<u8>) {
    if !is_tracked(viewer, dialog) {
        return;
    }
    dismiss(viewer, dialog);
    armed::arm(viewer, session_id, png);
}

/// The offer's **Draw new**: the pad. Its drawing replaces the remembered
/// signature only if it is remembered too.
pub(super) fn draw_new(viewer: &Viewer, dialog: &Window, session_id: u64) {
    if !is_tracked(viewer, dialog) {
        return;
    }
    dismiss(viewer, dialog);
    pad::open(viewer, session_id);
}

/// The offer's **Delete**: forgets the signature on this computer. No
/// confirmation — it is a picture the user can draw again in seconds — and
/// nothing is armed.
pub(super) fn delete_saved(viewer: &Viewer, dialog: &Window) {
    if !is_tracked(viewer, dialog) {
        return;
    }
    dismiss(viewer, dialog);
    viewer.status.set_text(FORGOTTEN);
    let store = viewer.state.borrow().signature.store.clone();
    glib::spawn_future_local(async move {
        let _ = gio::spawn_blocking(move || store.delete()).await;
    });
}

fn has_dialog(viewer: &Viewer) -> bool {
    viewer.state.borrow().signature.dialog.is_some()
}

/// Records `dialog` as the one open, tearing down a stale one it replaces — and
/// forgets it again if the window's own close button is used.
pub(super) fn track(viewer: &Viewer, dialog: &Window) {
    let stale = viewer
        .state
        .borrow_mut()
        .signature
        .dialog
        .replace(dialog.clone());
    if let Some(stale) = stale {
        stale.destroy();
    }
    dialog.connect_close_request({
        let viewer = viewer.clone();
        move |closing| {
            release(&viewer, closing);
            glib::Propagation::Proceed
        }
    });
}

/// Whether `dialog` is still the tracked one: `false` once it was cancelled or
/// a later dialog replaced it.
pub(super) fn is_tracked(viewer: &Viewer, dialog: &Window) -> bool {
    viewer.state.borrow().signature.dialog.as_ref() == Some(dialog)
}

/// Stops tracking `dialog` — but only if it still is the tracked one, so a
/// stale dialog cannot clobber a newer one's slot.
fn release(viewer: &Viewer, dialog: &Window) {
    let mut state = viewer.state.borrow_mut();
    if state.signature.dialog.as_ref() == Some(dialog) {
        state.signature.dialog = None;
    }
}

/// Closes `dialog` and stops tracking it.
pub(super) fn dismiss(viewer: &Viewer, dialog: &Window) {
    release(viewer, dialog);
    dialog.destroy();
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use std::time::Duration;

    use crate::app::state::AnnotationAccess;

    use super::super::store::tests::MemorySignatureStore;
    use super::super::store::SignatureStore;
    use super::super::test_support::{
        a_png, is_armed, open_document, settle_for, settle_until, the_session, Built,
    };

    fn title_of_the_open_dialog(viewer: &Viewer) -> Option<String> {
        viewer
            .state
            .borrow()
            .signature
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.title())
            .map(|title| title.to_string())
    }

    fn the_armed_picture(viewer: &Viewer) -> Option<Vec<u8>> {
        viewer
            .state
            .borrow()
            .signature
            .armed
            .as_ref()
            .map(|armed| armed.png.clone())
    }

    /// A pad with one line on it, ready for **Use**.
    fn a_signed_pad(viewer: &Viewer) -> pad::PadView {
        let view = pad::open(viewer, the_session(viewer));
        view.press((10.0, 10.0));
        view.drag_to((90.0, 40.0));
        view.release();
        view
    }

    #[gtk::test]
    fn gtk_ui_without_a_document_the_flow_says_so_and_opens_nothing() {
        let built = Built::new();
        let viewer = built.viewer();

        open(viewer);

        assert_eq!(viewer.status.text(), NO_DOCUMENT);
        assert!(!has_dialog(viewer));
    }

    #[gtk::test]
    fn gtk_ui_a_document_that_forbids_annotation_opens_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        built.with_store(MemorySignatureStore::default());
        open_document(viewer);
        if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
            session.annotation_access = AnnotationAccess::Forbidden;
        }

        open(viewer);
        settle_for(Duration::from_millis(100));

        assert!(!has_dialog(viewer));
        assert_eq!(
            viewer.status.text(),
            "This document does not permit annotation changes."
        );
    }

    #[gtk::test]
    fn gtk_ui_with_nothing_remembered_the_flow_opens_the_pad() {
        let built = Built::new();
        let viewer = built.viewer();
        built.with_store(MemorySignatureStore::default());
        open_document(viewer);

        open(viewer);
        settle_until(|| has_dialog(viewer));

        assert_eq!(
            title_of_the_open_dialog(viewer).as_deref(),
            Some("Draw your signature")
        );
    }

    #[gtk::test]
    fn gtk_ui_with_a_signature_remembered_the_flow_offers_it_first() {
        let built = Built::new();
        let viewer = built.viewer();
        built.with_store(MemorySignatureStore::holding(&a_png()));
        open_document(viewer);

        open(viewer);
        settle_until(|| has_dialog(viewer));

        assert_eq!(
            title_of_the_open_dialog(viewer).as_deref(),
            Some("Your signature")
        );
    }

    #[gtk::test]
    fn gtk_ui_a_second_click_while_a_dialog_is_open_changes_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        built.with_store(MemorySignatureStore::default());
        open_document(viewer);
        open(viewer);
        settle_until(|| has_dialog(viewer));
        let first = viewer.state.borrow().signature.dialog.clone();

        open(viewer);
        settle_for(Duration::from_millis(100));

        assert_eq!(viewer.state.borrow().signature.dialog, first);
    }

    #[gtk::test]
    fn gtk_ui_a_dialog_that_loads_after_another_document_opened_is_not_shown() {
        let built = Built::new();
        let viewer = built.viewer();
        built.with_store(MemorySignatureStore::default());
        open_document(viewer);
        open(viewer);
        open_document(viewer);

        settle_for(Duration::from_millis(100));

        assert!(!has_dialog(viewer));
    }

    #[gtk::test]
    fn gtk_ui_using_the_pad_arms_the_picture_and_remembers_it() {
        let built = Built::new();
        let viewer = built.viewer();
        let store = built.with_store(MemorySignatureStore::default());
        open_document(viewer);
        let view = a_signed_pad(viewer);

        view.use_button.emit_clicked();
        settle_until(|| is_armed(viewer) && store.current().is_some());

        let armed_picture = the_armed_picture(viewer).expect("the picture is armed");
        assert_eq!(&armed_picture[..4], b"\x89PNG");
        assert_eq!(store.current(), Some(armed_picture));
        assert!(!has_dialog(viewer), "the pad is gone");
    }

    #[gtk::test]
    fn gtk_ui_unticking_remember_uses_the_signature_once_and_keeps_the_saved_one() {
        let built = Built::new();
        let viewer = built.viewer();
        let store = built.with_store(MemorySignatureStore::holding(b"the saved one"));
        open_document(viewer);
        let view = a_signed_pad(viewer);
        view.remember.set_active(false);

        view.use_button.emit_clicked();
        settle_until(|| is_armed(viewer));
        settle_for(Duration::from_millis(100));

        assert!(is_armed(viewer));
        assert_eq!(store.current(), Some(b"the saved one".to_vec()));
    }

    #[gtk::test]
    fn gtk_ui_a_store_that_cannot_write_still_arms_and_says_it_was_not_remembered() {
        struct ReadOnlyStore;
        impl SignatureStore for ReadOnlyStore {
            fn load(&self) -> Option<Vec<u8>> {
                None
            }
            fn save(&self, _png: &[u8]) -> bool {
                false
            }
            fn delete(&self) {}
        }

        let built = Built::new();
        let viewer = built.viewer();
        viewer.state.borrow_mut().signature.store = Arc::new(ReadOnlyStore);
        open_document(viewer);
        let view = a_signed_pad(viewer);

        view.use_button.emit_clicked();
        settle_until(|| viewer.status.text() == NOT_REMEMBERED);

        assert!(is_armed(viewer));
        assert_eq!(viewer.status.text(), NOT_REMEMBERED);
    }

    #[gtk::test]
    fn gtk_ui_a_pad_cancelled_while_the_picture_is_made_arms_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        let store = built.with_store(MemorySignatureStore::default());
        open_document(viewer);
        let view = a_signed_pad(viewer);

        view.use_button.emit_clicked();
        dismiss(viewer, &view.window);
        settle_for(Duration::from_millis(200));

        assert!(!is_armed(viewer));
        assert_eq!(store.current(), None, "a cancelled pad remembers nothing");
    }

    #[gtk::test]
    fn gtk_ui_a_picture_that_arrives_after_another_document_opened_arms_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        let store = built.with_store(MemorySignatureStore::default());
        open_document(viewer);
        let view = a_signed_pad(viewer);

        view.use_button.emit_clicked();
        open_document(viewer);
        settle_for(Duration::from_millis(200));

        assert!(!is_armed(viewer));
        assert_eq!(store.current(), None);
    }

    #[gtk::test]
    fn gtk_ui_a_pad_replaced_by_another_dialog_cannot_arm() {
        let built = Built::new();
        let viewer = built.viewer();
        built.with_store(MemorySignatureStore::default());
        open_document(viewer);
        let first = a_signed_pad(viewer);
        first.use_button.emit_clicked();

        let second = pad::open(viewer, the_session(viewer));
        settle_for(Duration::from_millis(200));

        assert!(!is_armed(viewer));
        assert!(is_tracked(viewer, &second.window));
        assert!(!is_tracked(viewer, &first.window));
    }

    #[gtk::test]
    fn gtk_ui_the_offers_use_arms_the_remembered_signature_without_a_pad() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let remembered = a_png();
        let view = offer::open(viewer, the_session(viewer), remembered.clone());

        view.use_button.emit_clicked();

        assert_eq!(the_armed_picture(viewer), Some(remembered));
        assert!(!has_dialog(viewer));
    }

    #[gtk::test]
    fn gtk_ui_the_offers_draw_new_opens_the_pad_in_its_place() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let view = offer::open(viewer, the_session(viewer), a_png());

        view.draw_new.emit_clicked();

        assert_eq!(
            title_of_the_open_dialog(viewer).as_deref(),
            Some("Draw your signature")
        );
        assert!(!is_armed(viewer));
    }

    #[gtk::test]
    fn gtk_ui_the_offers_delete_forgets_the_signature_and_arms_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        let store = built.with_store(MemorySignatureStore::holding(&a_png()));
        open_document(viewer);
        let view = offer::open(viewer, the_session(viewer), a_png());

        view.delete.emit_clicked();
        settle_until(|| store.current().is_none());

        assert_eq!(store.current(), None);
        assert!(!is_armed(viewer));
        assert!(!has_dialog(viewer), "no confirmation, no leftover dialog");
        assert_eq!(viewer.status.text(), FORGOTTEN);
    }

    #[gtk::test]
    fn gtk_ui_the_offers_cancel_closes_it_and_keeps_the_signature() {
        let built = Built::new();
        let viewer = built.viewer();
        let store = built.with_store(MemorySignatureStore::holding(&a_png()));
        open_document(viewer);
        let view = offer::open(viewer, the_session(viewer), a_png());

        view.cancel.emit_clicked();

        assert!(!has_dialog(viewer));
        assert!(store.current().is_some());
        assert!(!is_armed(viewer));
    }
}
