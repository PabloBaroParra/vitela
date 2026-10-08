//! The "Add note" dialog: a sticky note's text is asked for before the note is
//! recorded, so a note is never created empty or with placeholder text.
//!
//! Mirrors the Windows shell's `PlaceTextNoteAsync`: a modal with a multiline
//! field, an Add button that stays disabled while the text is blank, a Cancel
//! that abandons the placement, and — because the dialog waits on the user — a
//! re-check after it closes that the document is still the one the rect was
//! drawn on, and that annotating is still allowed.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gdk, glib, Box as GtkBox, Button, EventControllerKey, Label, Orientation, ScrolledWindow,
    TextView, Window, WrapMode,
};
use pdf_document::{AnnotationId, Command, Rect};

use crate::app::state::{Viewer, PAGE_NO_LONGER_PRESENT};

use super::builder::{is_blank_note, note_annotation};
use super::command::{apply_command, command, model};

const CANCELED: &str = "Note placement canceled.";

/// Opens the dialog for a note whose rect the user has just traced on
/// `page_index`. Nothing is recorded until they press Add.
pub(super) fn prompt_for_note(viewer: &Viewer, page_index: usize, rect: Rect) {
    if let Some(refusal) = viewer.annotation_editing_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    let Some(session_id) = current_session_id(viewer) else {
        return;
    };
    let Some(window) = viewer.window() else {
        viewer.status.set_text("Could not open the note dialog.");
        return;
    };

    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(&window)
        .modal(true)
        .title(format!("Add note \u{2014} page {}", page_index + 1))
        .child(&content)
        .build();

    let label = Label::new(Some("Note text"));
    label.set_xalign(0.0);
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
    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = Button::with_label("Cancel");
    let add = Button::with_label("Add");
    add.set_sensitive(false);
    buttons.append(&cancel);
    buttons.append(&add);
    content.append(&label);
    content.append(&scroller);
    content.append(&buttons);

    let buffer = text.buffer();
    buffer.connect_changed({
        let add = add.clone();
        move |buffer| {
            let (start, end) = buffer.bounds();
            add.set_sensitive(!is_blank_note(buffer.text(&start, &end, false).as_str()));
        }
    });

    // Set once Add has run, so closing the window afterwards is not mistaken
    // for a cancel.
    let committed = Rc::new(Cell::new(false));
    add.connect_clicked({
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        let buffer = buffer.clone();
        let committed = committed.clone();
        move |_| {
            let (start, end) = buffer.bounds();
            let note = buffer.text(&start, &end, false).to_string();
            if is_blank_note(&note) {
                return;
            }
            committed.set(true);
            dialog.close();
            commit_note(&viewer, session_id, page_index, rect, note);
        }
    });
    cancel.connect_clicked({
        let dialog = dialog.clone();
        move |_| dialog.close()
    });
    // Cancel, Escape and the window's own close button all land here.
    dialog.connect_close_request({
        let viewer = viewer.clone();
        move |_| {
            if !committed.get() && current_session_id(&viewer) == Some(session_id) {
                viewer.status.set_text(CANCELED);
            }
            glib::Propagation::Proceed
        }
    });
    let keys = EventControllerKey::new();
    keys.connect_key_pressed({
        let dialog = dialog.clone();
        move |_, key, _, _| {
            if key == gdk::Key::Escape {
                dialog.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    });
    dialog.add_controller(keys);

    dialog.present();
    text.grab_focus();
}

/// Records the note, unless the dialog outlived the document it was opened on.
///
/// The prompt awaits the user, so by now the document may have been replaced:
/// attaching the old page rect to a different document would put a note
/// somewhere meaningless. `command` repeats the permission check, so a
/// document that stopped allowing annotation while the dialog was open refuses
/// here too.
fn commit_note(viewer: &Viewer, session_id: u64, page_index: usize, rect: Rect, text: String) {
    if current_session_id(viewer) != Some(session_id) {
        return;
    }
    command(viewer, move |session| {
        let id = AnnotationId(session.next_annotation_id);
        let page = session
            .backend_page_id(page_index)
            .ok_or_else(|| PAGE_NO_LONGER_PRESENT.to_string())?;
        let annotation = note_annotation(id, page, rect, &text)?;
        {
            let document = model(session)?;
            apply_command(document, Command::AddAnnotation(annotation));
        }
        session.next_annotation_id += 1;
        session.selected_annotation = Some(id);
        Ok("Text note added. Changes are pending save.".to_string())
    });
}

/// Which document session is open, or `None` with nothing open.
fn current_session_id(viewer: &Viewer) -> Option<u64> {
    let state = viewer.state.borrow();
    state.session.as_ref().map(|_| state.session_id)
}
