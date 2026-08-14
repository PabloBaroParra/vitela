//! The inline text editor a content-edit-mode click opens over a text run.
//!
//! First widget-over-page-coordinates pattern in this shell: annotations are
//! painted on the highlights `DrawingArea`, never real widgets, and the
//! password prompt is a modal top-level window rather than something
//! positioned against a page. This adds a real `gtk::Entry` as a third child
//! of the page's `Overlay`, positioned with plain margins computed from
//! `pdf_render::place_rect` — the same "coordinates are pre-scaled in Rust,
//! no cairo transform" posture the rest of the shell already uses.

use gtk::prelude::*;
use gtk::{gdk, glib, Entry, EventControllerFocus, EventControllerKey};
use pdf_document::{Command, FontKind, TextRun};
use pdf_edit::EditError;
use pdf_render::{place_rect, TextRect};

use crate::app::state::{ContentEditor, Viewer};

use super::command::{apply_command, validate_replacement};

/// Opens an inline editor over `run` on `page_index`.
///
/// Resolves whatever editor is already open first — a click always commits
/// the edit in progress before starting a new one, never abandons it
/// silently. Composite-font runs never open an editor at all: `pdf-edit`
/// rejects every replacement against one outright, so there is nothing an
/// editor here could do but fail; the refusal is reported immediately
/// instead, reusing `EditError`'s own message.
pub(crate) fn open_editor(viewer: &Viewer, page_index: usize, run: TextRun) {
    commit(viewer);

    if run.font_kind == FontKind::EmbeddedComposite {
        viewer.status.set_text(
            &EditError::CompositeFontNotEditable {
                resource_font_name: run.resource_font_name.clone(),
            }
            .to_string(),
        );
        return;
    }

    let entry = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(page) = session.pages.get(page_index) else {
            return;
        };

        let placed = place_rect(
            TextRect {
                x_pt: run.bbox.x as f32,
                y_pt: run.bbox.y as f32,
                width_pt: run.bbox.width as f32,
                height_pt: run.bbox.height as f32,
            },
            page.height_pt,
            page.budget.factor,
        );

        let entry = Entry::new();
        entry.set_text(&run.text);
        entry.set_halign(gtk::Align::Start);
        entry.set_valign(gtk::Align::Start);
        entry.set_margin_start(placed.left.round() as i32);
        entry.set_margin_top(placed.top.round() as i32);
        entry.set_width_request(placed.width.round().max(1.0) as i32);
        entry.set_height_request(placed.height.round().max(1.0) as i32);
        page.overlay.add_overlay(&entry);

        session.content_editor = Some(ContentEditor {
            page_index,
            run: run.clone(),
            entry: entry.clone(),
        });

        entry
    };

    wire_entry(viewer, &entry);
    entry.grab_focus();
    entry.select_region(0, -1);
}

fn wire_entry(viewer: &Viewer, entry: &Entry) {
    entry.connect_activate({
        let viewer = viewer.clone();
        move |_| commit(&viewer)
    });

    let focus = EventControllerFocus::new();
    focus.connect_leave({
        let viewer = viewer.clone();
        move |_| commit(&viewer)
    });
    entry.add_controller(focus);

    let keys = EventControllerKey::new();
    keys.connect_key_pressed({
        let viewer = viewer.clone();
        move |_, keyval, _keycode, _state| {
            if keyval == gdk::Key::Escape {
                cancel(&viewer);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    entry.add_controller(keys);
}

/// Validates and records the open editor's text, then closes it.
///
/// A no-op text (`after == run.text`) closes without touching the `EditLog`
/// at all — retyping the same words is not an edit. A failed validation
/// leaves the editor open with the user's text intact: see
/// `command::validate_replacement`'s doc for why this is checked before the
/// command is recorded rather than only at save time. Safe to call with no
/// editor open (focus-out and Enter both route here).
pub(crate) fn commit(viewer: &Viewer) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return;
    }

    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    let Some(editor) = session.content_editor.as_ref() else {
        return;
    };
    let after = editor.entry.text().to_string();

    if after == editor.run.text {
        let editor = session.content_editor.take().expect("checked above");
        drop(state);
        detach(viewer, &editor);
        return;
    }

    let page_index = editor.page_index;
    let run = editor.run.clone();
    let base = session
        .save_backing
        .as_ref()
        .expect("content_edit_refusal already required a model, which requires save_backing")
        .base
        .as_lopdf();

    match validate_replacement(base, page_index, &run, &after) {
        Ok(()) => {
            let document = session
                .document_model
                .as_mut()
                .expect("content_edit_refusal already required a model");
            apply_command(
                document,
                Command::ReplaceTextRunContent {
                    item: run,
                    after: after.clone(),
                },
            );
            session.edit_revision += 1;
            let editor = session.content_editor.take().expect("checked above");
            drop(state);
            detach(viewer, &editor);
            viewer
                .status
                .set_text("Text updated. Changes are pending save.");
        }
        Err(error) => {
            drop(state);
            viewer.status.set_text(&error.to_string());
        }
    }
}

/// Discards the open editor without recording anything (Escape, or content
/// edit mode being switched off while one is open).
pub(crate) fn cancel(viewer: &Viewer) {
    let editor = {
        let mut state = viewer.state.borrow_mut();
        state
            .session
            .as_mut()
            .and_then(|session| session.content_editor.take())
    };
    if let Some(editor) = editor {
        detach(viewer, &editor);
        viewer.status.set_text("Edit cancelled.");
    }
}

fn detach(viewer: &Viewer, editor: &ContentEditor) {
    let state = viewer.state.borrow();
    if let Some(page) = state
        .session
        .as_ref()
        .and_then(|session| session.pages.get(editor.page_index))
    {
        page.overlay.remove_overlay(&editor.entry);
    }
}
