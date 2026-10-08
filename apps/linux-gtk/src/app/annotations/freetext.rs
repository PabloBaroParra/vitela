//! Text boxes (`FreeText`): the dialog that asks for the text before a box is
//! recorded, the same dialog prefilled to retype one, and the rules both share.
//!
//! Mirrors [`super::note`] and the Windows shell's text dialogs: a modal with
//! a multiline field, a confirm button that stays disabled while the text is
//! blank, a Cancel that abandons the work, and a re-check after the dialog
//! closes that the document is still the one it was opened on.
//!
//! One thing a note never needs: a text box is drawn in Helvetica, so the core
//! refuses any character WinAnsi cannot show. That refusal is surfaced *in the
//! dialog*, with the text kept, rather than as a status line after the dialog
//! is gone — losing a paragraph to one stray character would be cruel.
//!
//! The rules (what counts as blank, what the refusal says, what an unchanged
//! edit is) are plain functions; the dialog is a thin wrapper over them.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gdk, glib, Box as GtkBox, Button, EventControllerKey, Label, Orientation, ScrolledWindow,
    TextBuffer, TextView, Window, WrapMode,
};
use pdf_annotate::AnnotateError;
use pdf_document::{Annotation, AnnotationId, AnnotationKind, Command, PageId, Rect};

use crate::app::state::{Viewer, PAGE_NO_LONGER_PRESENT};

use super::builder::is_blank_note;
use super::command::{apply_command, command, model};
use super::geometry::clamped_to_page;
use super::note::current_session_id;
use super::SELECTION_GONE;

const ADD_CANCELED: &str = "Text box placement canceled.";
const EDIT_CANCELED: &str = "Text box edit canceled.";
const NEEDS_TEXT: &str = "Type the text box's text before adding it.";
const TOO_SMALL: &str = "That text box is too small.";
const SELECT_A_TEXT_BOX: &str = "Select a text box first.";
const DOCUMENT_CHANGED: &str = "The document changed while the dialog was open; nothing was added.";
const NO_DIALOG: &str = "Could not open the text box dialog.";
const UNCHANGED: &str = "Text unchanged.";

/// What the dialog does with the text when the user confirms. `Err` keeps the
/// dialog open and shows the message in it.
type Confirm = Rc<dyn Fn(&Viewer, String) -> Result<(), String>>;

/// Whether the confirm button may be pressed for `text`.
///
/// The one rule shared with [`validate_text`], so the button cannot be enabled
/// for text the builder will then refuse as blank.
pub(super) fn add_enabled(text: &str) -> bool {
    !is_blank_note(text)
}

/// What the user is told when the core refuses their text.
///
/// A character WinAnsi cannot show names the character, so they can find and
/// remove it in a long paragraph.
pub(super) fn text_error_message(error: &AnnotateError) -> String {
    match error {
        AnnotateError::EncodingGap { character } => format!(
            "The character '{character}' can't be used in a text box: it is drawn in \
             Helvetica, which only covers Western European text. Remove it and try again."
        ),
        AnnotateError::InvalidRect => TOO_SMALL.to_string(),
        AnnotateError::UnsupportedOperation("empty free text") => NEEDS_TEXT.to_string(),
        other => other.to_string(),
    }
}

/// Checks `text` against the core's rules without building anything that is
/// kept, so the dialog can refuse in place.
pub(super) fn validate_text(text: &str) -> Result<(), String> {
    if !add_enabled(text) {
        return Err(NEEDS_TEXT.to_string());
    }
    // Any drawable rect will do: the text rules do not depend on it.
    let probe = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 50.0,
    };
    pdf_annotate::free_text(AnnotationId(0), PageId(0), probe, text)
        .map(|_| ())
        .map_err(|error| text_error_message(&error))
}

/// Builds the text box a user finished typing.
pub(super) fn free_text_annotation(
    id: AnnotationId,
    page: PageId,
    rect: Rect,
    text: &str,
) -> Result<Annotation, String> {
    pdf_annotate::free_text(id, page, rect, text).map_err(|error| text_error_message(&error))
}

/// `annotation` with its text replaced, or `None` when that changes nothing.
///
/// An unchanged edit records no undo step: a log full of no-ops would make
/// Ctrl+Z walk through steps that do nothing.
pub(super) fn retyped(annotation: &Annotation, text: &str) -> Result<Option<Annotation>, String> {
    let mut after = annotation.clone();
    pdf_annotate::set_annotation_contents(&mut after, text)
        .map_err(|error| text_error_message(&error))?;
    Ok((after != *annotation).then_some(after))
}

