//! Text selection and highlighting: dragging on a page to select text,
//! painting that selection and the search matches over it, and copying the
//! selected text to the clipboard.
//!
//! The geometry is not here — `pdf_render::selection` owns the arithmetic so
//! every shell answers "which character is under this point" the same way.
//! What lives here is the toolkit half: the gesture, the paint, and the
//! asynchronous text load the two depend on.

use gtk::prelude::*;
use gtk::{cairo, gdk, gio, glib, DrawingArea, GestureDrag};
use pdf_document::{Annotation, AnnotationKind, Color, FontKind, Rect};
use pdf_render::{
    caret_range, line_rects, place_rect, point_to_pdf, DocumentHandle, PageCharacters,
    PdfiumRenderer, PlacedRect, Priority, RenderError, TextRect, TextRun,
};

use super::annotations;
use super::content_edit;
use super::state::{DocumentSession, PageSlot, Selection, Viewer};

/// Selection fill. Alpha rather than an opaque box because the glyphs have to
/// stay legible underneath it.
const SELECTION_RGBA: (f64, f64, f64, f64) = (0.20, 0.45, 0.90, 0.35);
/// Every search match except the one the user is standing on.
const MATCH_RGBA: (f64, f64, f64, f64) = (0.95, 0.80, 0.20, 0.40);
/// The current match, distinct so Next/Previous visibly moves something.
const CURRENT_MATCH_RGBA: (f64, f64, f64, f64) = (0.95, 0.55, 0.10, 0.60);
/// An annotation the user is not standing on. Translucent for the same reason
/// the selection is: the page underneath has to stay readable.
const ANNOTATION_ALPHA: f64 = 0.45;
/// The selected annotation, distinct so the edit buttons visibly target it.
const SELECTED_ANNOTATION_ALPHA: f64 = 0.75;
/// Outline colour for annotation kinds this preview cannot draw faithfully
/// (text notes and image stamps) — deliberately not one of their real colours.
/// Alpha comes from the selection state, like every other annotation.
const PLACEHOLDER_ANNOTATION_RGB: (f64, f64, f64) = (0.20, 0.40, 0.80);
/// Thickness of an underline/strikeout rule, in device pixels. Deliberately
/// zoom-independent: a hairline that scales away is worse than one that stays
/// visible.
const RULE_THICKNESS: f64 = 2.0;
/// Side of a selection handle, in device pixels.
const HANDLE_PX: f64 = 8.0;
/// Handle fill. Solid, unlike the annotations, so a handle reads as chrome the
/// user can grab rather than as part of the mark.
const HANDLE_RGB: (f64, f64, f64) = (0.10, 0.35, 0.85);
/// Outline for a content-edit-mode text run that can be retyped.
const CONTENT_RUN_OUTLINE_RGBA: (f64, f64, f64, f64) = (0.20, 0.60, 0.30, 0.55);
/// Outline for a composite-font run: distinct colour *and* dashed, so the
/// difference reads even to a colour-blind user — T-161's "shown
/// distinguishable" requirement for runs `pdf-edit` will never accept an edit
/// against.
const COMPOSITE_RUN_OUTLINE_RGBA: (f64, f64, f64, f64) = (0.75, 0.20, 0.20, 0.55);
const COMPOSITE_RUN_DASH: [f64; 2] = [4.0, 3.0];
/// Outline for a page image in content-edit mode (T-162) — distinct from
/// the text-run outline colours so the two item kinds read as different
/// things.
const CONTENT_IMAGE_OUTLINE_RGBA: (f64, f64, f64, f64) = (0.55, 0.35, 0.85, 0.55);
/// The selected image's outline/handles, brighter than an unselected one —
/// same posture as `SELECTED_ANNOTATION_ALPHA` vs `ANNOTATION_ALPHA`.
const SELECTED_CONTENT_IMAGE_RGBA: (f64, f64, f64, f64) = (0.55, 0.35, 0.85, 0.85);

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
            extend_selection(&viewer, page_index, start_x + offset_x, start_y + offset_y);
        }
    });
    drag.connect_drag_end({
        let viewer = viewer.clone();
        // No-ops unless this drag was placing or reshaping an annotation; a
        // text selection is already complete by the time the button comes up.
        move |gesture, offset_x, offset_y| {
            if content_edit::mode_is_active(&viewer) {
                content_edit::handle_drag_end(&viewer, page_index, gesture, offset_x, offset_y);
                return;
            }
            annotations::finish_placement(&viewer);
            annotations::finish_annotation_drag(&viewer);
        }
    });
    area.add_controller(drag);
    super::input::connect_file_drop(&area, viewer, page_index);
    area
}

