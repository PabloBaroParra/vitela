//! The inline text editor a content-edit-mode click opens over a text run.
//!
//! First widget-over-page-coordinates pattern in this shell: annotations are
//! painted on the highlights `DrawingArea`, never real widgets, and the
//! password prompt is a modal top-level window rather than something
//! positioned against a page. This adds a real `gtk::Entry` as a third child
//! of the page's `Overlay`, positioned with plain margins computed from
//! `pdf_render::place_rect` — the same "coordinates are pre-scaled in Rust,
//! no cairo transform" posture the rest of the shell already uses.
//!
//! Being a real widget is also what lets a *new* box be nudged into place
//! before anything is typed into it: the same margins that position it are
//! what [`wire_drag`] updates. That is the only drag this module owns —
//! moving a run that already exists belongs to the page gesture in
//! [`super::text`], which does not have to fight the text field for the
//! press.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gdk, glib, Entry, EventControllerFocus, EventControllerKey, EventSequenceState, GestureDrag,
    PropagationPhase,
};
use pdf_document::{Command, ContentItemId, FontKind, PageId, Rect, TextRun};
use pdf_edit::EditError;
use pdf_render::{place_rect, TextRect};

use crate::app::document::refresh_after_content_edit;
use crate::app::state::{ContentEditor, PageSlot, Viewer};

use super::command::{
    amend_command, amended_command, apply_command, pending_text_command, validate_insert_text,
    validate_replacement, PendingText,
};
use super::model;
use super::CLICK_EPSILON_PX;

/// Fixed default box for a newly inserted text run (T-163), in PDF points —
/// there is no existing run to inherit a size from the way a replacement
/// does. 150pt wide is comfortably more than a short phrase in Helvetica at
/// this height; 14pt tall matches `insert_text_run`'s own reading of
/// `bbox.height` as the font size (`core/pdf-edit/src/insert.rs`), so 14pt
/// is both the box height and the point size the text is drawn at — large
/// enough to read at 100% zoom.
const INSERT_TEXT_WIDTH_PT: f64 = 150.0;
const INSERT_TEXT_HEIGHT_PT: f64 = 14.0;

/// Opens an inline editor over `run` on `page_index`.
///
/// Resolves whatever editor is already open first — a click always commits
/// the edit in progress before starting a new one, never abandons it
/// silently. Composite-font runs never open an editor at all: `pdf-edit`
/// rejects every replacement against one outright, so there is nothing an
/// editor here could do but fail; the refusal is reported immediately
/// instead, reusing `EditError`'s own message.
///
/// A run that already has an unsaved edit queued against it opens like any
/// other, prefilled with the text that edit gave it — the box on screen shows
/// what the page shows. What changes is where the commit lands: `amends`
/// carries the log entry to fold the result into, rather than a second
/// command being appended.
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

    // `run` may have come from `model::overlay_pending_content` rather than
    // the base document — a run that only exists because of a pending,
    // unsaved insertion, or one already carrying a queued replacement. Either
    // way its next edit amends the command already describing it instead of
    // queueing a second one no save could resolve; see
    // `command::pending_text_command`.
    let pending = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.document_model.as_ref())
        .map_or(PendingText::Nothing, |document| {
            pending_text_command(document, &run)
        });
    let amends = match pending {
        PendingText::Nothing => None,
        PendingText::Amend(index) => Some(index),
        // A run the overlay showed against a log that no longer holds the
        // command behind it. There is nothing to amend and nothing in the
        // base document to target either, so opening an editor could only
        // produce a command `pdf-edit` fails to resolve at save time —
        // refused here, same posture as the composite-font case above.
        PendingText::Unresolvable => {
            viewer.status.set_text(
                "This text is part of an unsaved edit that is no longer available — save \
                 and reopen before editing it again.",
            );
            return;
        }
    };

    let entry = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(page) = session.pages.get(page_index) else {
            return;
        };

        let entry = Entry::new();
        entry.set_text(&run.text);
        place_entry(&entry, run.bbox, page);
        page.overlay.add_overlay(&entry);

        session.content_editor = Some(ContentEditor {
            page_index,
            run: run.clone(),
            entry: entry.clone(),
            is_insertion: false,
            amends,
        });

        entry
    };

    wire_entry(viewer, &entry, false);
    entry.grab_focus();
    entry.select_region(0, -1);
}