/// Whether `annotation` is a text box, i.e. whether "Edit text" applies.
pub(super) fn is_text_box(annotation: &Annotation) -> bool {
    matches!(annotation.kind, AnnotationKind::FreeText { .. })
}

/// Whether a dialog opened on session `opened` has outlived its document.
///
/// `current` is `None` with nothing open. Replaced and closed documents are
/// both stale: attaching a page rect to a different document would put the box
/// somewhere meaningless.
pub(super) fn is_stale(opened: u64, current: Option<u64>) -> bool {
    current != Some(opened)
}

/// Tells the user why nothing happened, when the document withholds
/// annotation. Checked before any dialog is built, so a refusal never costs
/// the user a typed paragraph.
fn refused(viewer: &Viewer) -> bool {
    match viewer.annotation_editing_refusal() {
        Some(refusal) => {
            viewer.status.set_text(refusal);
            true
        }
        None => false,
    }
}

/// The page's size in PDF space, when it is known.
fn page_size(viewer: &Viewer, page_index: usize) -> Option<(f64, f64)> {
    let state = viewer.state.borrow();
    let slot = state.session.as_ref()?.pages.get(page_index)?;
    Some(slot.placement().unrotated())
}

/// Opens the "Add text box" dialog for a box the user has just placed on
/// `page_index`. Nothing is recorded until they confirm.
pub(super) fn prompt_for_free_text(viewer: &Viewer, page_index: usize, rect: Rect) {
    open_add_dialog(viewer, page_index, rect);
}

fn open_add_dialog(viewer: &Viewer, page_index: usize, rect: Rect) -> Option<Dialog> {
    if refused(viewer) {
        return None;
    }
    let session_id = current_session_id(viewer)?;
    // A click near the edge asks for a box that hangs off the page.
    let rect = match page_size(viewer, page_index) {
        Some((width, height)) => clamped_to_page(rect, width, height),
        None => rect,
    };
    let on_confirm: Confirm = Rc::new(move |viewer: &Viewer, text: String| {
        commit_new(viewer, session_id, page_index, rect, text)
    });
    present(
        viewer,
        session_id,
        Spec {
            title: format!("Add text box \u{2014} page {}", page_index + 1),
            caption: "Text",
            confirm_label: "Add",
            initial: String::new(),
            canceled: ADD_CANCELED,
        },
        on_confirm,
    )
}

/// Opens the dialog prefilled with the selected text box's text.
pub(super) fn edit_selected_text(viewer: &Viewer) {
    open_edit_dialog(viewer);
}

fn open_edit_dialog(viewer: &Viewer) -> Option<Dialog> {
    if refused(viewer) {
        return None;
    }
    let Some((session_id, id, contents)) = selected_text_box(viewer) else {
        viewer.status.set_text(SELECT_A_TEXT_BOX);
        return None;
    };
    let on_confirm: Confirm =
        Rc::new(move |viewer: &Viewer, text: String| commit_edit(viewer, session_id, id, text));
    present(
        viewer,
        session_id,
        Spec {
            title: "Edit text box".to_string(),
            caption: "Text",
            confirm_label: "Save",
            initial: contents,
            canceled: EDIT_CANCELED,
        },
        on_confirm,
    )
}

/// The selected annotation's session, id and text, when it is a text box.
fn selected_text_box(viewer: &Viewer) -> Option<(u64, AnnotationId, String)> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    let id = session.selected_annotation?;
    let annotation = session.document_model.as_ref()?.annotations.get(id)?;
    match &annotation.kind {
        AnnotationKind::FreeText { contents, .. } => Some((state.session_id, id, contents.clone())),
        _ => None,
    }
}

/// Records the new box, unless the dialog outlived the document it was opened
/// on. `command` repeats the permission check, so a document that stopped
/// allowing annotation while the dialog was open refuses here too.
fn commit_new(
    viewer: &Viewer,
    session_id: u64,
    page_index: usize,
    rect: Rect,
    text: String,
) -> Result<(), String> {
    if is_stale(session_id, current_session_id(viewer)) {
        viewer.status.set_text(DOCUMENT_CHANGED);
        return Ok(());
    }
    validate_text(&text)?;
    command(viewer, move |session| {
        let id = AnnotationId(session.next_annotation_id);
        let page = session
            .backend_page_id(page_index)
            .ok_or_else(|| PAGE_NO_LONGER_PRESENT.to_string())?;
        let annotation = free_text_annotation(id, page, rect, &text)?;
        {
            let document = model(session)?;
            apply_command(document, Command::AddAnnotation(annotation));
        }
        session.next_annotation_id += 1;
        session.selected_annotation = Some(id);
        Ok("Text box added. Changes are pending save.".to_string())
    });
    Ok(())
}