/// A pointer position on a drawn page, in PDF page space.
///
/// `None` when the page is not laid out yet — there is no page-space answer to
/// give, and guessing one would place an annotation somewhere arbitrary.
pub(crate) fn pointer_to_pdf(
    viewer: &Viewer,
    page_index: usize,
    x: f64,
    y: f64,
) -> Option<(f64, f64)> {
    let state = viewer.state.borrow();
    let page = state.session.as_ref()?.pages.get(page_index)?;
    let (x, y) = point_to_pdf(x, y, page.height_pt, page.budget.factor);
    Some((f64::from(x), f64::from(y)))
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

    if content_edit::mode_is_active(viewer) {
        draw_content_run_outlines(context, page, scale);
        draw_content_image_outlines(context, page, session, page_index, scale);
    }

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

    for annotation in session
        .document_model
        .iter()
        .flat_map(|document| document.annotations.iter())
        .filter(|annotation| annotation.page.0 as usize == page_index)
    {
        let selected = session.selected_annotation == Some(annotation.id);
        // While an annotation is being dragged, paint where it is heading
        // rather than where it still sits in the model — nothing reaches the
        // model until the pointer comes up.
        let live = session
            .annotation_drag
            .as_ref()
            .filter(|drag| drag.id == annotation.id)
            .and_then(|drag| annotations::dragged(annotation, drag));
        let painted = live.as_ref().unwrap_or(annotation);
        draw_annotation(
            context,
            painted,
            session.stamp_surfaces.get(&painted.id),
            page.height_pt,
            scale,
            selected,
        );
        if selected {
            draw_handles(context, painted, page.height_pt, scale);
        }
    }

    // The annotation being dragged right now, painted through the same
    // function as a committed one — what the user drags is what they get.
    // Drawn selected, because it is about to become the selection.
    if let Some(preview) = session
        .placement
        .as_ref()
        .filter(|placement| placement.page_index == page_index)
        .and_then(super::annotations::placement_preview)
    {
        draw_annotation(context, &preview, None, page.height_pt, scale, true);
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

/// Paints one outline per text run on `page`, while content-edit mode is on.
///
/// A no-op until the page's content has been parsed at least once
/// (`content_edit::load_all_page_content` does this eagerly the moment the
/// mode turns on) — draw funcs must not have side effects, so this never
/// triggers the parse itself.
fn draw_content_run_outlines(context: &cairo::Context, page: &PageSlot, scale: f64) {
    let Some(content) = page.content.as_ref() else {
        return;
    };
    for run in &content.text_runs {
        let placed = place_rect(
            TextRect {
                x_pt: run.bbox.x as f32,
                y_pt: run.bbox.y as f32,
                width_pt: run.bbox.width as f32,
                height_pt: run.bbox.height as f32,
            },
            page.height_pt,
            scale,
        );
        if run.font_kind == FontKind::EmbeddedComposite {
            let (red, green, blue, alpha) = COMPOSITE_RUN_OUTLINE_RGBA;
            context.set_source_rgba(red, green, blue, alpha);
            context.set_dash(&COMPOSITE_RUN_DASH, 0.0);
        } else {
            let (red, green, blue, alpha) = CONTENT_RUN_OUTLINE_RGBA;
            context.set_source_rgba(red, green, blue, alpha);
            context.set_dash(&[], 0.0);
        }
        context.rectangle(placed.left, placed.top, placed.width, placed.height);
        let _ = context.stroke();
    }
}

/// Paints one outline per image on `page`, while content-edit mode is on —
/// the image twin of [`draw_content_run_outlines`] (T-162).
///
/// The selected image (or the one being dragged) is painted at its live
/// rect — sourced from `content_edit::geometry::dragged_rect`, the same
/// "what the user drags is what they get" posture `draw_highlights` already
/// uses for `session.annotation_drag` — in the brighter colour, with its
/// handles; every other image is painted at its committed rect.
fn draw_content_image_outlines(
    context: &cairo::Context,
    page: &PageSlot,
    session: &DocumentSession,
    page_index: usize,
    scale: f64,
) {
    let Some(content) = page.content.as_ref() else {
        return;
    };
    let selected_id = session
        .selected_image
        .as_ref()
        .filter(|selected| selected.page_index == page_index)
        .map(|selected| selected.item.id);
    let live_rect = session
        .image_drag
        .as_ref()
        .filter(|drag| drag.page_index == page_index)
        .and_then(|drag| content_edit::geometry::dragged_rect(drag.item.bbox, drag));

    for image in &content.images {
        let selected = selected_id == Some(image.id);
        let rect = if selected {
            live_rect.unwrap_or(image.bbox)
        } else {
            image.bbox
        };
        let placed = place_rect(
            TextRect {
                x_pt: rect.x as f32,
                y_pt: rect.y as f32,
                width_pt: rect.width as f32,
                height_pt: rect.height as f32,
            },
            page.height_pt,
            scale,
        );
        let (red, green, blue, alpha) = if selected {
            SELECTED_CONTENT_IMAGE_RGBA
        } else {
            CONTENT_IMAGE_OUTLINE_RGBA
        };
        context.set_source_rgba(red, green, blue, alpha);
        context.set_dash(&[], 0.0);
        context.rectangle(placed.left, placed.top, placed.width, placed.height);
        let _ = context.stroke();
        if selected {
            draw_image_handles(context, rect, page.height_pt, scale);
        }
    }
}

/// Paints the four corner handles of the selected image — the image twin of
/// [`draw_handles`] (T-162), over a plain [`Rect`] rather than an
/// `Annotation`, since an image has no annotation kind of its own.
fn draw_image_handles(context: &cairo::Context, rect: Rect, page_height_pt: f32, scale: f64) {
    let placed = place_rect(
        TextRect {
            x_pt: rect.x as f32,
            y_pt: rect.y as f32,
            width_pt: rect.width as f32,
            height_pt: rect.height as f32,
        },
        page_height_pt,
        scale,
    );
    let (red, green, blue) = HANDLE_RGB;
    context.set_source_rgb(red, green, blue);
    for (x, y) in [
        (placed.left, placed.top),
        (placed.left + placed.width, placed.top),
        (placed.left, placed.top + placed.height),
        (placed.left + placed.width, placed.top + placed.height),
    ] {
        context.rectangle(
            x - HANDLE_PX / 2.0,
            y - HANDLE_PX / 2.0,
            HANDLE_PX,
            HANDLE_PX,
        );
    }
    let _ = context.fill();
}

/// Paints one annotation from the editable model onto its page's layer.
///
/// These annotations are *not* in pdfium's raster of the page: they live only
/// in the document model's pending `EditLog` until a save writes them out, so
/// the shell previews them itself. A kind this preview cannot draw is skipped
/// rather than approximated with a wrong shape.
fn draw_annotation(
    context: &cairo::Context,
    annotation: &Annotation,
    stamp: Option<&cairo::ImageSurface>,
    page_height_pt: f32,
    scale: f64,
    selected: bool,
) {
    let alpha = if selected {
        SELECTED_ANNOTATION_ALPHA
    } else {
        ANNOTATION_ALPHA
    };
    match &annotation.kind {
        AnnotationKind::Highlight { rect, color } => {
            let placed = place_annotation(*rect, page_height_pt, scale);
            set_annotation_color(context, *color, alpha);
            context.rectangle(placed.left, placed.top, placed.width, placed.height);
            let _ = context.fill();
        }
        AnnotationKind::Underline { rect, color } => {
            let placed = place_annotation(*rect, page_height_pt, scale);
            set_annotation_color(context, *color, alpha);
            context.rectangle(
                placed.left,
                placed.top + placed.height - RULE_THICKNESS,
                placed.width,
                RULE_THICKNESS,
            );
            let _ = context.fill();
        }
        AnnotationKind::Strikeout { rect, color } => {
            let placed = place_annotation(*rect, page_height_pt, scale);
            set_annotation_color(context, *color, alpha);
            context.rectangle(
                placed.left,
                placed.top + (placed.height - RULE_THICKNESS) / 2.0,
                placed.width,
                RULE_THICKNESS,
            );
            let _ = context.fill();
        }
        AnnotationKind::Shape { rect, color } => {
            let placed = place_annotation(*rect, page_height_pt, scale);
            set_annotation_color(context, *color, alpha);
            context.rectangle(placed.left, placed.top, placed.width, placed.height);
            let _ = context.stroke();
        }
        AnnotationKind::Ink { points, color } => {
            let Some(&first) = points.first() else {
                return;
            };
            set_annotation_color(context, *color, alpha);
            let (x, y) = place_annotation_point(first, page_height_pt, scale);
            context.move_to(x, y);
            for &point in &points[1..] {
                let (x, y) = place_annotation_point(point, page_height_pt, scale);
                context.line_to(x, y);
            }
            let _ = context.stroke();
        }
        AnnotationKind::Stamp { rect, .. } => {
            let placed = place_annotation(*rect, page_height_pt, scale);
            if let Some(surface) = stamp {
                draw_stamp_surface(context, surface, placed, alpha);
            } else {
                draw_annotation_outline(context, placed, alpha);
            }
        }
        // No preview appearance yet: outlined so the user can see where the
        // annotation landed and that it is selected.
        AnnotationKind::TextNote { rect, .. } => {
            draw_annotation_outline(
                context,
                place_annotation(*rect, page_height_pt, scale),
                alpha,
            );
        }
        _ => {}
    }
}

/// Decodes stamp image bytes into a Cairo surface once, when the stamp is
/// created.
///
/// Deliberately not stored as a `gdk::Texture`: the draw function below runs
/// on every frame, so downloading the texture there would copy the entire
/// bitmap out of the GPU per redraw. Paying that once makes each later paint a
/// blit.
///
/// `None` when GTK cannot decode the bytes. The caller keeps the valid PDF
/// annotation and falls back to the outline preview.
pub(crate) fn stamp_surface(image_bytes: Vec<u8>) -> Option<cairo::ImageSurface> {
    let texture = gdk::Texture::from_bytes(&glib::Bytes::from_owned(image_bytes)).ok()?;
    let (width, height) = (texture.width(), texture.height());
    if width <= 0 || height <= 0 {
        return None;
    }
    let stride = width.checked_mul(4)?;
    let mut pixels = vec![0; (stride as usize).checked_mul(height as usize)?];
    texture.download(&mut pixels, stride as usize);
    // `Texture::download` produces Cairo ARGB32 premultiplied pixels. Giving
    // them to Cairo directly preserves both native channel order and alpha.
    downloaded_texture_surface(pixels, width, height, stride)
}

/// Paints a decoded stamp into the current PDF-space annotation rect. The clip
/// prevents source pixels leaking beyond a resized rect, while the current
/// placement geometry makes move, resize, and zoom updates automatic.
fn draw_stamp_surface(
    context: &cairo::Context,
    surface: &cairo::ImageSurface,
    placed: PlacedRect,
    alpha: f64,
) {
    let (width, height) = (surface.width(), surface.height());
    if width <= 0 || height <= 0 || placed.width <= 0.0 || placed.height <= 0.0 {
        draw_annotation_outline(context, placed, alpha);
        return;
    }

    let _ = context.save();
    context.rectangle(placed.left, placed.top, placed.width, placed.height);
    context.clip();
    context.translate(placed.left, placed.top);
    context.scale(
        placed.width / f64::from(width),
        placed.height / f64::from(height),
    );
    let _ = context.set_source_surface(surface, 0.0, 0.0);
    let _ = context.paint();
    let _ = context.restore();
}

/// Wraps GTK's Cairo-native downloaded pixels without reinterpreting them as
/// straight RGBA data.
fn downloaded_texture_surface(
    pixels: Vec<u8>,
    width: i32,
    height: i32,
    stride: i32,
) -> Option<cairo::ImageSurface> {
    cairo::ImageSurface::create_for_data(pixels, cairo::Format::ARgb32, width, height, stride).ok()
}

fn draw_annotation_outline(context: &cairo::Context, placed: PlacedRect, alpha: f64) {
    let (red, green, blue) = PLACEHOLDER_ANNOTATION_RGB;
    context.set_source_rgba(red, green, blue, alpha);
    context.rectangle(placed.left, placed.top, placed.width, placed.height);
    let _ = context.stroke();
}

/// Paints the four corner handles of the selected annotation.
///
/// Drawn at a fixed size in device pixels rather than scaled with the page:
/// a handle that shrank with the zoom would become impossible to hit exactly
/// when the user has zoomed out to see the whole page.
fn draw_handles(
    context: &cairo::Context,
    annotation: &Annotation,
    page_height_pt: f32,
    scale: f64,
) {
    let Some(rect) = annotations::bounds(annotation) else {
        return;
    };
    let placed = place_annotation(rect, page_height_pt, scale);
    let (red, green, blue) = HANDLE_RGB;
    context.set_source_rgb(red, green, blue);
    for (x, y) in [
        (placed.left, placed.top),
        (placed.left + placed.width, placed.top),
        (placed.left, placed.top + placed.height),
        (placed.left + placed.width, placed.top + placed.height),
    ] {
        context.rectangle(
            x - HANDLE_PX / 2.0,
            y - HANDLE_PX / 2.0,
            HANDLE_PX,
            HANDLE_PX,
        );
    }
    let _ = context.fill();
}

/// How far, in PDF points, a press may land from a corner and still count as
/// grabbing its handle. Derived from the zoom so the grab area matches the
/// handle the user can see.
pub(crate) fn handle_reach(viewer: &Viewer, page_index: usize) -> f64 {
    let state = viewer.state.borrow();
    let scale = state
        .session
        .as_ref()
        .and_then(|session| session.pages.get(page_index))
        .map_or(1.0, |page| page.budget.factor);
    if scale.is_finite() && scale > 0.0 {
        HANDLE_PX / scale
    } else {
        HANDLE_PX
    }
}

/// The annotation twin of [`place_rect`]: the same PDF→screen transform, for
/// the `f64` [`Rect`] the document model uses rather than pdfium's `f32`
/// [`TextRect`].
///
/// Delegates rather than re-deriving, so the transform stays defined exactly
/// once — in `pdf_render::selection`, which all four shells share.
fn place_annotation(rect: Rect, page_height_pt: f32, scale: f64) -> PlacedRect {
    place_rect(
        TextRect {
            x_pt: rect.x as f32,
            y_pt: rect.y as f32,
            width_pt: rect.width as f32,
            height_pt: rect.height as f32,
        },
        page_height_pt,
        scale,
    )
}

/// [`place_annotation`] for a bare point (an ink polyline vertex), which has
/// no height to subtract.
fn place_annotation_point(point: (f64, f64), page_height_pt: f32, scale: f64) -> (f64, f64) {
    let (x, y) = point;
    (x * scale, (f64::from(page_height_pt) - y) * scale)
}

fn set_annotation_color(context: &cairo::Context, color: Color, alpha: f64) {
    context.set_source_rgba(
        f64::from(color.r) / 255.0,
        f64::from(color.g) / 255.0,
        f64::from(color.b) / 255.0,
        alpha,
    );
}

/// The current text selection as one rect per line, in PDF page space,
/// alongside the page it is on.
///
/// One rect per line rather than one box around the lot: a selection spanning
/// three lines is three bands of text, and a single bounding box would swallow
/// the margins and both ragged ends. `line_rects` is the same union the search
/// highlighter uses, so a marked-up selection lines up with a search hit over
/// the same words.
///
/// `None` when nothing is selected, when the page's text has not loaded yet,
/// or when the selection is empty — a click that selected no characters must
/// not become a zero-width annotation.
pub(crate) fn selected_line_rects(viewer: &Viewer) -> Option<(usize, Vec<TextRect>)> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    let selection = session.selection.as_ref()?;
    let page = session.pages.get(selection.page_index)?;
    let characters = page.characters.as_ref()?;
    let rects = line_rects(&characters.rects_in(resolve_range(characters, selection)?));

    (!rects.is_empty()).then_some((selection.page_index, rects))
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
    // Content-edit mode claims the whole gesture. An image press is
    // live-claimed right here (T-162's `content_edit::begin_drag`, so its
    // drag has a live preview); a text-run click still has none and only
    // resolves later, on `drag_end` (`content_edit::handle_drag_end`).
    if content_edit::mode_is_active(viewer) {
        if let Some(point) = pointer_to_pdf(viewer, page_index, x, y) {
            content_edit::begin_drag(viewer, page_index, point, handle_reach(viewer, page_index));
        }
        return;
    }
    // An armed creation tool claims the drag: the user is drawing an
    // annotation, not selecting text. Checked before the extraction refusal
    // below, because placing an annotation extracts nothing — a document may
    // forbid copying its text and still permit being annotated.
    if let Some(point) = pointer_to_pdf(viewer, page_index, x, y) {
        if annotations::begin_placement(viewer, page_index, point) {
            return;
        }
        // Then an annotation already on the page: its corner handles resize
        // it, its body moves it, and any other one becomes the selection.
        // Ahead of text selection, because a press that lands on an
        // annotation is aimed at the annotation.
        if annotations::begin_annotation_drag(
            viewer,
            page_index,
            point,
            handle_reach(viewer, page_index),
        ) {
            return;
        }
    }
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

fn extend_selection(viewer: &Viewer, page_index: usize, x: f64, y: f64) {
    // The gesture belongs to the page the drag started on, so the placement
    // stays in that page's space even if the pointer wanders past its edge.
    if let Some(point) = pointer_to_pdf(viewer, page_index, x, y) {
        if annotations::extend_placement(viewer, point)
            || annotations::extend_annotation_drag(viewer, point)
            || content_edit::extend_drag(viewer, point)
        {
            return;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloaded_texture_surface_preserves_premultiplied_orange_alpha() {
        let source = downloaded_texture_surface(0x8080_4000_u32.to_ne_bytes().to_vec(), 1, 1, 4)
            .expect("valid ARGB32 source surface");
        let mut destination =
            cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).expect("destination surface");
        let context = cairo::Context::new(&destination).expect("destination context");

        context
            .set_source_surface(&source, 0.0, 0.0)
            .expect("source surface");
        context.paint().expect("paint source");
        drop(context);

        let pixels = destination.data().expect("destination pixels");
        assert_eq!(
            u32::from_ne_bytes(pixels[..4].try_into().unwrap()),
            0x8080_4000
        );
    }
}
