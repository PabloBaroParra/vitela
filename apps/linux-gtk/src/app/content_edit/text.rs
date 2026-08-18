//! Dragging an existing text run across the page, and the single door every
//! reposition goes through on its way to the `EditLog`.
//!
//! The text twin of [`super::image`]'s drag lifecycle, and deliberately the
//! same shape: a press that lands on a run is claimed *at press time*, the
//! outline follows the pointer live, and the release records the move. That
//! symmetry is the point — in content-edit mode a page item is grabbed and
//! moved, whether it is a picture or a line of text.
//!
//! The one thing this gesture cannot reach is a box that is still being
//! composed — a blank insertion sits in a widget, not on the page, so there
//! is nothing under the pointer to hit-test. `super::editor` drags that one
//! itself, and records nothing when it does: a run that exists nowhere yet
//! is not moved, it is simply described somewhere else.

use gtk::gdk;
use gtk::prelude::*;
use gtk::{cairo, Picture};
use pdf_document::{Command, Document, Rect, TextRun};

use crate::app::selection;
use crate::app::state::{DocumentSession, DragPreview, PageSlot, TextDrag, Viewer};

use super::command::{
    amend_command, amended_command, apply_command, moved_text_command, pending_move_index,
    pending_text_command, text_move_refusal, validate_insert_text, validate_move_text, PendingText,
};
use super::{editor, model, CLICK_EPSILON_PX};

/// Claims a press that lands on a movable text run, so the drag that may
/// follow moves it.
///
/// Returns whether a run took the press. `false` leaves the gesture to
/// whatever comes next — which, on release, is the click that opens the
/// inline editor.
///
/// Two presses are deliberately not claimed. One that lands while an insert
/// kind is armed belongs to the insertion (a click there is always about
/// creating something new — see `handle_drag_end`'s own note), and one that
/// finds no run under it has nothing to move.
pub(crate) fn begin_text_drag(viewer: &Viewer, page_index: usize, point: (f64, f64)) -> bool {
    if viewer.content_edit_refusal().is_some() {
        // Reported by the paths that actually try to record something; a
        // press that will never reach the log stays silent.
        return false;
    }
    if viewer.state.borrow().content_insert_mode.is_some() {
        return false;
    }

    // Resolves whatever editor is open first, mirroring
    // `image::begin_image_drag`'s own first line and for the same reason: a
    // press elsewhere on the page must not leave a half-typed edit stranded,
    // and it is claimed here, before any focus-out on the `Entry` is
    // guaranteed to have run.
    editor::commit(viewer);

    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    let Some(base) = session
        .save_backing
        .as_ref()
        .map(|backing| backing.base.as_lopdf())
    else {
        return false;
    };
    let pending = session
        .document_model
        .as_ref()
        .map(|document| &document.pending_edits);
    let Some(page) = session.pages.get_mut(page_index) else {
        return false;
    };
    let hit = match model::ensure_page_content(&mut page.content, base, page_index, pending) {
        Ok(content) => model::text_run_at(content, (point.0 as f32, point.1 as f32)).cloned(),
        Err(error) => {
            drop(state);
            viewer.status.set_text(&error.to_string());
            return false;
        }
    };
    let Some(run) = hit else {
        return false;
    };

    // Resolved once, at press, rather than on every motion event: what makes
    // a run unmovable is a *pending command*, and the log cannot change
    // while the pointer is down.
    let refusal = session
        .document_model
        .as_ref()
        .and_then(|document| text_move_refusal(document, &run));

    // Likewise captured once: the page's pixels do not change while the
    // pointer is down, and downloading them per motion event would be
    // megabytes of memcpy per frame.
    let preview = session
        .pages
        .get(page_index)
        .and_then(|page| capture_preview(page, &run));

    session.text_drag = Some(TextDrag {
        page_index,
        preview,
        run,
        refusal,
        origin: point,
        current: point,
    });
    true
}