/// Opens a blank inline editor at `point` (PDF page space) to compose a
/// brand-new text run (T-163's "insert text" sub-mode), rather than
/// retyping an existing one.
///
/// Resolves whatever editor is already open first, same as [`open_editor`]:
/// a click always commits the edit in progress before starting a new one.
/// Unlike a replacement there is no run to read a font/size/position from,
/// so this picks a fixed default box anchored at `point` and a font
/// resource name guaranteed not to collide with one already on the page —
/// see [`super::model::unused_font_resource_name`]'s own doc for why a
/// colliding name would be a silent miscoding bug, not just a cosmetic one.
pub(crate) fn open_insert_editor(viewer: &Viewer, page_index: usize, point: (f64, f64)) {
    commit(viewer);

    let entry = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(base) = session
            .save_backing
            .as_ref()
            .map(|backing| backing.base.as_lopdf())
        else {
            return;
        };
        // Collected into an owned `Vec` before `session.pages` is borrowed
        // mutably below — see `image::apply_insertion` for the same shape and
        // the reason the base document alone cannot answer this.
        let reserved = session
            .document_model
            .as_ref()
            .map(|document| model::reserved_font_resource_names(&document.pending_edits))
            .unwrap_or_default();
        let pending = session
            .document_model
            .as_ref()
            .map(|document| &document.pending_edits);
        let Some(page) = session.pages.get_mut(page_index) else {
            return;
        };
        let resource_font_name =
            match model::ensure_page_content(&mut page.content, base, page_index, pending) {
                Ok(content) => model::unused_font_resource_name(content, &reserved),
                Err(error) => {
                    drop(state);
                    viewer.status.set_text(&error.to_string());
                    return;
                }
            };

        // The box's bottom-left sits at `point`, matching how
        // `pdf_edit::insert::insert_text_run` reads the bbox back: the
        // baseline it computes (`bbox.y + 0.25 * size`) sits just above the
        // box's own bottom edge, so what the user clicked is where the text
        // actually lands, not somewhere inside an invisible margin.
        let bbox = Rect {
            x: point.0,
            y: point.1,
            width: INSERT_TEXT_WIDTH_PT,
            height: INSERT_TEXT_HEIGHT_PT,
        };
        let run = TextRun {
            // Never consulted by `pdf_edit::insert_text_run` — insertion has
            // no existing item to target, so this id is a placeholder only;
            // `PageContent`'s real ids come from the *next* parse, once the
            // run this template describes actually exists on the page.
            id: ContentItemId(0),
            page: PageId(page_index as u32),
            bbox,
            resource_font_name,
            font_kind: FontKind::Standard14,
            text: String::new(),
        };

        let entry = Entry::new();
        place_entry(&entry, bbox, page);
        page.overlay.add_overlay(&entry);

        session.content_editor = Some(ContentEditor {
            page_index,
            run,
            entry: entry.clone(),
            is_insertion: true,
            // Nothing to amend: this run has no command of its own yet. The
            // one this commit records is what a *later* click on the same
            // text will amend, via the synthetic id the overlay mints for it.
            amends: None,
        });

        entry
    };

    wire_entry(viewer, &entry, true);
    entry.grab_focus();
}

/// Positions the editor box over `bbox` on `page`, in the page overlay's
/// own coordinates.
///
/// The one place the widget's geometry is computed, shared by opening an
/// editor, opening a blank one for an insertion, and putting the box back
/// after a refused drag — three call sites that must agree on where a given
/// run's box sits, or a rejected move would leave the box somewhere the run
/// is not.
fn place_entry(entry: &Entry, bbox: Rect, page: &PageSlot) {
    let placed = place_rect(
        TextRect {
            x_pt: bbox.x as f32,
            y_pt: bbox.y as f32,
            width_pt: bbox.width as f32,
            height_pt: bbox.height as f32,
        },
        page.height_pt,
        page.budget.factor,
    );

    entry.set_halign(gtk::Align::Start);
    entry.set_valign(gtk::Align::Start);
    // Clamped at zero because GTK refuses a negative margin, which is also
    // what stops a drag from carrying the box off the top-left of the page.
    entry.set_margin_start(placed.left.round().max(0.0) as i32);
    entry.set_margin_top(placed.top.round().max(0.0) as i32);
    entry.set_width_request(placed.width.round().max(1.0) as i32);
    entry.set_height_request(placed.height.round().max(1.0) as i32);
}

