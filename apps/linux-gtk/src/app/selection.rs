//! Text selection and highlighting: dragging on a page to select text,
//! painting that selection and the search matches over it, and copying the
//! selected text to the clipboard.
//!
//! The geometry is not here — `pdf_render::selection` owns the arithmetic so
//! every shell answers "which character is under this point" the same way.
//! What lives here is the toolkit half: the gesture, the paint, and the
//! asynchronous text load the two depend on.

use gtk::prelude::*;
use gtk::{cairo, gio, glib, DrawingArea, GestureDrag};
use pdf_render::{
    caret_range, line_rects, place_rect, point_to_pdf, DocumentHandle, PageCharacters,
    PdfiumRenderer, PlacedRect, Priority, RenderError, TextRect, TextRun,
};

use super::state::{Selection, Viewer};

/// Selection fill. Alpha rather than an opaque box because the glyphs have to
/// stay legible underneath it.
const SELECTION_RGBA: (f64, f64, f64, f64) = (0.20, 0.45, 0.90, 0.35);
/// Every search match except the one the user is standing on.
const MATCH_RGBA: (f64, f64, f64, f64) = (0.95, 0.80, 0.20, 0.40);
/// The current match, distinct so Next/Previous visibly moves something.
const CURRENT_MATCH_RGBA: (f64, f64, f64, f64) = (0.95, 0.55, 0.10, 0.60);

/// Builds the transparent layer that paints one page's highlights and
/// receives its drag gestures.
pub(crate) fn build_highlight_layer(viewer: &Viewer, page_index: usize) -> DrawingArea {
    let area = DrawingArea::new();
    area.set_draw_func({
        let viewer = viewer.clone();
        move |_, context, _, _| draw_highlights(&viewer, page_index, context)
    });

    let drag = GestureDrag::new();
    drag.connect_drag_begin({
        let viewer = viewer.clone();
        move |_, x, y| begin_selection(&viewer, page_index, x, y)
    });
    drag.connect_drag_update({
        let viewer = viewer.clone();
        move |gesture, offset_x, offset_y| {
            // `start_point` is in the widget's coordinates and the offsets are
            // relative to it, so the current pointer position is their sum.
            let Some((start_x, start_y)) = gesture.start_point() else {
                return;
            };
            extend_selection(&viewer, start_x + offset_x, start_y + offset_y);
        }
    });
    area.add_controller(drag);
    area
}

/// Paints one page's search matches and selection, in that order, so a
/// selection over a match stays visible.
fn draw_highlights(viewer: &Viewer, page_index: usize, context: &cairo::Context) {
    // Draw funcs run from the frame clock, never re-entrantly from inside a
    // handler that already holds the state — a plain borrow is safe here.
    let state = viewer.state.borrow();
    let Some(session) = state.session.as_ref() else {
        return;
    };
    let Some(page) = session.pages.get(page_index) else {
        return;
    };
    let scale = page.budget.factor;

    if let Some(search) = session.search.as_ref() {
        for (index, found) in search.matches.iter().enumerate() {
            if found.page_index as usize != page_index {
                continue;
            }
            let color = if index == search.current {
                CURRENT_MATCH_RGBA
            } else {
                MATCH_RGBA
            };
            fill_all(
                context,
                &line_rects(&found.character_bounds),
                page.height_pt,
                scale,
                color,
            );
        }
    }

    // A selection whose page has no text loaded yet paints nothing; the load
    // it kicked off calls `redraw` when it lands.
    let Some(selection) = session
        .selection
        .as_ref()
        .filter(|selection| selection.page_index == page_index)
    else {
        return;
    };
    let Some(characters) = page.characters.as_ref() else {
        return;
    };
    let Some(range) = resolve_range(characters, selection) else {
        return;
    };
    fill_all(
        context,
        &characters.rects_in(range),
        page.height_pt,
        scale,
        SELECTION_RGBA,
    );
}

/// Resolves a selection's two PDF-space points into a caret range.
fn resolve_range(
    characters: &PageCharacters,
    selection: &Selection,
) -> Option<std::ops::Range<usize>> {
    let anchor = characters.caret_at(selection.anchor.0, selection.anchor.1)?;
    let focus = characters.caret_at(selection.focus.0, selection.focus.1)?;
    Some(caret_range(anchor, focus))
}

fn fill_all(
    context: &cairo::Context,
    rects: &[TextRect],
    page_height_pt: f32,
    scale: f64,
    color: (f64, f64, f64, f64),
) {
    let (red, green, blue, alpha) = color;
    context.set_source_rgba(red, green, blue, alpha);
    for rect in rects {
        let PlacedRect {
            left,
            top,
            width,
            height,
        } = place_rect(*rect, page_height_pt, scale);
        context.rectangle(left, top, width, height);
    }
    // One fill for the whole batch: overlapping rects in a single path merge
    // instead of compounding their alpha into darker seams at the joins.
    let _ = context.fill();
}

