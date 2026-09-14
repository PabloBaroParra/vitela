//! The inline text editor a content-edit-mode click opens over a text run.
//!
//! First widget-over-page-coordinates pattern in this shell: annotations are
//! painted on the highlights `DrawingArea`, never real widgets, and the
//! password prompt is a modal top-level window rather than something
//! positioned against a page. This adds a real `gtk::Entry` as a third child
//! of the page's `Overlay`, positioned from `pdf_render::place_rect` — the
//! same "coordinates are pre-scaled in Rust" posture the rest of the shell
//! uses.
//!
//! The one exception to "no transform" is the page's own turn. A `/Rotate`
//! makes the run this box is editing run down the screen rather than across
//! it, and a box whose text stayed level would be a box that no longer looks
//! like the thing it is replacing. GTK4 can only turn a widget through a
//! `GtkFixed`'s child transform, so the entry sits inside a `Fixed` that is
//! itself placed the way the bare entry used to be — see [`place_entry`].
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
    gdk, glib, graphene, gsk, Entry, EventControllerFocus, EventControllerKey, EventSequenceState,
    Fixed, GestureDrag, PropagationPhase, ScrolledWindow,
};
use pdf_document::{Command, ContentItemId, FontKind, Rect, TextRun};
use pdf_edit::EditError;
use pdf_render::{place_point, point_to_pdf, PagePlacement, PageRotation};

use crate::app::state::{ContentEditor, Viewer};
use crate::app::update_content_edit_controls;
use crate::app::write::refresh_preview;

use super::command::{
    amend_command, amended_command, apply_command, pending_move_index, pending_text_command,
    validate_insert_text, validate_remove_text, validate_replacement, PendingText,
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
    let (pending, moved) = {
        let state = viewer.state.borrow();
        let document = state
            .session
            .as_ref()
            .and_then(|session| session.document_model.as_ref());
        match document {
            Some(document) => (
                pending_text_command(document, &run),
                pending_move_index(document, &run).is_some(),
            ),
            None => (PendingText::Nothing, false),
        }
    };
    let amends = match pending {
        // `pending_text_command` is deliberately blind to a pending
        // `MoveTextRun` (see its own doc — `text::plan_move` needs `Nothing`
        // here to amend a second move itself), so a run only moved this
        // session still has to be caught here before falling through to an
        // ordinary replacement: `commit`'s replace branch validates against
        // `base`, which the move never touched, so the run's *current*
        // (moved) bbox would fail to resolve there with an opaque "no
        // content item" — the retype and the move are not two operations
        // `pdf-edit` has a combined command for, unlike two retypes of the
        // same run.
        PendingText::Nothing if moved => {
            viewer.status.set_text(
                "This text was moved and not yet saved — save and reopen before retyping it.",
            );
            return;
        }
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
        let frame = Fixed::new();
        frame.put(&entry, 0.0, 0.0);
        place_entry(&frame, &entry, run.bbox, page.placement());
        page.overlay.add_overlay(&frame);

        session.content_editor = Some(ContentEditor {
            page_index,
            run: run.clone(),
            entry: entry.clone(),
            frame,
            is_insertion: false,
            amends,
        });

        entry
    };

    wire_entry(viewer, &entry, false);
    focus_without_scrolling(&viewer.scroll, &entry);
    entry.select_region(0, -1);
    // The Edit page's "Delete text" is gated on exactly this editor being
    // open over an existing run, so the page learns about it here — the
    // image card's controls are refreshed the same way from every
    // select/deselect in `super::image`.
    update_content_edit_controls(viewer);
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
        let Some(base) = session.save_backing.as_ref().map(|backing| &backing.base) else {
            return;
        };
        let Some(document) = session.document_model.as_ref() else {
            return;
        };
        // Collected into an owned `Vec` before `session.pages` is borrowed
        // mutably below — see `image::apply_insertion` for the same shape and
        // the reason the base document alone cannot answer this.
        let reserved = model::reserved_font_resource_names(&document.pending_edits);
        let pending = Some(&document.pending_edits);
        // The canvas index names a page id only through the open handle, and
        // only a page whose bytes this session can reach has content to
        // parse — see `super::content_page`.
        let Some(page_id) = super::content_page(session, page_index) else {
            return;
        };
        // Before the mutable `pages` borrow below: the probe reads other
        // fields of the same session.
        let Some(probe) = super::page_probe(document, base, &session.imported_sources, page_id)
        else {
            return;
        };
        let Some(page) = session.pages.get_mut(page_index) else {
            return;
        };
        let resource_font_name =
            match model::ensure_page_content(&mut page.content, probe, page_id, pending) {
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
            page: page_id,
            bbox,
            resource_font_name,
            font_kind: FontKind::Standard14,
            text: String::new(),
        };

        let entry = Entry::new();
        let frame = Fixed::new();
        frame.put(&entry, 0.0, 0.0);
        place_entry(&frame, &entry, bbox, page.placement());
        page.overlay.add_overlay(&frame);

        session.content_editor = Some(ContentEditor {
            page_index,
            run,
            entry: entry.clone(),
            frame,
            is_insertion: true,
            // Nothing to amend: this run has no command of its own yet. The
            // one this commit records is what a *later* click on the same
            // text will amend, via the synthetic id the overlay mints for it.
            amends: None,
        });

        entry
    };

    wire_entry(viewer, &entry, true);
    focus_without_scrolling(&viewer.scroll, &entry);
    // Same call as [`open_editor`]'s, for the opposite outcome: an insertion
    // has no run on the page yet, so this is what keeps "Delete text"
    // *disabled* while a blank box is open.
    update_content_edit_controls(viewer);
}