/// Replaces the text of box `id`: one `ReplaceAnnotation`, so one undo step.
fn commit_edit(
    viewer: &Viewer,
    session_id: u64,
    id: AnnotationId,
    text: String,
) -> Result<(), String> {
    if is_stale(session_id, current_session_id(viewer)) {
        viewer.status.set_text(DOCUMENT_CHANGED);
        return Ok(());
    }
    validate_text(&text)?;
    let current = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.document_model.as_ref())
        .and_then(|document| document.annotations.get(id))
        .cloned();
    let Some(current) = current else {
        viewer.status.set_text(SELECTION_GONE);
        return Ok(());
    };
    // Checked before `command`, which would mark the document unsaved for an
    // edit that changed nothing.
    if retyped(&current, &text)?.is_none() {
        viewer.status.set_text(UNCHANGED);
        return Ok(());
    }
    command(viewer, move |session| {
        let document = model(session)?;
        let before = document
            .annotations
            .get(id)
            .cloned()
            .ok_or_else(|| SELECTION_GONE.to_string())?;
        let after = retyped(&before, &text)?.ok_or_else(|| UNCHANGED.to_string())?;
        apply_command(document, Command::ReplaceAnnotation { before, after });
        Ok("Text box edited. Changes are pending save.".to_string())
    });
    Ok(())
}

/// What a dialog says and starts with.
struct Spec {
    title: String,
    caption: &'static str,
    confirm_label: &'static str,
    initial: String,
    /// Reported when the dialog closes without confirming.
    canceled: &'static str,
}

/// The widgets of an open dialog, kept so a test can drive them. Production
/// code only needs the dialog to exist, so the fields go unread outside tests.
#[cfg_attr(not(test), allow(dead_code))]
struct Dialog {
    window: Window,
    text: TextView,
    confirm: Button,
    cancel: Button,
    error: Label,
}

fn buffer_text(buffer: &TextBuffer) -> String {
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, false).to_string()
}