fn begin_selection(viewer: &Viewer, page_index: usize, x: f64, y: f64) {
    // A document that withholds extraction gets no selection at all: loading
    // its text runs is itself extraction, so the refusal belongs here rather
    // than only at the clipboard. Reporting it on the first drag also tells
    // the user why the pointer appears to do nothing.
    if let Some(refusal) = viewer.text_extraction_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    let needs_text = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(page) = session.pages.get(page_index) else {
            return;
        };
        let point = point_to_pdf(x, y, page.height_pt, page.budget.factor);
        let needs_text = page.characters.is_none() && !page.characters_requested;
        session.selection = Some(Selection {
            page_index,
            anchor: point,
            focus: point,
        });
        needs_text
    };
    if needs_text {
        load_page_text(viewer, page_index);
    }
    redraw(viewer);
}

fn extend_selection(viewer: &Viewer, x: f64, y: f64) {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(page_index) = session
            .selection
            .as_ref()
            .map(|selection| selection.page_index)
        else {
            return;
        };
        let Some(page) = session.pages.get(page_index) else {
            return;
        };
        let point = point_to_pdf(x, y, page.height_pt, page.budget.factor);
        if let Some(selection) = session.selection.as_mut() {
            selection.focus = point;
        }
    }
    redraw(viewer);
}

/// Loads one page's text runs off the GTK thread and flattens them into the
/// page slot.
fn load_page_text(viewer: &Viewer, page_index: usize) {
    let document = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(page) = session.pages.get_mut(page_index) else {
            return;
        };
        page.characters_requested = true;
        session.document
    };

    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = gio::spawn_blocking(move || {
                PdfiumRenderer::new()
                    .text_runs(document, page_index as u32, Priority::Visible)
                    .wait()
            })
            .await
            .expect("text-run task panicked");
            apply_page_text(&viewer, document, page_index, result);
        }
    });
}

fn apply_page_text(
    viewer: &Viewer,
    document: DocumentHandle,
    page_index: usize,
    result: Result<Vec<TextRun>, RenderError>,
) {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        // The document was replaced while the load ran: these runs describe a
        // page that is no longer on screen.
        if session.document != document {
            return;
        }
        let Some(page) = session.pages.get_mut(page_index) else {
            return;
        };
        match result {
            Ok(runs) => page.characters = Some(PageCharacters::from_runs(&runs)),
            // Leave `characters_requested` set: a page whose text pdfium
            // refuses once will refuse again, and retrying on every motion
            // event of an ongoing drag would flood the actor.
            Err(_) => return,
        }
    }
    redraw(viewer);
}

/// Copies the selected text, reporting through the status label either way.
pub(crate) fn copy_selection(viewer: &Viewer) {
    // Checked again even though `begin_selection` already refuses, so there
    // should be nothing selected to copy. This is the call that actually hands
    // the document's text to the user, and the one place the permission most
    // needs to hold; it must not depend on another function having held the
    // line earlier.
    if let Some(refusal) = viewer.text_extraction_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    let text = {
        let state = viewer.state.borrow();
        let selected = state.session.as_ref().and_then(|session| {
            let selection = session.selection.as_ref()?;
            let page = session.pages.get(selection.page_index)?;
            let characters = page.characters.as_ref()?;
            Some(characters.text_in(resolve_range(characters, selection)?))
        });
        selected.unwrap_or_default()
    };

    if text.is_empty() {
        viewer.status.set_text("Select some text before copying.");
        return;
    }
    let count = text.chars().count();
    viewer.scroll.clipboard().set_text(&text);
    viewer.status.set_text(&format!(
        "Copied {count} character{} to the clipboard.",
        if count == 1 { "" } else { "s" }
    ));
}

/// Requests a repaint of every page's highlight layer.
///
/// All pages, not just the visible ones: `queue_draw` on an off-screen widget
/// costs a dirty flag, whereas working out which pages are on screen would
/// duplicate `layout::visible_range` for no gain.
pub(crate) fn redraw(viewer: &Viewer) {
    let state = viewer.state.borrow();
    let Some(session) = state.session.as_ref() else {
        return;
    };
    for page in &session.pages {
        page.highlights.queue_draw();
    }
}

/// Puts a page's highlight layer back on top of its overlay stack.
///
/// `Overlay` paints its children in the order they were added, and the tile
/// pipeline adds opaque bitmaps as the user zooms in. Without this, every
/// highlight on a tiled page would be painted and then covered by the tiles
/// that arrived after it.
pub(crate) fn raise_highlights(overlay: &gtk::Overlay, highlights: &DrawingArea) {
    overlay.remove_overlay(highlights);
    overlay.add_overlay(highlights);
}

/// Wires Ctrl+C to the selection copy, as a window action with an
/// application accelerator.
///
/// Not an `EventControllerKey` on the window: that runs in the bubble phase,
/// so the focused widget gets the key first and the search `Entry` — which
/// has its own Ctrl+C — swallows it before the window ever sees it. An
/// accelerator is resolved by the window's shortcut manager instead of by the
/// focus chain, which is also the extension point T-052 hangs the rest of the
/// standard shortcuts off.
pub(crate) fn connect_copy(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    let copy = gio::SimpleAction::new("copy", None);
    copy.connect_activate({
        let viewer = viewer.clone();
        move |_, _| copy_selection(&viewer)
    });
    window.add_action(&copy);
    application.set_accels_for_action("win.copy", &["<Control>c"]);
}