/// Reads the page's rendered bitmap back out of its `Picture`, so the drag
/// can carry the run's actual pixels around instead of an empty rectangle.
///
/// `None` whenever the pixels cannot be had — the page has not rendered yet,
/// the paintable is not a texture, or the buffer will not wrap as a cairo
/// surface. Every one of those costs the *preview* and nothing else: the
/// outline still follows the pointer and the move still records, which is
/// exactly the feedback an image drag gives.
fn capture_preview(page: &PageSlot, run: &TextRun) -> Option<DragPreview> {
    let surface = page_surface(&page.picture)?;
    // The bitmap is rendered at the page's own DPI while the outline is
    // computed in widget units, so everything cut out of it has to be
    // scaled by the ratio between the two.
    let widget_width = f64::from(page.width_pt) * page.budget.factor;
    if widget_width <= 0.0 {
        return None;
    }
    let scale = f64::from(surface.width()) / widget_width;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }

    // Placed from the *bled* box, the one the preview will actually cover,
    // so the samples below land outside the patch rather than on the ink
    // inside it.
    let covered = bled_rect(run.bbox);
    let placed = pdf_render::place_rect(
        pdf_render::TextRect {
            x_pt: covered.x as f32,
            y_pt: covered.y as f32,
            width_pt: covered.width as f32,
            height_pt: covered.height as f32,
        },
        page.height_pt,
        page.budget.factor,
    );
    let mut surface = surface;
    let background = sampled_background(&mut surface, &placed, scale);

    Some(DragPreview {
        page: surface,
        scale,
        background,
    })
}

/// The page bitmap as a cairo surface.
///
/// `Texture::download` writes premultiplied BGRA, which is byte-for-byte
/// what cairo calls `ARgb32` on a little-endian machine — the same
/// correspondence GTK itself relies on, so no channel shuffling is needed.
fn page_surface(picture: &Picture) -> Option<cairo::ImageSurface> {
    let texture = picture.paintable()?.downcast::<gdk::Texture>().ok()?;
    let (width, height) = (texture.width(), texture.height());
    let stride = cairo::Format::ARgb32.stride_for_width(width as u32).ok()?;
    let mut data = vec![0u8; stride as usize * height as usize];
    texture.download(&mut data, stride as usize);
    cairo::ImageSurface::create_for_data(data, cairo::Format::ARgb32, width, height, stride).ok()
}

/// The colour to paint over the space a dragged run leaves behind, sampled
/// from just outside its box.
///
/// Four points rather than one, averaged: a single sample that happens to
/// land on a rule, a border or a neighbouring glyph would fill the gap with
/// that instead of with the page. Falls back to white when the surface
/// cannot be read, which is what a PDF page is unless it says otherwise.
fn sampled_background(
    surface: &mut cairo::ImageSurface,
    placed: &pdf_render::PlacedRect,
    scale: f64,
) -> (f64, f64, f64) {
    const WHITE: (f64, f64, f64) = (1.0, 1.0, 1.0);
    const MARGIN: f64 = 3.0;

    let (width, height) = (surface.width(), surface.height());
    let stride = surface.stride() as usize;
    let Ok(data) = surface.data() else {
        return WHITE;
    };

    let left = (placed.left - MARGIN) * scale;
    let right = (placed.left + placed.width + MARGIN) * scale;
    let top = (placed.top - MARGIN) * scale;
    let bottom = (placed.top + placed.height + MARGIN) * scale;

    let mut total = (0.0, 0.0, 0.0);
    let mut taken = 0.0;
    for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        let (x, y) = (x.round() as i32, y.round() as i32);
        if x < 0 || y < 0 || x >= width || y >= height {
            continue;
        }
        let offset = y as usize * stride + x as usize * 4;
        let Some(pixel) = data.get(offset..offset + 4) else {
            continue;
        };
        // Premultiplied BGRA. A fully transparent sample carries no colour
        // to speak of, so it is passed over rather than counted as black.
        let alpha = f64::from(pixel[3]) / 255.0;
        if alpha <= 0.0 {
            continue;
        }
        total.0 += f64::from(pixel[2]) / 255.0 / alpha;
        total.1 += f64::from(pixel[1]) / 255.0 / alpha;
        total.2 += f64::from(pixel[0]) / 255.0 / alpha;
        taken += 1.0;
    }

    if taken == 0.0 {
        return WHITE;
    }
    (total.0 / taken, total.1 / taken, total.2 / taken)
}

