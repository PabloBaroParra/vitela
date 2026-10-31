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
use pdf_document::{Command, Rect, TextRun};

use crate::app::selection;
use crate::app::state::{DocumentSession, DragPreview, PageSlot, TextDrag, Viewer};

use super::command::{self, text_move_refusal};
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
    if editor::keep_refused_editor(viewer) {
        return false;
    }

    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    let Some(base) = session.save_backing.as_ref().map(|backing| &backing.base) else {
        return false;
    };
    let Some(document) = session.document_model.as_ref() else {
        return false;
    };
    let pending = Some(&document.pending_edits);
    // See `super::content_page`.
    let Some(page_id) = super::content_page(session, page_index) else {
        return false;
    };
    // Before the mutable `pages` borrow below: the probe reads other fields
    // of the same session.
    let Some(probe) = super::page_probe(document, base, &session.imported_sources, page_id) else {
        return false;
    };
    let Some(page) = session.pages.get_mut(page_index) else {
        return false;
    };
    let hit = match model::ensure_page_content(&mut page.content, probe, page_id, pending) {
        // A run that is part of a block (B25 T-207) carries the whole block:
        // the reader grabbed a phrase, not the fragment under the pointer.
        Ok(content) => model::text_run_at(content, (point.0 as f32, point.1 as f32))
            .cloned()
            .map(|run| {
                let block = model::block_runs(content, &run);
                (model::block_stand_in(run, &block), block)
            }),
        Err(error) => {
            drop(state);
            viewer.status.set_text(&error.to_string());
            return false;
        }
    };
    let Some((run, block)) = hit else {
        return false;
    };

    // Resolved once, at press, rather than on every motion event: what makes
    // a run unmovable is a *pending command*, and the log cannot change
    // while the pointer is down. A block's runs are the core's to judge when
    // the batch is recorded.
    let refusal = if block.is_empty() {
        session
            .document_model
            .as_ref()
            .and_then(|document| text_move_refusal(document, &run))
    } else {
        None
    };

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
        block,
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
        page.placement(),
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
        record_move(session, &drag, to)
    };

    match outcome {
        Ok(()) => crate::app::write::refresh_preview(viewer, "Text moved."),
        Err(message) => {
            viewer.status.set_text(&message);
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

/// Records `drag`'s run, or its whole block, moved to `to`, through the
/// core's queue policy (`pdf_save::record`).
///
/// The core decides where the move goes: a run already dragged this session
/// amends its queued move, keeping the snapshot the file holds; a pending
/// insertion is amended to its new spot rather than moved; a run with a
/// pending retype is refused. A block is one batch, and moved again it stays
/// one undo step.
fn record_move(session: &mut DocumentSession, drag: &TextDrag, to: Rect) -> Result<(), String> {
    if drag.block.is_empty() {
        return command::record(
            session,
            Command::MoveTextRun {
                item: drag.run.clone(),
                to,
            },
        );
    }
    let (dx, dy) = (to.x - drag.run.bbox.x, to.y - drag.run.bbox.y);
    command::record_batch(session, pdf_edit::move_block(&drag.block, dx, dy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{ContentItemId, FontKind, PageId};

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
            block: Vec::new(),
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
    // --- record_move: through the core's queue policy ------------------

    fn line_session(lines: &[&str]) -> (DocumentSession, Vec<TextRun>) {
        let base = gen_fixtures::build_multi_line_page_document(lines);
        let runs = pdf_edit::read_page_content(&base, PageId(0))
            .expect("the page parses")
            .text_runs;
        (crate::app::test_fixtures::backed_session(base), runs)
    }

    fn drag_of(run: TextRun, block: Vec<TextRun>, by: (f64, f64)) -> TextDrag {
        TextDrag {
            page_index: 0,
            preview: None,
            run,
            refusal: None,
            block,
            origin: (0.0, 0.0),
            current: by,
        }
    }

    fn entries(session: &DocumentSession) -> &[Command] {
        session
            .document_model
            .as_ref()
            .expect("a model")
            .pending_edits
            .entries()
    }

    /// The regression the old shell-side planner existed for, now held by
    /// the core: after a move the run is hit-tested at its new box, and a
    /// second drag from there must amend the queued move, keeping the box
    /// the file still holds, instead of naming a box the file never had.
    #[test]
    fn a_run_moved_twice_is_one_queued_move_from_its_original_box() {
        let (mut session, runs) = line_session(&["Hello world"]);
        let run = runs.first().cloned().expect("the line parses as a run");

        let first = drag_of(run.clone(), Vec::new(), (100.0, -50.0));
        record_move(&mut session, &first, dragged_origin(&first)).expect("the first move");

        let moved = TextRun {
            bbox: dragged_origin(&first),
            ..run.clone()
        };
        let second = drag_of(moved, Vec::new(), (-80.0, -150.0));
        let to = dragged_origin(&second);
        record_move(&mut session, &second, to).expect("the second move amends the first");

        assert_eq!(entries(&session), [Command::MoveTextRun { item: run, to }]);
    }

    /// A block's move is one entry, and moving it again folds into that
    /// entry rather than adding a second.
    #[test]
    fn a_block_moved_twice_is_one_batch() {
        let (mut session, runs) = line_session(&["Hello world", "Second line"]);
        let block = runs;
        let stand_in = model::block_stand_in(block[0].clone(), &block);

        let first = drag_of(stand_in.clone(), block.clone(), (30.0, -20.0));
        record_move(&mut session, &first, dragged_origin(&first)).expect("the block moves");

        let moved: Vec<TextRun> = block
            .iter()
            .map(|run| TextRun {
                bbox: Rect {
                    x: run.bbox.x + 30.0,
                    y: run.bbox.y - 20.0,
                    ..run.bbox
                },
                ..run.clone()
            })
            .collect();
        let second = drag_of(
            model::block_stand_in(moved[0].clone(), &moved),
            moved,
            (10.0, 10.0),
        );
        record_move(&mut session, &second, dragged_origin(&second))
            .expect("the second move folds into the batch");

        assert!(
            matches!(entries(&session), [Command::Batch(children)] if children.len() == block.len())
        );
    }
}