/// Makes an insertion's box draggable, so a new text box can be nudged into
/// place before anything is typed into it.
///
/// **Only an insertion's box.** An editor opened over an existing run does
/// not get this, deliberately: dragging inside a text field is how anyone
/// selects text with a mouse, and a gesture that stole it would trade an
/// everyday interaction for one the page already offers — pressing the run
/// itself and dragging it (`super::text::begin_text_drag`). A blank box has
/// no text to select yet, so there is nothing to trade.
///
/// Press and pull moves the box; press and release places the caret. Both
/// live in the same pixels and distance tells them apart, using the same
/// [`CLICK_EPSILON_PX`] threshold the page gesture uses. The controller runs
/// in the **capture** phase, ahead of the `Entry`'s own text gesture, and
/// claims the sequence only once the pointer has actually travelled.
fn wire_drag(viewer: &Viewer, entry: &Entry) {
    let drag = GestureDrag::new();
    drag.set_propagation_phase(PropagationPhase::Capture);

    // Where the box's margins stood when the press landed. `None` means this
    // press must not move anything, either because there is no editor to move
    // or because moving this particular run was refused before the drag could
    // start.
    let anchor: Rc<Cell<Option<(i32, i32)>>> = Rc::new(Cell::new(None));
    // Whether the pointer has passed the threshold, i.e. whether this gesture
    // has become a move rather than a click.
    let moving = Rc::new(Cell::new(false));

    drag.connect_drag_begin({
        let viewer = viewer.clone();
        let entry = entry.clone();
        let anchor = anchor.clone();
        let moving = moving.clone();
        move |_, _, _| {
            moving.set(false);
            anchor.set(
                is_composing_an_insertion(&viewer)
                    .then(|| (entry.margin_start(), entry.margin_top())),
            );
        }
    });

    drag.connect_drag_update({
        let entry = entry.clone();
        let anchor = anchor.clone();
        let moving = moving.clone();
        move |gesture, offset_x, offset_y| {
            let Some((left, top)) = anchor.get() else {
                return;
            };
            if !moving.get() {
                if offset_x.abs() < CLICK_EPSILON_PX && offset_y.abs() < CLICK_EPSILON_PX {
                    return;
                }
                // Past the threshold, so this is a move. Claiming the
                // sequence is what stops the `Entry` from going on selecting
                // text under the pointer for the rest of the drag.
                gesture.set_state(EventSequenceState::Claimed);
                moving.set(true);
            }
            entry.set_margin_start((f64::from(left) + offset_x).round().max(0.0) as i32);
            entry.set_margin_top((f64::from(top) + offset_y).round().max(0.0) as i32);
        }
    });

    drag.connect_drag_end({
        let viewer = viewer.clone();
        let entry = entry.clone();
        move |_, _, _| {
            let Some((left, top)) = anchor.take() else {
                return;
            };
            if !moving.replace(false) {
                return;
            }
            // Measured from the margins the box actually ended up with rather
            // than from the pointer's own offset: `place_entry` clamps at the
            // page's top-left corner, and reading the widget back is what
            // keeps the move that gets recorded equal to the one the user
            // watched happen.
            finish_drag(
                &viewer,
                entry.margin_start() - left,
                entry.margin_top() - top,
            );
        }
    });

    entry.add_controller(drag);
}

/// Whether the open editor is composing a run that exists nowhere yet — the
/// one state in which the box itself may be dragged.
///
/// `amends` being set means the run is already in the log (and, for a
/// replacement, in the file), so moving it is a real edit that belongs to the
/// page gesture, not to this widget.
fn is_composing_an_insertion(viewer: &Viewer) -> bool {
    let state = viewer.state.borrow();
    state
        .session
        .as_ref()
        .and_then(|session| session.content_editor.as_ref())
        .is_some_and(|editor| editor.is_insertion && editor.amends.is_none())
}

/// Resolves a finished drag of an insertion's box, `dx`/`dy` being how far it
/// actually moved in device pixels.
///
/// Nothing reaches the `EditLog`: the run this box will insert does not exist
/// yet, so moving the box is not an edit of the page, only a different
/// description of what the eventual commit will add. That is what makes
/// "click, nudge into place, type" one uninterrupted gesture rather than
/// three edits.
fn finish_drag(viewer: &Viewer, dx: i32, dy: i32) {
    if dx == 0 && dy == 0 {
        return;
    }

    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    let Some(editor) = session.content_editor.as_ref() else {
        return;
    };
    let page_index = editor.page_index;
    let bbox = editor.run.bbox;
    let Some(page) = session.pages.get(page_index) else {
        return;
    };
    let scale = page.budget.factor;
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }

    // Screen pixels grow downwards and PDF points grow upwards, which is the
    // whole of the difference between the two spaces here: the box keeps the
    // size it was composed with, so only the origin moves.
    let destination = Rect {
        x: bbox.x + f64::from(dx) / scale,
        y: bbox.y - f64::from(dy) / scale,
        ..bbox
    };
    session
        .content_editor
        .as_mut()
        .expect("read through the same field just above")
        .run
        .bbox = destination;
}

