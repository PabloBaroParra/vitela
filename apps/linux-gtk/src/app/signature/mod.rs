//! Drawing a signature with the mouse and remembering it on this computer
//! (T-088) — the Linux half of what Android shipped in PRs #350 and #353.
//!
//! The Sign section of the tools panel gets a **Draw signature** entry. It opens
//! a modal pad; the strokes become a PNG with a transparent background and black
//! ink; the PNG arms the *same* image stamp Ctrl+V and a dropped image file use,
//! and the next click on a page places it through the core's stamp placement.
//! With **Remember on this computer** ticked (the default) the PNG is kept in
//! the user's data directory, and the next **Draw signature** offers it first.
//!
//! This is not the cryptographic signature in [`super::sign`]: that one signs
//! the file; this one is a picture stamped on a page.
//!
//! Why the strokes become a PNG *here*, in the shell, and not in the core: it is
//! the one step that depends on how the platform's pen and canvas report ink —
//! pad pixels, pen width, a toolkit's path renderer. The core's contract starts
//! one step later, at "image bytes in, stamp out" (`stamp_from_image_bytes`,
//! `stamp_placement`), which every shell shares. Android drew the same line, and
//! the geometry rules in [`ink`] are its rules, so the same strokes give the same
//! picture on every platform.
//!
//! Split by responsibility:
//!
//! - [`ink`] — the pad's rules and the crop/scale geometry. Pure.
//! - [`png`] — strokes to PNG bytes. Cairo draws, the `image` crate encodes.
//! - [`store`] — the remembered file, behind a trait.
//! - [`pad`] and [`offer`] — the two dialogs. They collect answers only.
//! - [`flow`] — which dialog opens and what each answer does.
//! - [`armed`] — a signature waiting for its page: arming, releasing, placing.

mod armed;
mod flow;
mod ink;
mod offer;
mod pad;
mod png;
mod store;

#[cfg(test)]
mod test_support;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button};

use crate::app::state::Viewer;
use crate::app::tools_panel::FILL_SIGN_PAGE;

pub(crate) use armed::place_armed;
pub(crate) use store::{default_store, SignatureStore};

const DRAW_LABEL: &str = "Draw signature…";
const DRAW_TOOLTIP: &str =
    "Draw your signature with the mouse, then click a page to place it as an image";

/// Builds the **Draw signature** entry and appends it to the Sign section.
///
/// Insensitive until a document that permits annotation is open —
/// [`update_draw_signature_control`] owns every transition out of that state.
pub(crate) fn build_draw_signature_button(sign_content: &GtkBox) -> Button {
    let button = Button::with_label(DRAW_LABEL);
    button.set_tooltip_text(Some(DRAW_TOOLTIP));
    button.set_sensitive(false);
    sign_content.append(&button);
    button
}

/// Wires the entry, and releases a waiting signature when the user leaves the
/// Sign tab: nothing on another tool's panel says a click is about to place a
/// picture.
pub(crate) fn connect_signature(viewer: &Viewer) {
    viewer.draw_signature.connect_clicked({
        let viewer = viewer.clone();
        move |_| flow::open(&viewer)
    });
    viewer.tools_stack.connect_visible_child_notify({
        let viewer = viewer.clone();
        move |stack| {
            if stack.visible_child_name().as_deref() != Some(FILL_SIGN_PAGE) {
                armed::disarm(&viewer);
            }
        }
    });
}

/// Refreshes the entry's sensitivity. Called from
/// `sign::update_sign_controls`, which `document::show_document` runs whenever a
/// document opens, closes or is reloaded.
///
/// The question is the annotation one, not the signing one: the result is a
/// stamp, so a document that forbids annotation changes cannot take it, and
/// with no document there is no page to place it on.
pub(crate) fn update_draw_signature_control(viewer: &Viewer) {
    let has_document = viewer.state.borrow().session.is_some();
    let enabled = has_document && viewer.annotation_editing_refusal().is_none();
    viewer.draw_signature.set_sensitive(enabled);
}

/// The id of the document open right now, or `None` with no document. Changes
/// only when `document::show_document` replaces the visible document, which is
/// what makes it the right thing to capture before background work: a result
/// that comes back to a different id was made for a document that is gone.
fn current_session_id(viewer: &Viewer) -> Option<u64> {
    let state = viewer.state.borrow();
    state.session.as_ref().map(|_| state.session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::app::state::AnnotationAccess;

    use super::test_support::{open_document, Built};

    #[gtk::test]
    fn gtk_ui_the_sign_section_offers_draw_signature_disabled_until_a_document_opens() {
        let built = Built::new();
        let viewer = built.viewer();

        assert_eq!(viewer.draw_signature.label().as_deref(), Some(DRAW_LABEL));
        assert!(!viewer.draw_signature.is_sensitive());
        assert!(
            viewer.draw_signature.is_ancestor(&viewer.tools_stack),
            "the entry lives in the tools panel"
        );
    }

    #[gtk::test]
    fn gtk_ui_draw_signature_follows_the_open_document() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);

        crate::app::sign::update_sign_controls(viewer);

        assert!(viewer.draw_signature.is_sensitive());
    }

    #[gtk::test]
    fn gtk_ui_draw_signature_stays_disabled_on_a_document_that_forbids_annotation() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
            session.annotation_access = AnnotationAccess::Forbidden;
        }

        crate::app::sign::update_sign_controls(viewer);

        assert!(!viewer.draw_signature.is_sensitive());
    }

    #[gtk::test]
    fn gtk_ui_draw_signature_is_disabled_again_when_the_document_closes() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        crate::app::sign::update_sign_controls(viewer);
        viewer.state.borrow_mut().session = None;

        crate::app::sign::update_sign_controls(viewer);

        assert!(!viewer.draw_signature.is_sensitive());
    }

    #[gtk::test]
    fn gtk_ui_clicking_draw_signature_opens_the_pad() {
        let built = Built::new();
        let viewer = built.viewer();
        built.with_store(super::store::tests::MemorySignatureStore::default());
        open_document(viewer);
        crate::app::sign::update_sign_controls(viewer);

        viewer.draw_signature.emit_clicked();
        super::test_support::settle_until(|| viewer.state.borrow().signature.dialog.is_some());

        assert!(viewer.state.borrow().signature.dialog.is_some());
    }
}