/// Extends the text drag in flight, if any. Returns whether one existed.
pub(crate) fn extend_text_drag(viewer: &Viewer, point: (f64, f64)) -> bool {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(drag) = state
            .session
            .as_mut()
            .and_then(|session| session.text_drag.as_mut())
        else {
            return false;
        };
        drag.current = point;
    }
    selection::redraw(viewer);
    true
}

/// Resolves the text drag in flight, if any.
///
/// `offset_x`/`offset_y` are the gesture's own device-pixel offsets, which is
/// what decides whether this was a move at all: below [`CLICK_EPSILON_PX`]
/// the press was a click, and saying so (`false`) is what lets the click go
/// on to open the inline editor. Screen space rather than page space on
/// purpose — the threshold is about how steady the user's hand was, not
/// about how far the page thinks that is at the current zoom.
///
/// Returns whether the gesture was consumed here.
pub(crate) fn finish_text_drag(viewer: &Viewer, offset_x: f64, offset_y: f64) -> bool {
    let drag = {
        let mut state = viewer.state.borrow_mut();
        match state
            .session
            .as_mut()
            .and_then(|session| session.text_drag.take())
        {
            Some(drag) => drag,
            None => return false,
        }
    };

    if offset_x.abs() < CLICK_EPSILON_PX && offset_y.abs() < CLICK_EPSILON_PX {
        selection::redraw(viewer);
        return false;
    }

    // Held back until the drag proves to be one: reporting at press time
    // would put the message up for every click on the run, including the
    // ones that only wanted to open the editor.
    if let Some(refusal) = drag.refusal {
        viewer.status.set_text(refusal);
        selection::redraw(viewer);
        return true;
    }

    let to = dragged_origin(&drag);
    let outcome = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return true;
        };
        record_move(session, drag.page_index, &drag.run, to)
    };

    match outcome {
        MoveRecord::Recorded => {
            crate::app::document::refresh_after_content_edit(viewer, "Text moved.")
        }
        MoveRecord::Refused(message) => {
            viewer.status.set_text(&message);
            selection::redraw(viewer);
        }
        MoveRecord::Lost => {
            viewer
                .status
                .set_text("That edit is no longer available — nothing was recorded.");
            selection::redraw(viewer);
        }
    }
    true
}

/// Where the run's box has been dragged to, in PDF page space. Only the
/// origin moves: a run's width and height are the font's to decide.
pub(crate) fn dragged_origin(drag: &TextDrag) -> Rect {
    Rect {
        x: drag.run.bbox.x + (drag.current.0 - drag.origin.0),
        y: drag.run.bbox.y + (drag.current.1 - drag.origin.1),
        ..drag.run.bbox
    }
}

/// A glyph's ink reaches past the box the parser measured — ascenders and
/// descenders both — so the pixels a drag carries are cut with a margin.
/// Too tight a cut shaves the tops off the very letters being moved.
const PREVIEW_BLEED_PT: f64 = 2.0;

/// The box a live preview cuts from, and the box it draws to: the run's own
/// rect grown by [`PREVIEW_BLEED_PT`] on every side, before and after the
/// drag's displacement.
///
/// Both come out of one expression on purpose. They were once grown
/// separately, and the two grew differently — the destination gained the
/// margin on its origin but not on its height, and since `place_rect` reads
/// a top edge as `page_height - (y + height)`, that put every dragged run
/// two bleeds *below* where the pointer had it, and made it jump back on
/// release. Two places computing "the same rect" is the bug; one place
/// cannot disagree with itself.
pub(crate) fn preview_rects(drag: &TextDrag) -> (Rect, Rect) {
    (bled_rect(drag.run.bbox), bled_rect(dragged_origin(drag)))
}