/// Puts the cursor in `entry` without letting the page canvas jump.
///
/// A box is always opened where the user just clicked, so it is on screen by
/// construction and there is never anything for a scroll to reveal. But
/// `grab_focus` on a child that has only just been added is a child the
/// `ScrolledWindow` has not measured yet, and it scrolls to the origin to
/// bring into view a widget it believes is sitting there. The reader is thrown
/// back to the start of the page and the run they were editing leaves the
/// screen.
///
/// Measured before the fix: canvas parked at 100, a run whose box computes to
/// x=144 in a 516-wide viewport — fully visible — and opening its editor left
/// the scroll at 0. Only the first focus does it; once the entry has been
/// allocated, focusing it again moves nothing.
///
/// This predates any of the rotation work and was simply never visible: with
/// the default fit there is no horizontal scroll to lose. A page turned to
/// landscape is the first thing that gives the canvas somewhere to jump from.
fn focus_without_scrolling(scroll: &ScrolledWindow, entry: &Entry) {
    let (horizontal, vertical) = (scroll.hadjustment(), scroll.vadjustment());
    let (left, top) = (horizontal.value(), vertical.value());
    entry.grab_focus();
    horizontal.set_value(left);
    vertical.set_value(top);
}

/// Positions the editor box over `bbox` on `page`, in the page overlay's
/// own coordinates.
///
/// The one place the widget's geometry is computed, shared by opening an
/// editor, opening a blank one for an insertion, and putting the box back
/// after a refused drag — three call sites that must agree on where a given
/// run's box sits, or a rejected move would leave the box somewhere the run
/// is not.
fn place_entry(frame: &Fixed, entry: &Entry, bbox: Rect, page: PagePlacement) {
    let (width, height) = entry_size(entry, bbox, page);

    // Where the run's PDF-space top-left corner lands on screen. The entry's
    // own top-left starts there whichever way the page is turned; the turn
    // decides which way the rest of the box then runs.
    let anchor = place_point((bbox.x, bbox.y + bbox.height), page);
    // The entry's box in the frame's coordinates. A quarter turn sends the
    // entry's height into *negative* screen x and the half turn sends both
    // sides negative, so the frame's origin is not the anchor: it is whichever
    // corner of the turned box comes out top-left. `local` is the anchor's
    // offset from it, and is exactly what the transform has to translate by.
    let (local_x, local_y) = match page.rotation {
        PageRotation::None => (0.0, 0.0),
        PageRotation::Clockwise90 => (height, 0.0),
        PageRotation::Clockwise180 => (width, height),
        PageRotation::Clockwise270 => (0.0, width),
    };

    frame.set_halign(gtk::Align::Start);
    frame.set_valign(gtk::Align::Start);
    // Clamped at zero because GTK refuses a negative margin, which is also
    // what stops a box from being carried off the top-left of the page.
    frame.set_margin_start((anchor.0 - local_x).round().max(0.0) as i32);
    frame.set_margin_top((anchor.1 - local_y).round().max(0.0) as i32);

    frame.set_child_transform(
        entry,
        Some(
            &gsk::Transform::new()
                .translate(&graphene::Point::new(local_x as f32, local_y as f32))
                .rotate(page.rotation.degrees() as f32),
        ),
    );
}