fn wire_entry(viewer: &Viewer, entry: &Entry, draggable: bool) {
    if draggable {
        wire_drag(viewer, entry);
    }

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
/// Three shapes, in the order they are tried: amending the command a run
/// already has queued (`ContentEditor::amends`), inserting a brand-new run
/// (`is_insertion`), or replacing an untouched one. All three validate
/// against the same base document — the one the whole log was recorded
/// against — before anything reaches the log.
///
/// A no-op text (`after == run.text`) closes without touching the `EditLog`
/// at all: for a replacement that means retyping the same words is not an
/// edit, and for an insertion (`is_insertion`, T-163) `run.text` starts as
/// the empty string, so leaving the box empty is the same "nothing to
/// record" case rather than a special one. A failed validation leaves the
/// editor open with the user's text intact either way: see
/// `command::validate_replacement`/`validate_insert_text`'s docs for why this
/// is checked before the command is recorded rather than only at save time.
/// Safe to call with no editor open (focus-out and Enter both route here).
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
    let is_insertion = editor.is_insertion;
    let amends = editor.amends;
    let run = editor.run.clone();
    let base = session
        .save_backing
        .as_ref()
        .expect("content_edit_refusal already required a model, which requires save_backing")
        .base
        .as_lopdf();

    // Checked before `is_insertion`, and it wins: a run that came off the
    // overlay is a retype of something already queued whether that something
    // was an insertion or a replacement, and the entry it amends is what
    // decides which. `is_insertion` only describes how *this* editor was
    // opened, which for a second edit is always "over an existing run".
    if let Some(index) = amends {
        let existing = session
            .document_model
            .as_ref()
            .and_then(|document| document.pending_edits.entries().get(index))
            .cloned();
        // Both `None` cases mean the log moved on underneath an editor opened
        // against it (the model was dropped, or the entry is no longer a text
        // command). Closing without recording is the honest outcome: there is
        // no entry left to amend, and recording a fresh command instead would
        // be the very duplicate this path exists to avoid.
        let Some(amended) = existing
            .as_ref()
            .and_then(|existing| amended_command(existing, &after, None))
        else {
            let editor = session.content_editor.take().expect("checked above");
            drop(state);
            detach(viewer, &editor);
            // Said out loud rather than swallowed: the user typed something
            // and it is not being recorded, which is exactly the outcome that
            // must never happen quietly.
            viewer
                .status
                .set_text("That edit is no longer available — nothing was recorded.");
            return;
        };

        let validated = match &amended {
            Command::InsertTextRun(run) => validate_insert_text(base, page_index, run),
            Command::ReplaceTextRunContent { item, after } => {
                validate_replacement(base, page_index, item, after)
            }
            // `amended_command` produces no other shape.
            _ => Ok(()),
        };

        match validated {
            Ok(()) => {
                let document = session
                    .document_model
                    .as_mut()
                    .expect("read through the same field just above");
                // The log's own refusal is honoured rather than assumed away:
                // claiming "Text updated." over an amendment it declined would
                // mark the document dirty and re-render it unchanged, which
                // reads to the user as their retype vanishing.
                let recorded = amend_command(document, index, amended);
                let editor = session.content_editor.take().expect("checked above");
                if !recorded {
                    drop(state);
                    detach(viewer, &editor);
                    viewer
                        .status
                        .set_text("That edit is no longer available — nothing was recorded.");
                    return;
                }
                session.edit_revision += 1;
                // Same reason as every other branch: recorded now, so a
                // failed refresh cannot leave a dirty document reporting clean.
                session.unsaved_to_disk = true;
                drop(state);
                detach(viewer, &editor);
                refresh_after_content_edit(viewer, "Text updated.");
            }
            Err(error) => {
                drop(state);
                viewer.status.set_text(&error.to_string());
            }
        }
        return;
    }

    if is_insertion {
        let mut new_run = run;
        new_run.text = after;
        match validate_insert_text(base, page_index, &new_run) {
            Ok(()) => {
                let document = session
                    .document_model
                    .as_mut()
                    .expect("content_edit_refusal already required a model");
                apply_command(document, Command::InsertTextRun(new_run));
                session.edit_revision += 1;
                // Marked here, at the moment the command joins the log, not
                // when `refresh_after_content_edit` lands: a refresh that
                // fails still leaves a recorded edit behind, and a document
                // that reports itself clean is one the open-another-document
                // guard will discard without asking.
                session.unsaved_to_disk = true;
                let editor = session.content_editor.take().expect("checked above");
                drop(state);
                detach(viewer, &editor);
                refresh_after_content_edit(viewer, "Text inserted.");
            }
            Err(error) => {
                drop(state);
                viewer.status.set_text(&error.to_string());
            }
        }
        return;
    }

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
            // Same reason as the insertion branch above: recorded now, so a
            // failed refresh cannot leave a dirty document reporting clean.
            session.unsaved_to_disk = true;
            let editor = session.content_editor.take().expect("checked above");
            drop(state);
            detach(viewer, &editor);
            refresh_after_content_edit(viewer, "Text updated.");
        }
        Err(error) => {
            // A move recorded just above stays recorded: it validated on its
            // own and undo can still remove it. Only the retype failed, and
            // the editor stays open holding it.
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