/// `rect` grown by [`PREVIEW_BLEED_PT`] on every side.
fn bled_rect(rect: Rect) -> Rect {
    Rect {
        x: rect.x - PREVIEW_BLEED_PT,
        y: rect.y - PREVIEW_BLEED_PT,
        width: rect.width + 2.0 * PREVIEW_BLEED_PT,
        height: rect.height + 2.0 * PREVIEW_BLEED_PT,
    }
}

/// What [`record_move`] did.
#[derive(Debug, PartialEq)]
enum MoveRecord {
    Recorded,
    /// `pdf-edit` will not move this run — the message is its own.
    Refused(String),
    /// The entry this move had to amend is no longer in the log. Nothing was
    /// recorded, and that has to be said out loud rather than swallowed.
    Lost,
}

/// Validates `run`'s reposition to `to` and records it in `session`'s log.
///
/// Validation runs against the command [`plan_move`] chose, not against the
/// gesture that asked — an insertion amended to a new spot is checked as an
/// insertion, because that is what will be replayed.
///
/// `session.unsaved_to_disk` is set here, at the moment the command joins
/// the log, rather than by the refresh that follows — a refresh that fails
/// still leaves a recorded edit behind, and a document that reports itself
/// clean is one the open-another-document guard discards without asking.
fn record_move(
    session: &mut DocumentSession,
    page_index: usize,
    run: &TextRun,
    to: Rect,
) -> MoveRecord {
    // Planned and validated first, against borrows that all end here: the
    // recording below needs the session mutably.
    let plan = {
        let Some(base) = session
            .save_backing
            .as_ref()
            .map(|backing| backing.base.as_lopdf())
        else {
            return MoveRecord::Lost;
        };
        let Some(document) = session.document_model.as_ref() else {
            return MoveRecord::Lost;
        };
        let plan = match plan_move(document, run, to) {
            Ok(plan) => plan,
            Err(record) => return record,
        };
        if let Err(error) = validate(base, page_index, plan.command()) {
            return MoveRecord::Refused(error.to_string());
        }
        plan
    };

    let Some(document) = session.document_model.as_mut() else {
        return MoveRecord::Lost;
    };
    let recorded = match plan {
        Plan::Amend(index, command) => amend_command(document, index, command),
        Plan::Append(command) => {
            apply_command(document, command);
            true
        }
    };

    if !recorded {
        return MoveRecord::Lost;
    }
    session.edit_revision += 1;
    session.unsaved_to_disk = true;
    MoveRecord::Recorded
}

/// Runs the command about to be recorded against the real `pdf-edit` call,
/// on a throwaway clone — the same probe-before-record contract every other
/// content edit in this shell uses (`command::validate_replacement`).
fn validate(
    base: &lopdf::Document,
    page_index: usize,
    command: &Command,
) -> Result<(), pdf_edit::EditError> {
    match command {
        Command::MoveTextRun { item, to } => validate_move_text(base, page_index, item, *to),
        Command::InsertTextRun(run) => validate_insert_text(base, page_index, run),
        // `plan_move` produces no other shape.
        _ => Ok(()),
    }
}

/// Where a reposition goes in the log.
#[derive(Debug, PartialEq)]
enum Plan {
    /// Fold it into the entry at this position.
    Amend(usize, Command),
    /// Add it as a new entry.
    Append(Command),
}

impl Plan {
    fn command(&self) -> &Command {
        match self {
            Plan::Amend(_, command) | Plan::Append(command) => command,
        }
    }
}