/// The size the entry will actually occupy, in its own upright frame.
///
/// The run's box at this zoom is only a floor. A 14pt line is ten-odd pixels
/// tall at a readable zoom and GTK will not shrink an `Entry` below the height
/// of its own font, so asking for ten and getting thirty-four is the normal
/// case, not an edge one.
///
/// As a bare overlay child that overflow was invisible — the entry simply drew
/// a little taller than the run. Inside a `Fixed` it is not: the frame measures
/// the *transformed* bounds of its child and clamps them at its own origin, so
/// on a quarter turn an entry taller than the run asked for runs off the
/// frame's left edge and everything past it is clipped away. That is a box with
/// no visible text in it. Asking the entry what it actually needs, and then
/// requesting exactly that, is what keeps the frame, the transform and the
/// widget describing one box instead of three.
fn entry_size(entry: &Entry, bbox: Rect, page: PagePlacement) -> (f64, f64) {
    let requested = |points: f64| (points * page.scale).round().max(1.0) as i32;
    entry.set_width_request(requested(bbox.width));
    entry.set_height_request(requested(bbox.height));
    // Pins the entry's natural width to its minimum. Without it the natural
    // grows with the text, and `GtkFixed` measures a child by its natural: the
    // frame would quietly widen as the user typed while the transform below
    // went on translating by the size the box had when it opened, walking the
    // text off the frame's edge on a quarter turn. It also makes the box match
    // the run it is replacing rather than the sentence being typed into it —
    // the entry scrolls its own content, the way any entry too narrow for its
    // text does.
    entry.set_max_width_chars(1);
    // Read back rather than computed: the floor is GTK's — font metrics,
    // padding and whatever the theme's CSS says an entry may not be smaller
    // than — and none of that is ours to predict.
    let (minimum, _) = entry.preferred_size();
    // Re-requested so the minimum *is* the size: `GtkFixed` allocates a child
    // its preferred size, and a request equal to the minimum leaves no room
    // for the two to disagree.
    entry.set_width_request(minimum.width());
    entry.set_height_request(minimum.height());
    (f64::from(minimum.width()), f64::from(minimum.height()))
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

    // The run's PDF-space box when the press landed, and the page it is being
    // dragged on. `None` means this press must not move anything, either
    // because there is no editor to move or because moving this particular
    // run was refused before the drag could start.
    //
    // The *box*, not the widget's margins. Screen pixels stopped being a
    // usable record of where the box is the moment a page could be turned
    // under it: on a quarter turn a pointer moving right moves the run *down*
    // the page, and a margin cannot say that. Keeping the PDF box as the truth
    // and re-placing the widget from it makes the move the user watches and
    // the move that gets recorded one piece of arithmetic instead of two that
    // have to be kept in agreement.
    let anchor: Rc<Cell<Option<(Rect, PagePlacement)>>> = Rc::new(Cell::new(None));
    // Whether the pointer has passed the threshold, i.e. whether this gesture
    // has become a move rather than a click.
    let moving = Rc::new(Cell::new(false));

    drag.connect_drag_begin({
        let viewer = viewer.clone();
        let anchor = anchor.clone();
        let moving = moving.clone();
        move |_, _, _| {
            moving.set(false);
            anchor.set(dragged_from(&viewer));
        }
    });

    drag.connect_drag_update({
        let viewer = viewer.clone();
        let anchor = anchor.clone();
        let moving = moving.clone();
        move |gesture, offset_x, offset_y| {
            let Some((origin, page)) = anchor.get() else {
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
            reposition(&viewer, dragged_box(origin, page, offset_x, offset_y));
        }
    });

    drag.connect_drag_end({
        let viewer = viewer.clone();
        move |_, offset_x, offset_y| {
            let Some((origin, page)) = anchor.take() else {
                return;
            };
            if !moving.replace(false) {
                return;
            }
            // The same box the last `drag_update` put on screen, from the same
            // inputs — so what gets recorded is what the user watched happen,
            // with no need to read the widget back to find out what that was.
            finish_drag(&viewer, dragged_box(origin, page, offset_x, offset_y));
        }
    });

    entry.add_controller(drag);
}

/// The box a drag starting now would move, and the page it sits on — or
/// `None` when this press must not move anything.
fn dragged_from(viewer: &Viewer) -> Option<(Rect, PagePlacement)> {
    if !is_composing_an_insertion(viewer) {
        return None;
    }
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    let editor = session.content_editor.as_ref()?;
    let page = session.pages.get(editor.page_index)?;
    Some((editor.run.bbox, page.placement()))
}

/// `origin` moved by a pointer offset, in PDF space, clamped to the page.
///
/// The offset is turned back into PDF space by running both of its ends
/// through [`point_to_pdf`] and subtracting, rather than by a formula of its
/// own: the transform is affine, so the difference of the two *is* the
/// offset's PDF-space twin — turn and zoom included — and there is no second
/// derivation of the turn here to drift from the first.
fn dragged_box(origin: Rect, page: PagePlacement, offset_x: f64, offset_y: f64) -> Rect {
    let (from_x, from_y) = point_to_pdf(0.0, 0.0, page);
    let (to_x, to_y) = point_to_pdf(offset_x, offset_y, page);
    let moved = Rect {
        x: origin.x + f64::from(to_x - from_x),
        y: origin.y + f64::from(to_y - from_y),
        ..origin
    };
    // Kept on the page, the way the old margin clamp kept the widget off the
    // overlay's top-left corner — but in PDF space, where "the page" means the
    // same thing at every turn, and against all four edges rather than two.
    let (page_width, page_height) = page.unrotated();
    Rect {
        x: moved.x.clamp(0.0, (page_width - moved.width).max(0.0)),
        y: moved.y.clamp(0.0, (page_height - moved.height).max(0.0)),
        ..moved
    }
}

/// Moves the open editor's box to `bbox` on screen. Live drag feedback only —
/// nothing here reaches the run or the `EditLog`.
fn reposition(viewer: &Viewer, bbox: Rect) {
    // Read out and dropped before any widget is touched: placing the frame
    // relayouts the page, and a draw func reached that way takes its own
    // borrow of this same state.
    let editor = {
        let state = viewer.state.borrow();
        state.session.as_ref().and_then(|session| {
            let editor = session.content_editor.as_ref()?;
            let page = session.pages.get(editor.page_index)?;
            Some((editor.frame.clone(), editor.entry.clone(), page.placement()))
        })
    };
    if let Some((frame, entry, page)) = editor {
        place_entry(&frame, &entry, bbox, page);
    }
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
fn finish_drag(viewer: &Viewer, destination: Rect) {
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    let Some(editor) = session.content_editor.as_mut() else {
        return;
    };
    // Only the origin ever moves: the box keeps the size it was composed
    // with, and `dragged_box` carries `width`/`height` through untouched.
    editor.run.bbox = destination;
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
    // Capture phase: `Entry`'s own internal key controller (bubble phase, by
    // default) sees Escape first otherwise and consumes it — the box's
    // selected text (`select_region` in `open_editor`) means GTK's default
    // handling has something to do with it, so it never reaches this
    // controller and `cancel` never fires.
    keys.set_propagation_phase(PropagationPhase::Capture);
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

    let is_insertion = editor.is_insertion;
    let amends = editor.amends;
    let run = editor.run.clone();
    let base = &session
        .save_backing
        .as_ref()
        .expect("content_edit_refusal already required a model, which requires save_backing")
        .base;
    let Some(document) = session.document_model.as_ref() else {
        return;
    };
    // Resolved through the run's own page rather than through a position:
    // an imported page's bytes are in the PDF it came from
    // (`super::page_probe`). A page this session cannot reach is one no
    // command against it could be validated on, so the edit is refused
    // rather than probed against the wrong document.
    let Some(probe) = super::page_probe(document, base, &session.imported_sources, run.page) else {
        let editor = session.content_editor.take().expect("checked above");
        drop(state);
        detach(viewer, &editor);
        viewer
            .status
            .set_text("That page cannot be edited — nothing was recorded.");
        return;
    };

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
            Command::InsertTextRun(run) => validate_insert_text(probe, run),
            Command::ReplaceTextRunContent { item, after } => {
                validate_replacement(probe, item, after)
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
                refresh_preview(viewer, "Text updated.");
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
        match validate_insert_text(probe, &new_run) {
            Ok(()) => {
                let document = session
                    .document_model
                    .as_mut()
                    .expect("content_edit_refusal already required a model");
                apply_command(document, Command::InsertTextRun(new_run));
                session.edit_revision += 1;
                // Marked here, at the moment the command joins the log, not
                // when `refresh_preview` lands: a refresh that
                // fails still leaves a recorded edit behind, and a document
                // that reports itself clean is one the open-another-document
                // guard will discard without asking.
                session.unsaved_to_disk = true;
                let editor = session.content_editor.take().expect("checked above");
                drop(state);
                detach(viewer, &editor);
                refresh_preview(viewer, "Text inserted.");
            }
            Err(error) => {
                drop(state);
                viewer.status.set_text(&error.to_string());
            }
        }
        return;
    }

    match validate_replacement(probe, &run, &after) {
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
            refresh_preview(viewer, "Text updated.");
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

/// Why a run that only exists as an unsaved insertion cannot be deleted from
/// here, said in terms of what to do instead.
///
/// `EditLog` has no "drop this entry" door, by design — `apply`, `undo`,
/// `redo` and `amend` are the whole surface — and neither of the two things
/// this could otherwise record is honest. A `RemoveTextRun` would name a run
/// no saved file contains, which resolves against nothing at save time and
/// fails the *entire* save; amending the insertion into an empty one would
/// leave a run painting no glyphs on the page and still answering clicks.
/// Undo removes the insertion outright, which is the operation actually being
/// asked for.
pub(crate) const INSERTION_NOT_DELETABLE: &str =
    "This text hasn't been saved yet — undo the insertion to remove it.";

/// Said when the log moved on underneath an editor opened against it — the
/// same wording, and the same reasoning, [`commit`]'s two amendment dead ends
/// already use.
const NOTHING_RECORDED: &str = "That edit is no longer available — nothing was recorded.";

/// Removes the run the open inline editor sits on, recording
/// [`Command::RemoveTextRun`] — the Edit page's "Delete text", and the text
/// half of what the image card has had since T-162.
///
/// Whatever is in the box is discarded rather than committed first: a delete
/// supersedes a retype of the same run, and recording both would queue two
/// commands against one item — the duplicate `EditLog::amend` exists to
/// prevent.
///
/// # The run's one pending command decides where this lands
///
/// - **Nothing queued** (the ordinary case): the run is as the file last
///   saved it, and a `RemoveTextRun` is appended.
/// - **A queued retype** (`ReplaceTextRunContent`): that entry is *amended*
///   into the removal, carrying the original snapshot the recorded command
///   holds rather than the run this shell hit-tested. Appending instead would
///   leave a removal whose `item` describes text the replacement already
///   replaced — resolvable against nothing at save time.
/// - **A queued insertion**: refused, see [`INSERTION_NOT_DELETABLE`].
///
/// A run with a queued **move** is not a case here: `open_editor` refuses to
/// open over one at all, so no editor is ever sitting on it for this button
/// to act through.
pub(crate) fn delete_open_run(viewer: &Viewer) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return;
    }

    /// What this delete does to the log, and the run it does it with. The
    /// run differs between the two: an amendment must carry the *recorded*
    /// snapshot, never the one the overlay handed the shell.
    enum Removal {
        Append(TextRun),
        Amend(usize, TextRun),
    }

    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    let Some(editor) = session.content_editor.as_ref() else {
        return;
    };
    // A blank insertion box has no run on the page to remove — nothing has
    // been recorded for it yet, and `Escape` is what closes it. The button is
    // already insensitive for one (`update_content_edit_controls`); this is
    // the guard behind that, not a path a click reaches.
    if editor.is_insertion {
        return;
    }
    let amends = editor.amends;
    let run = editor.run.clone();

    let removal = match amends {
        None => Removal::Append(run),
        Some(index) => {
            let existing = session
                .document_model
                .as_ref()
                .and_then(|document| document.pending_edits.entries().get(index))
                .cloned();
            match existing {
                Some(Command::ReplaceTextRunContent { item, .. }) => Removal::Amend(index, item),
                Some(Command::InsertTextRun(_)) => {
                    drop(state);
                    viewer.status.set_text(INSERTION_NOT_DELETABLE);
                    return;
                }
                _ => {
                    drop(state);
                    viewer.status.set_text(NOTHING_RECORDED);
                    return;
                }
            }
        }
    };

    let target = match &removal {
        Removal::Append(run) | Removal::Amend(_, run) => run,
    };
    let base = &session
        .save_backing
        .as_ref()
        .expect("content_edit_refusal already required a model, which requires save_backing")
        .base;
    let Some(document) = session.document_model.as_ref() else {
        return;
    };
    let Some(probe) = super::page_probe(document, base, &session.imported_sources, target.page)
    else {
        drop(state);
        viewer
            .status
            .set_text("That page cannot be edited — nothing was recorded.");
        return;
    };
    if let Err(error) = validate_remove_text(probe, target) {
        // The editor stays open holding the run, exactly as a failed
        // replacement leaves it — the delete did not happen, so the target
        // has not stopped being one.
        drop(state);
        viewer.status.set_text(&error.to_string());
        return;
    }

    let document = session
        .document_model
        .as_mut()
        .expect("content_edit_refusal already required a model");
    let recorded = match removal {
        Removal::Append(target) => {
            apply_command(document, Command::RemoveTextRun(target));
            true
        }
        // The log's own refusal is honoured rather than assumed away, the
        // same posture `commit`'s amendment branch takes: reporting a
        // deletion the log declined would mark the document dirty and
        // re-render it unchanged, which reads as the text coming back.
        Removal::Amend(index, target) => {
            amend_command(document, index, Command::RemoveTextRun(target))
        }
    };

    let editor = session.content_editor.take().expect("checked above");
    if !recorded {
        drop(state);
        detach(viewer, &editor);
        viewer.status.set_text(NOTHING_RECORDED);
        return;
    }
    session.edit_revision += 1;
    // Dirty at record time, not at refresh time — same reason every other
    // branch in this module gives: a refresh that fails still leaves a
    // recorded edit behind, and a document reporting itself clean is one the
    // open-another-document guard discards without asking.
    session.unsaved_to_disk = true;
    drop(state);
    detach(viewer, &editor);
    refresh_preview(viewer, "Text deleted.");
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

/// Takes the editor box off the page.
///
/// The single door every editor teardown goes through — commit, cancel, and
/// [`delete_open_run`] all end here — which is why the Edit page's controls
/// are refreshed from this one place rather than from each of the branches
/// above: "Delete text" is gated on an editor being open, and an editor that
/// closes without the page hearing about it leaves a live button aimed at a
/// run nothing is editing any more.
///
/// Always called with the `state` borrow already dropped (the callers do so
/// before detaching), which is what lets both this function and
/// `update_content_edit_controls` take their own.
fn detach(viewer: &Viewer, editor: &ContentEditor) {
    {
        let state = viewer.state.borrow();
        if let Some(page) = state
            .session
            .as_ref()
            .and_then(|session| session.pages.get(editor.page_index))
        {
            page.overlay.remove_overlay(&editor.frame);
        }
    }
    update_content_edit_controls(viewer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::{Overlay, Picture, Window};
    use pdf_render::PageRotation;

    /// A letter page turned `rotation`, drawn 1:1. The drawn size is already
    /// swapped for a quarter turn, the way pdfium reports it.
    fn turned(rotation: PageRotation) -> PagePlacement {
        let (width_pt, height_pt) = match rotation {
            PageRotation::Clockwise90 | PageRotation::Clockwise270 => (792.0, 612.0),
            _ => (612.0, 792.0),
        };
        PagePlacement {
            width_pt,
            height_pt,
            rotation,
            scale: 1.0,
        }
    }

    fn a_box() -> Rect {
        Rect {
            x: 100.0,
            y: 500.0,
            width: 150.0,
            height: 14.0,
        }
    }

    /// The whole reason the drag stopped speaking in margins. Dragging the box
    /// to the right is "right" on the *screen*, and on a turned page the
    /// screen's right is not the page's. Turn a page 90 degrees clockwise and
    /// the direction that used to be up now points right, so a rightward drag
    /// walks the run *up* its own page — `y` rising, since PDF y grows
    /// upwards. Three quarters of a turn puts up on the left, so the same
    /// drag walks it down.
    #[test]
    fn a_rightward_pointer_drag_moves_the_box_the_way_the_page_faces() {
        let origin = a_box();
        let moved = |rotation| dragged_box(origin, turned(rotation), 40.0, 0.0);

        let upright = moved(PageRotation::None);
        assert_eq!(
            (upright.x, upright.y),
            (140.0, 500.0),
            "along the page's +x"
        );

        let quarter = moved(PageRotation::Clockwise90);
        assert_eq!((quarter.x, quarter.y), (100.0, 540.0), "up the page");

        let half = moved(PageRotation::Clockwise180);
        assert_eq!((half.x, half.y), (60.0, 500.0), "along the page's -x");

        let three_quarters = moved(PageRotation::Clockwise270);
        assert_eq!(
            (three_quarters.x, three_quarters.y),
            (100.0, 460.0),
            "down the page"
        );
    }

    /// The box keeps the size it was composed with — only its origin moves.
    #[test]
    fn a_drag_never_resizes_the_box() {
        for rotation in [
            PageRotation::None,
            PageRotation::Clockwise90,
            PageRotation::Clockwise180,
            PageRotation::Clockwise270,
        ] {
            let moved = dragged_box(a_box(), turned(rotation), 37.0, -21.0);

            assert_eq!(
                (moved.width, moved.height),
                (a_box().width, a_box().height),
                "{rotation:?}"
            );
        }
    }

    /// The margin clamp that used to do this went away with the margins. A
    /// box dragged hard at a corner has to stop at the page edge rather than
    /// compose a run at coordinates no page holds.
    #[test]
    fn a_drag_cannot_carry_the_box_off_the_page() {
        let page = turned(PageRotation::None);
        let (page_width, page_height) = page.unrotated();

        let off_the_bottom_left = dragged_box(a_box(), page, -9_000.0, 9_000.0);
        assert_eq!((off_the_bottom_left.x, off_the_bottom_left.y), (0.0, 0.0));

        let off_the_top_right = dragged_box(a_box(), page, 9_000.0, -9_000.0);
        assert_eq!(
            (off_the_top_right.x, off_the_top_right.y),
            (page_width - a_box().width, page_height - a_box().height)
        );
    }

    /// A box wider than the page it is on would otherwise hand `f64::clamp` a
    /// range whose low end is above its high end, which panics.
    #[test]
    fn a_box_too_big_for_its_page_clamps_rather_than_panicking() {
        let oversized = Rect {
            x: 0.0,
            y: 0.0,
            width: 10_000.0,
            height: 10_000.0,
        };

        let moved = dragged_box(oversized, turned(PageRotation::None), 50.0, 50.0);

        assert_eq!((moved.x, moved.y), (0.0, 0.0));
    }

    /// The `(x, y)` a 2D `gsk::Transform` maps `point` to.
    fn transformed(transform: &gsk::Transform, point: (f64, f64)) -> (f64, f64) {
        let (xx, yx, xy, yy, dx, dy) = transform.to_2d();
        (
            f64::from(xx) * point.0 + f64::from(xy) * point.1 + f64::from(dx),
            f64::from(yx) * point.0 + f64::from(yy) * point.1 + f64::from(dy),
        )
    }

    /// The frame and the transform inside it have to describe the same box, or
    /// the entry is turned but sitting somewhere the run is not.
    ///
    /// Checked by mapping the entry's own top-left and bottom-right through the
    /// child transform and asserting they land on opposite corners of the
    /// frame's footprint — which makes this a test of the *pairing* rather than
    /// of either half alone.
    #[gtk::test]
    fn gtk_ui_the_turned_entry_covers_exactly_the_frame_it_sits_in() {
        for rotation in [
            PageRotation::None,
            PageRotation::Clockwise90,
            PageRotation::Clockwise180,
            PageRotation::Clockwise270,
        ] {
            let page = turned(rotation);
            let bbox = a_box();
            let frame = Fixed::new();
            let entry = Entry::new();
            frame.put(&entry, 0.0, 0.0);

            place_entry(&frame, &entry, bbox, page);

            // The box `place_entry` settled on, which is the entry's own and
            // not the run's — see `entry_size`.
            let (size, _) = entry.preferred_size();
            let (width, height) = (f64::from(size.width()), f64::from(size.height()));
            let (footprint_width, footprint_height) = match rotation {
                PageRotation::Clockwise90 | PageRotation::Clockwise270 => (height, width),
                _ => (width, height),
            };
            let transform = frame.child_transform(&entry).expect("place_entry sets one");
            let corners = [
                transformed(&transform, (0.0, 0.0)),
                transformed(&transform, (width, height)),
            ];

            for (x, y) in corners {
                assert!(
                    x.abs() < 1e-3 || (x - footprint_width).abs() < 1e-3,
                    "{rotation:?}: x {x} is on neither edge of a {footprint_width} wide frame"
                );
                assert!(
                    y.abs() < 1e-3 || (y - footprint_height).abs() < 1e-3,
                    "{rotation:?}: y {y} is on neither edge of a {footprint_height} tall frame"
                );
            }
            assert!(
                (corners[0].0 - corners[1].0).abs() > 1e-3
                    && (corners[0].1 - corners[1].1).abs() > 1e-3,
                "{rotation:?}: the two corners must be opposite ones, not the same"
            );

            frame.remove(&entry);
        }
    }

    /// The canvas must stay where the reader put it.
    ///
    /// `grab_focus` on a child the `ScrolledWindow` has only just been handed
    /// is a child it has not measured, and it scrolls to the origin to reveal
    /// a widget it believes is sitting there — throwing the reader back to the
    /// start of the page with the run they were editing off screen. Only the
    /// first focus does it, which is why it cannot be caught by focusing an
    /// editor that is already open.
    ///
    /// Invisible until a page is wide enough to scroll sideways, so turning a
    /// page to landscape is what surfaced it. The fault itself is older than
    /// any of the rotation work.
    ///
    /// Built on the shell's own `Viewer` rather than on a scroller assembled
    /// here: a hand-rolled one does not reproduce it, and a test that cannot
    /// fail is worse than no test. What it cannot use is a real document —
    /// the GTK gate runs without pdfium — so the canvas is given a page-shaped
    /// widget instead, which is all the scroll position depends on.
    #[gtk::test]
    fn gtk_ui_focusing_a_fresh_editor_leaves_the_canvas_where_it_was() {
        const PARKED_AT: f64 = 300.0;

        let built = crate::app::ui_tests::built_ui();
        built.viewer.view_stack.set_visible_child_name("editor");
        built.window.set_default_size(700, 600);
        built.window.present();

        // Wider than the viewport will be, so there is a horizontal scroll
        // position to lose in the first place — which on a real document is
        // what a page turned to landscape produces.
        let page = Picture::new();
        page.set_width_request(2_000);
        page.set_height_request(1_500);
        let overlay = Overlay::new();
        overlay.set_child(Some(&page));
        overlay.set_halign(gtk::Align::Center);
        built.viewer.pages.append(&overlay);
        settle(&built.viewer.pages);

        let horizontal = built.viewer.scroll.hadjustment();
        horizontal.set_value(PARKED_AT);
        pump();
        assert_eq!(
            horizontal.value(),
            PARKED_AT,
            "the canvas has to start somewhere other than the origin, \
             or this test cannot tell a jump from a no-op"
        );

        // An editor box, added and focused the way `open_editor` does it, over
        // a part of the page the reader can see: there is nothing here for a
        // scroll to reveal.
        let entry = Entry::new();
        let frame = Fixed::new();
        frame.put(&entry, 0.0, 0.0);
        frame.set_halign(gtk::Align::Start);
        frame.set_valign(gtk::Align::Start);
        frame.set_margin_start(400);
        frame.set_margin_top(200);
        overlay.add_overlay(&frame);
        // No main-loop turn between adding the box and focusing it — that is
        // how `open_editor` does it, and an unallocated child is exactly what
        // the scrolled window mishandles.
        focus_without_scrolling(&built.viewer.scroll, &entry);
        pump();

        // Asked of the root rather than of the entry: a `GtkEntry` hands the
        // cursor to the `GtkText` inside it, so the focus widget is a
        // descendant of the box and never the box itself. Asserted at all
        // because a `grab_focus` that quietly did nothing would leave the
        // scroll untouched too, and pass the assertion below for the wrong
        // reason.
        let focused =
            gtk::prelude::RootExt::focus(&built.window).expect("something took the cursor");
        assert!(
            focused.is_ancestor(&entry),
            "the cursor has to land inside the box"
        );
        assert_eq!(
            horizontal.value(),
            PARKED_AT,
            "opening a box must not scroll the canvas"
        );

        built.viewer.state.borrow_mut().session = None;
        built.window.close();
    }

    /// Pumps the main loop until `widget` has been allocated.
    fn settle(widget: &impl IsA<gtk::Widget>) {
        let context = glib::MainContext::default();
        for _ in 0..2_000 {
            while context.iteration(false) {}
            if widget.as_ref().width() > 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    /// Runs the loop dry, for the settling that is not about one widget's size.
    fn pump() {
        let context = glib::MainContext::default();
        for _ in 0..200 {
            while context.iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// A text run is a dozen pixels tall at a readable zoom and GTK will not
    /// shrink an `Entry` below the height of its own font. As a bare overlay
    /// child that overflow was harmless — the box just drew a little taller
    /// than the run. Inside a `Fixed` it is not: on a quarter turn the entry's
    /// height runs into negative frame x, which `GtkFixed` clamps away, and
    /// what the user gets is a sliver of a box with no text visible in it.
    ///
    /// So the frame has to be the entry's own size, turned — never the run's.
    #[gtk::test]
    fn gtk_ui_the_frame_holds_the_whole_entry_at_every_turn() {
        // A 210pt line at a 14pt size: far wider than it is tall, and far
        // shorter than an `Entry` can be drawn.
        let bbox = Rect {
            x: 72.0,
            y: 700.0,
            width: 210.0,
            height: 14.0,
        };

        for rotation in [
            PageRotation::None,
            PageRotation::Clockwise90,
            PageRotation::Clockwise180,
            PageRotation::Clockwise270,
        ] {
            let page = PagePlacement {
                scale: 0.72,
                ..turned(rotation)
            };
            let entry = Entry::new();
            entry.set_text("This file ships inside the app.");
            let frame = Fixed::new();
            frame.put(&entry, 0.0, 0.0);

            place_entry(&frame, &entry, bbox, page);

            let host = Overlay::new();
            host.set_child(Some(&gtk::DrawingArea::new()));
            host.add_overlay(&frame);
            let window = Window::new();
            window.set_default_size(1_200, 900);
            window.set_child(Some(&host));
            window.present();
            settle(&frame);

            let (minimum, natural) = entry.preferred_size();
            assert_eq!(
                (minimum.width(), minimum.height()),
                (natural.width(), natural.height()),
                "{rotation:?}: the entry must not be free to grow past the box \
                 the frame was measured for"
            );
            let (wanted_width, wanted_height) = match rotation {
                PageRotation::Clockwise90 | PageRotation::Clockwise270 => {
                    (minimum.height(), minimum.width())
                }
                _ => (minimum.width(), minimum.height()),
            };
            assert_eq!(
                (frame.width(), frame.height()),
                (wanted_width, wanted_height),
                "{rotation:?}: the frame is the entry's own box, turned"
            );

            window.destroy();
        }
    }
}