fn present(viewer: &Viewer, session_id: u64, spec: Spec, on_confirm: Confirm) -> Option<Dialog> {
    let Some(parent) = viewer.window() else {
        viewer.status.set_text(NO_DIALOG);
        return None;
    };

    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let window = Window::builder()
        .transient_for(&parent)
        .modal(true)
        .title(spec.title.as_str())
        .child(&content)
        .build();

    let caption = Label::new(Some(spec.caption));
    caption.set_xalign(0.0);
    let text = TextView::builder()
        .wrap_mode(WrapMode::WordChar)
        .accepts_tab(false)
        .build();
    let scroller = ScrolledWindow::builder()
        .child(&text)
        .min_content_width(320)
        .min_content_height(120)
        .max_content_height(300)
        .build();
    let error = Label::new(None);
    error.set_xalign(0.0);
    error.set_wrap(true);
    error.add_css_class("error");
    error.set_visible(false);
    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = Button::with_label("Cancel");
    let confirm = Button::with_label(spec.confirm_label);
    buttons.append(&cancel);
    buttons.append(&confirm);
    content.append(&caption);
    content.append(&scroller);
    content.append(&error);
    content.append(&buttons);

    let buffer = text.buffer();
    buffer.set_text(&spec.initial);
    confirm.set_sensitive(add_enabled(&spec.initial));
    buffer.connect_changed({
        let confirm = confirm.clone();
        let error = error.clone();
        move |buffer| {
            confirm.set_sensitive(add_enabled(&buffer_text(buffer)));
            // The message was about the text that has just changed.
            error.set_visible(false);
        }
    });

    // Set once the confirm has succeeded, so closing the window afterwards is
    // not mistaken for a cancel.
    let committed = Rc::new(Cell::new(false));
    confirm.connect_clicked({
        let viewer = viewer.clone();
        let window = window.clone();
        let buffer = buffer.clone();
        let committed = committed.clone();
        let error = error.clone();
        move |_| {
            let typed = buffer_text(&buffer);
            if !add_enabled(&typed) {
                return;
            }
            match on_confirm(&viewer, typed) {
                Ok(()) => {
                    committed.set(true);
                    window.close();
                }
                Err(message) => {
                    error.set_text(&message);
                    error.set_visible(true);
                }
            }
        }
    });
    cancel.connect_clicked({
        let window = window.clone();
        move |_| window.close()
    });
    // Cancel, Escape and the window's own close button all land here.
    window.connect_close_request({
        let viewer = viewer.clone();
        let canceled = spec.canceled;
        move |_| {
            if !committed.get() && current_session_id(&viewer) == Some(session_id) {
                viewer.status.set_text(canceled);
            }
            glib::Propagation::Proceed
        }
    });
    let keys = EventControllerKey::new();
    keys.connect_key_pressed({
        let window = window.clone();
        move |_, key, _, _| {
            if key == gdk::Key::Escape {
                window.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);

    window.present();
    text.grab_focus();
    Some(Dialog {
        window,
        text,
        confirm,
        cancel,
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::signature::test_support::{open_document, Built};
    use crate::app::state::AnnotationAccess;

    const PAGE_RECT: Rect = Rect {
        x: 100.0,
        y: 600.0,
        width: 200.0,
        height: 50.0,
    };

    // --- pure rules -----------------------------------------------------

    #[test]
    fn confirm_is_enabled_only_for_text_with_something_in_it() {
        assert!(!add_enabled(""));
        assert!(!add_enabled("   "));
        assert!(!add_enabled("\n\t \n"));
        assert!(add_enabled("x"));
        assert!(add_enabled("  x  "));
    }

    #[test]
    fn a_character_helvetica_cannot_show_is_named_in_the_message() {
        let message = validate_text("Hola \u{65E5}\u{672C}").expect_err("CJK is not WinAnsi");

        assert!(message.contains('\u{65E5}'), "{message}");
    }

    #[test]
    fn spanish_text_passes_validation() {
        assert_eq!(
            validate_text("Canci\u{F3}n, a\u{F1}os, \u{BF}qu\u{E9}?"),
            Ok(())
        );
    }

    #[test]
    fn blank_text_is_refused_with_the_needs_text_message() {
        assert_eq!(validate_text("  "), Err(NEEDS_TEXT.to_string()));
    }

    #[test]
    fn an_unchanged_edit_is_not_an_edit() {
        let text_box = free_text_annotation(AnnotationId(1), PageId(0), PAGE_RECT, "uno")
            .expect("valid text box");

        assert_eq!(retyped(&text_box, "uno"), Ok(None));
    }

    #[test]
    fn a_changed_edit_carries_the_new_text_and_keeps_the_rect() {
        let text_box = free_text_annotation(AnnotationId(1), PageId(0), PAGE_RECT, "uno")
            .expect("valid text box");

        let after = retyped(&text_box, "dos")
            .expect("valid")
            .expect("text changed");

        match after.kind {
            AnnotationKind::FreeText { contents, rect, .. } => {
                assert_eq!(contents, "dos");
                assert_eq!(rect, PAGE_RECT);
            }
            other => panic!("expected a text box, got {other:?}"),
        }
    }

    #[test]
    fn a_retype_with_an_unsupported_character_is_refused() {
        let text_box = free_text_annotation(AnnotationId(1), PageId(0), PAGE_RECT, "uno")
            .expect("valid text box");

        assert!(retyped(&text_box, "\u{65E5}").is_err());
    }

    #[test]
    fn a_dialog_is_stale_once_its_document_is_replaced_or_closed() {
        assert!(!is_stale(3, Some(3)));
        assert!(is_stale(3, Some(4)));
        assert!(is_stale(3, None));
    }

    #[test]
    fn only_a_text_box_offers_edit_text() {
        let text_box = free_text_annotation(AnnotationId(1), PageId(0), PAGE_RECT, "uno")
            .expect("valid text box");
        let note = pdf_annotate::text_note(AnnotationId(2), PageId(0), PAGE_RECT, "uno");

        assert!(is_text_box(&text_box));
        assert!(!is_text_box(&note));
    }

    // --- the dialog -----------------------------------------------------

    /// Destroys the dialog when the test ends, also when it panics: `#[gtk::test]`
    /// cases share one thread, so a modal left behind would still be there for
    /// the next case.
    struct Closing(Window);

    impl Drop for Closing {
        fn drop(&mut self) {
            self.0.destroy();
        }
    }

    fn annotation_count(viewer: &Viewer) -> usize {
        viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.document_model.as_ref())
            .map_or(0, |document| document.annotations.iter().count())
    }

    fn can_undo(viewer: &Viewer) -> bool {
        viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.document_model.as_ref())
            .is_some_and(|document| document.pending_edits.can_undo())
    }

    fn undo_once(viewer: &Viewer) {
        let mut state = viewer.state.borrow_mut();
        let document = state
            .session
            .as_mut()
            .and_then(|session| session.document_model.as_mut())
            .expect("a model");
        let mut log = std::mem::take(&mut document.pending_edits);
        log.undo(document);
        document.pending_edits = log;
    }

    fn contents_of(viewer: &Viewer, id: AnnotationId) -> Option<String> {
        let state = viewer.state.borrow();
        let document = state.session.as_ref()?.document_model.as_ref()?;
        match &document.annotations.get(id)?.kind {
            AnnotationKind::FreeText { contents, .. } => Some(contents.clone()),
            _ => None,
        }
    }

    /// Seeds a text box straight into the model (no undo entry) and selects it.
    fn seed_selected_text_box(viewer: &Viewer, text: &str) -> AnnotationId {
        let id = AnnotationId(50);
        let annotation = free_text_annotation(id, PageId(0), PAGE_RECT, text).expect("valid");
        let mut state = viewer.state.borrow_mut();
        let session = state.session.as_mut().expect("a document is open");
        session
            .document_model
            .as_mut()
            .expect("a model")
            .annotations
            .insert(annotation);
        session.selected_annotation = Some(id);
        id
    }

    #[gtk::test]
    fn gtk_ui_confirm_follows_the_text_between_blank_and_not() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let dialog = open_add_dialog(viewer, 0, PAGE_RECT).expect("a dialog");
        let _closing = Closing(dialog.window.clone());

        assert!(!dialog.confirm.is_sensitive(), "starts blank");
        dialog.text.buffer().set_text("x");
        assert!(dialog.confirm.is_sensitive());
        dialog.text.buffer().set_text("  \n ");
        assert!(!dialog.confirm.is_sensitive());
    }

    #[gtk::test]
    fn gtk_ui_cancel_adds_nothing_and_leaves_no_undo_step() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let dialog = open_add_dialog(viewer, 0, PAGE_RECT).expect("a dialog");
        let _closing = Closing(dialog.window.clone());
        dialog.text.buffer().set_text("Hola");

        dialog.cancel.emit_clicked();

        assert_eq!(annotation_count(viewer), 0);
        assert!(!can_undo(viewer));
        assert_eq!(viewer.status.text(), ADD_CANCELED);
    }

    #[gtk::test]
    fn gtk_ui_confirm_adds_one_text_box_in_one_undo_step() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let dialog = open_add_dialog(viewer, 0, PAGE_RECT).expect("a dialog");
        let _closing = Closing(dialog.window.clone());
        dialog.text.buffer().set_text("Hola");

        dialog.confirm.emit_clicked();

        assert_eq!(annotation_count(viewer), 1);
        assert!(can_undo(viewer));
        undo_once(viewer);
        assert_eq!(annotation_count(viewer), 0);
        assert!(!can_undo(viewer), "adding was exactly one step");
    }

    #[gtk::test]
    fn gtk_ui_an_unsupported_character_keeps_the_dialog_and_the_text() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let dialog = open_add_dialog(viewer, 0, PAGE_RECT).expect("a dialog");
        let _closing = Closing(dialog.window.clone());
        dialog.text.buffer().set_text("Hola \u{65E5}\u{672C}");

        dialog.confirm.emit_clicked();

        assert!(dialog.error.is_visible());
        assert!(dialog.error.text().contains('\u{65E5}'));
        assert_eq!(buffer_text(&dialog.text.buffer()), "Hola \u{65E5}\u{672C}");
        assert_eq!(annotation_count(viewer), 0);
        assert!(!can_undo(viewer));
    }

    #[gtk::test]
    fn gtk_ui_typing_again_clears_the_refusal() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let dialog = open_add_dialog(viewer, 0, PAGE_RECT).expect("a dialog");
        let _closing = Closing(dialog.window.clone());
        dialog.text.buffer().set_text("\u{65E5}");
        dialog.confirm.emit_clicked();
        assert!(dialog.error.is_visible());

        dialog.text.buffer().set_text("Hola");

        assert!(!dialog.error.is_visible());
    }

    #[gtk::test]
    fn gtk_ui_a_dialog_that_outlives_its_document_adds_nothing_to_the_next_one() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let dialog = open_add_dialog(viewer, 0, PAGE_RECT).expect("a dialog");
        let _closing = Closing(dialog.window.clone());
        dialog.text.buffer().set_text("Hola");
        open_document(viewer);

        dialog.confirm.emit_clicked();

        assert_eq!(annotation_count(viewer), 0);
        assert_eq!(viewer.status.text(), DOCUMENT_CHANGED);
    }

    #[gtk::test]
    fn gtk_ui_a_document_that_forbids_annotating_is_refused_before_any_dialog() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
            session.annotation_access = AnnotationAccess::Forbidden;
        }

        let dialog = open_add_dialog(viewer, 0, PAGE_RECT);

        assert!(dialog.is_none());
        assert_eq!(
            viewer.status.text(),
            "This document does not permit annotation changes."
        );
    }

    #[gtk::test]
    fn gtk_ui_editing_starts_from_the_current_text() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        seed_selected_text_box(viewer, "uno");
        let dialog = open_edit_dialog(viewer).expect("a dialog");
        let _closing = Closing(dialog.window.clone());

        assert_eq!(buffer_text(&dialog.text.buffer()), "uno");
        assert!(dialog.confirm.is_sensitive());
    }

    #[gtk::test]
    fn gtk_ui_an_edit_is_one_undo_step_and_undo_restores_the_text() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let id = seed_selected_text_box(viewer, "uno");
        let dialog = open_edit_dialog(viewer).expect("a dialog");
        let _closing = Closing(dialog.window.clone());
        dialog.text.buffer().set_text("dos");

        dialog.confirm.emit_clicked();

        assert_eq!(contents_of(viewer, id).as_deref(), Some("dos"));
        undo_once(viewer);
        assert_eq!(contents_of(viewer, id).as_deref(), Some("uno"));
        assert!(!can_undo(viewer), "the edit was exactly one step");
    }

    #[gtk::test]
    fn gtk_ui_confirming_unchanged_text_records_no_undo_step() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        seed_selected_text_box(viewer, "uno");
        let dialog = open_edit_dialog(viewer).expect("a dialog");
        let _closing = Closing(dialog.window.clone());

        dialog.confirm.emit_clicked();

        assert!(!can_undo(viewer));
        assert_eq!(viewer.status.text(), UNCHANGED);
    }

    #[gtk::test]
    fn gtk_ui_cancelling_an_edit_changes_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let id = seed_selected_text_box(viewer, "uno");
        let dialog = open_edit_dialog(viewer).expect("a dialog");
        let _closing = Closing(dialog.window.clone());
        dialog.text.buffer().set_text("dos");

        dialog.cancel.emit_clicked();

        assert_eq!(contents_of(viewer, id).as_deref(), Some("uno"));
        assert!(!can_undo(viewer));
        assert_eq!(viewer.status.text(), EDIT_CANCELED);
    }

    #[gtk::test]
    fn gtk_ui_an_edit_dialog_that_outlives_its_document_changes_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        seed_selected_text_box(viewer, "uno");
        let dialog = open_edit_dialog(viewer).expect("a dialog");
        let _closing = Closing(dialog.window.clone());
        dialog.text.buffer().set_text("dos");
        open_document(viewer);
        let id = seed_selected_text_box(viewer, "tres");

        dialog.confirm.emit_clicked();

        assert_eq!(contents_of(viewer, id).as_deref(), Some("tres"));
        assert!(!can_undo(viewer));
    }

    #[gtk::test]
    fn gtk_ui_editing_with_a_note_selected_opens_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        {
            let mut state = viewer.state.borrow_mut();
            let session = state.session.as_mut().expect("a document is open");
            session
                .document_model
                .as_mut()
                .expect("a model")
                .annotations
                .insert(pdf_annotate::text_note(
                    AnnotationId(60),
                    PageId(0),
                    PAGE_RECT,
                    "a note",
                ));
            session.selected_annotation = Some(AnnotationId(60));
        }

        assert!(open_edit_dialog(viewer).is_none());
        assert_eq!(viewer.status.text(), SELECT_A_TEXT_BOX);
    }
}