/// Decides which log entry `run`'s reposition to `to` belongs in.
///
/// Pure over the document's pending log — no I/O, no validation, no session
/// — because the decision is the part that is easy to get subtly wrong, and
/// it is worth being able to state each rule as a test.
///
/// **Three shapes, and they are not variations of one another:**
///
/// - A run that exists **in the file** and has never been dragged becomes a
///   new `Command::MoveTextRun`.
/// - A run **already dragged this session** amends the entry it has, rather
///   than queueing another. The recorded entry still describes the run as
///   the file holds it, so only the destination is stale — while a second
///   entry would name a box the first already vacated, and resolve against
///   nothing at save time.
/// - A run on the page **only because of a pending insertion** is not moved
///   at all: its `InsertTextRun` is amended to describe a different spot.
///   A `MoveTextRun` against it would name a run no base document contains,
///   and since a resolution failure aborts the whole save, that one entry
///   would take every other queued edit down with it. Same reasoning
///   `command::pending_text_command` applies to retyping, reaching the same
///   answer for the same reason.
///
/// A pending *replacement* is refused rather than planned — see
/// [`text_move_refusal`], which turns that press away before a drag starts.
/// Reaching it here means one was recorded behind this gesture's back.
fn plan_move(document: &Document, run: &TextRun, to: Rect) -> Result<Plan, MoveRecord> {
    let index = match pending_text_command(document, run) {
        PendingText::Nothing => {
            return Ok(match pending_move_index(document, run) {
                Some(index) => {
                    let amended = document
                        .pending_edits
                        .entries()
                        .get(index)
                        .and_then(|existing| moved_text_command(existing, to));
                    match amended {
                        Some(amended) => Plan::Amend(index, amended),
                        None => return Err(MoveRecord::Lost),
                    }
                }
                None => Plan::Append(Command::MoveTextRun {
                    item: run.clone(),
                    to,
                }),
            })
        }
        PendingText::Unresolvable => return Err(MoveRecord::Lost),
        PendingText::Amend(index) => index,
    };

    let Some(existing @ Command::InsertTextRun(_)) = document.pending_edits.entries().get(index)
    else {
        return Err(MoveRecord::Refused(
            "This text has an unsaved edit — save the document before moving it.".to_string(),
        ));
    };
    match amended_command(existing, &run.text, Some(to)) {
        Some(amended) => Ok(Plan::Amend(index, amended)),
        None => Err(MoveRecord::Lost),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{ContentItemId, EditLog, FontKind, PageId};

    fn a_run() -> TextRun {
        TextRun {
            id: ContentItemId(1),
            page: PageId(0),
            bbox: Rect {
                x: 100.0,
                y: 500.0,
                width: 80.0,
                height: 12.0,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "Hello".to_string(),
        }
    }

    fn a_drag(current: (f64, f64)) -> TextDrag {
        TextDrag {
            page_index: 0,
            preview: None,
            run: a_run(),
            refusal: None,
            origin: (120.0, 505.0),
            current,
        }
    }

    #[test]
    fn a_drag_translates_the_box_by_the_pointer_delta() {
        let moved = dragged_origin(&a_drag((150.0, 480.0)));

        assert_eq!((moved.x, moved.y), (130.0, 475.0));
    }

    /// The run keeps the size the font gave it however far it is dragged —
    /// a move is not a resize, and a text run has no resize at all.
    #[test]
    fn a_drag_never_changes_the_size_of_the_box() {
        let moved = dragged_origin(&a_drag((400.0, 100.0)));

        assert_eq!((moved.width, moved.height), (80.0, 12.0));
    }

    /// The preview's two boxes must be the same box until the pointer moves.
    /// When they were grown by two separate expressions they differed even
    /// at rest, which drew every dragged run below the pointer and made it
    /// pop back into place on release.
    #[test]
    fn a_preview_reads_and_draws_the_same_box_until_the_pointer_moves() {
        let (source, destination) = preview_rects(&a_drag((120.0, 505.0)));

        assert_eq!(source, destination);
    }

    /// And once it does move, the two differ by the pointer's delta and by
    /// nothing else — same size, so the patch cannot stretch or slip.
    #[test]
    fn a_preview_offsets_its_destination_by_exactly_the_pointer_delta() {
        let (source, destination) = preview_rects(&a_drag((150.0, 480.0)));

        assert_eq!(
            (destination.x - source.x, destination.y - source.y),
            (30.0, -25.0)
        );
        assert_eq!(
            (destination.width, destination.height),
            (source.width, source.height)
        );
    }

    #[test]
    fn a_drag_that_never_moved_leaves_the_box_where_it_was() {
        let moved = dragged_origin(&a_drag((120.0, 505.0)));

        assert_eq!((moved.x, moved.y), (100.0, 500.0));
    }
    // --- plan_move: which entry a reposition belongs in -------------------

    fn a_document() -> Document {
        Document::blank()
    }

    fn record(document: &mut Document, command: Command) {
        let mut log = std::mem::take(&mut document.pending_edits);
        log.apply(document, command);
        document.pending_edits = log;
    }

    fn a_rect(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 80.0,
            height: 12.0,
        }
    }

    #[test]
    fn a_run_with_nothing_queued_gets_a_new_move() {
        let document = a_document();

        let plan = plan_move(&document, &a_run(), a_rect(300.0, 200.0)).expect("planned");

        assert_eq!(
            plan,
            Plan::Append(Command::MoveTextRun {
                item: a_run(),
                to: a_rect(300.0, 200.0),
            })
        );
    }

    /// The regression this whole plan exists for: after a move is recorded
    /// the run is hit-tested at its *new* box, and planning from that box
    /// must still amend the entry the run already has. Appending instead
    /// would record a command naming a box the base document no longer
    /// matches, which `pdf-edit` refuses with `ItemNotFound`.
    #[test]
    fn a_run_dragged_a_second_time_amends_its_move_and_keeps_the_original_item() {
        let mut document = a_document();
        let first = a_rect(300.0, 200.0);
        record(
            &mut document,
            Command::MoveTextRun {
                item: a_run(),
                to: first,
            },
        );
        // What the shell hit-tests after the refresh: same run, at the box
        // the move gave it (`model::overlay_pending_content`).
        let as_hit_tested = TextRun {
            bbox: first,
            ..a_run()
        };
        let second = a_rect(120.0, 640.0);

        let plan = plan_move(&document, &as_hit_tested, second).expect("planned");

        assert_eq!(
            plan,
            Plan::Amend(
                0,
                Command::MoveTextRun {
                    item: a_run(),
                    to: second,
                }
            ),
            "the amended entry must keep the run as the file holds it"
        );
    }

    /// A run on the page only because of a pending insertion has no saved
    /// position to move from — the insertion itself is amended instead.
    #[test]
    fn dragging_a_pending_insertion_amends_the_insertion() {
        let mut document = a_document();
        let inserted = TextRun {
            id: ContentItemId(0),
            bbox: a_rect(50.0, 700.0),
            ..a_run()
        };
        record(&mut document, Command::InsertTextRun(inserted));
        // The synthetic id `overlay_pending_content` hands a pending
        // insertion, pointing back at log entry 0.
        let as_hit_tested = TextRun {
            id: ContentItemId(super::super::model::PENDING_ITEM_ID_BASE),
            bbox: a_rect(50.0, 700.0),
            ..a_run()
        };

        let plan = plan_move(&document, &as_hit_tested, a_rect(300.0, 400.0)).expect("planned");

        let Plan::Amend(0, Command::InsertTextRun(moved)) = plan else {
            panic!("a pending insertion is moved by amending it, got {plan:?}");
        };
        assert_eq!((moved.bbox.x, moved.bbox.y), (300.0, 400.0));
        assert_eq!(
            moved.id,
            ContentItemId(0),
            "the placeholder id the command carries is never replaced by the synthetic one"
        );
    }

    /// The case `text_move_refusal` turns away at press time. Reaching here
    /// means one was recorded behind the gesture's back, and silently
    /// dropping the move would be worse than saying so.
    #[test]
    fn a_run_with_a_pending_retype_is_refused_rather_than_planned() {
        let mut document = a_document();
        record(
            &mut document,
            Command::ReplaceTextRunContent {
                item: a_run(),
                after: "Adios".to_string(),
            },
        );

        let refusal = plan_move(&document, &a_run(), a_rect(300.0, 200.0)).expect_err("refused");

        assert!(matches!(refusal, MoveRecord::Refused(_)));
    }

    #[test]
    fn a_synthetic_id_with_nothing_behind_it_records_nothing() {
        let document = a_document();
        let stale = TextRun {
            id: ContentItemId(super::super::model::PENDING_ITEM_ID_BASE + 7),
            ..a_run()
        };

        let refusal = plan_move(&document, &stale, a_rect(300.0, 200.0)).expect_err("lost");

        assert_eq!(refusal, MoveRecord::Lost);
    }

    /// The regression, end to end against a real page: drag a run, then drag
    /// it again from where it now sits, and the command that gets recorded
    /// must still resolve against the file.
    ///
    /// The bug this locks out validated the *grabbed* run — which by then
    /// describes a box the base document has never contained — instead of
    /// the command about to be recorded, so the second drag was refused with
    /// `ItemNotFound` and the run could be moved exactly once.
    #[test]
    fn moving_a_run_a_second_time_validates_against_the_box_the_file_still_holds() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let mut cache = None;
        let run = model::ensure_page_content(&mut cache, &base, 0, None)
            .expect("page 0 parses")
            .text_runs
            .first()
            .cloned()
            .expect("the line parses as a run");

        let mut document = Document::blank();
        let first = Rect {
            x: run.bbox.x + 100.0,
            y: run.bbox.y - 50.0,
            ..run.bbox
        };
        record(
            &mut document,
            Command::MoveTextRun {
                item: run.clone(),
                to: first,
            },
        );

        // Exactly what the shell hit-tests after the refresh: the page
        // re-parsed from the untouched base, with the pending log layered on.
        let mut cache = None;
        let as_hit_tested =
            model::ensure_page_content(&mut cache, &base, 0, Some(&document.pending_edits))
                .expect("page 0 parses")
                .text_runs
                .iter()
                .find(|candidate| candidate.id == run.id)
                .cloned()
                .expect("the moved run is still on the page");
        assert_eq!(
            (as_hit_tested.bbox.x, as_hit_tested.bbox.y),
            (first.x, first.y),
            "the overlay must show it where the move put it"
        );

        let second = Rect {
            x: run.bbox.x + 20.0,
            y: run.bbox.y - 200.0,
            ..run.bbox
        };
        assert!(
            validate_move_text(&base, 0, &as_hit_tested, second).is_err(),
            "the grabbed run's own box is not one the file has ever held —              validating against it is precisely the bug"
        );

        let plan = plan_move(&document, &as_hit_tested, second).expect("planned");

        validate(&base, 0, plan.command())
            .expect("the planned command must resolve against the base document");
    }

    /// Guards the assumption the amend path rests on: `EditLog::amend` takes
    /// a move in place of a move.
    #[test]
    fn the_log_accepts_a_move_amending_a_move() {
        let mut document = a_document();
        record(
            &mut document,
            Command::MoveTextRun {
                item: a_run(),
                to: a_rect(300.0, 200.0),
            },
        );
        let mut log: EditLog = std::mem::take(&mut document.pending_edits);

        assert!(log.amend(
            0,
            Command::MoveTextRun {
                item: a_run(),
                to: a_rect(120.0, 640.0),
            }
        ));
    }
}
