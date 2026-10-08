//! Turning a finished gesture into an `Annotation`.
//!
//! Every creation path in the shell funnels through here, so the tool the user
//! armed and the annotation kind that lands in the document cannot drift apart.

use pdf_document::{Annotation, AnnotationId, Color, Command, PageId, Rect};

use crate::app::selection;
use crate::app::state::{Placement, Tool, Viewer};

use super::command::{apply_command, command, model};
use super::geometry::traced_rect;

const DEFAULT_COLOR: Color = Color {
    r: 255,
    g: 220,
    b: 0,
};

const INK_NEEDS_A_DRAG: &str = "Drag to draw an ink annotation.";
const NOTE_NEEDS_TEXT: &str = "Type the note's text before adding it.";

/// A 1×1 opaque PNG, inlined so the Stamp button has something valid to stamp
/// without reaching for a file. `stamp_from_image_bytes` decodes it to read the
/// alpha channel, so a placeholder still has to be a real image — this is the
/// smallest one that is. Replaced by the clipboard/drag-and-drop image once
/// T-050/T-051 land.
const PLACEHOLDER_STAMP_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0,
    0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 207, 192, 0, 0, 3, 1, 1,
    0, 24, 221, 141, 176, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Builds the annotation a finished [`Placement`] describes.
///
/// The geometry comes from where the user actually dragged — this is the one
/// place the pointer becomes an annotation, and the reason no creation path
/// carries a hard-coded rect any more.
pub(super) fn annotation_at(
    placement: &Placement,
    id: AnnotationId,
    page: PageId,
    rect: Rect,
) -> Result<Annotation, String> {
    match placement.tool {
        Tool::Highlight | Tool::Underline | Tool::Strikeout => {
            markup_annotation(placement.tool, id, page, rect)
        }
        Tool::Ink => {
            // A tap leaves a single point, which is not a stroke — better to
            // say so than to record an invisible annotation.
            if placement.points.len() < 2 {
                return Err(INK_NEEDS_A_DRAG.to_string());
            }
            Ok(pdf_annotate::ink(
                id,
                page,
                placement.points.clone(),
                DEFAULT_COLOR,
            ))
        }
        // A draft: it only exists so the placement preview can outline the
        // note while the pointer is down. The note that lands in the document
        // is built by `note_annotation` once the user has typed its text —
        // `gesture::finish_placement` diverts this tool to the note dialog
        // before it ever reaches here.
        Tool::TextNote => Ok(pdf_annotate::text_note(id, page, rect, "")),
        // The same kind of draft, for the same reason: the text box that
        // lands in the document is built by `freetext::free_text_annotation`
        // once the user has typed its text, and `gesture::finish_placement`
        // diverts this tool to that dialog before it reaches here.
        Tool::FreeText => Ok(pdf_annotate::text_note(id, page, rect, "")),
        Tool::Shape => Ok(pdf_annotate::shape(id, page, rect, DEFAULT_COLOR)),
        Tool::Stamp => pdf_annotate::stamp_from_image_bytes(id, page, PLACEHOLDER_STAMP_PNG, rect)
            .map_err(|error| error.to_string()),
    }
}

/// Whether `text` says nothing: empty or only whitespace.
///
/// The one rule shared by the dialog's Add button and [`note_annotation`], so
/// the button cannot be enabled for text the builder will then refuse.
pub(super) fn is_blank_note(text: &str) -> bool {
    text.trim().is_empty()
}

/// Builds the sticky note a user finished typing. The text is stored exactly
/// as typed; a blank one is refused rather than recorded as an empty note.
pub(super) fn note_annotation(
    id: AnnotationId,
    page: PageId,
    rect: Rect,
    text: &str,
) -> Result<Annotation, String> {
    if is_blank_note(text) {
        return Err(NOTE_NEEDS_TEXT.to_string());
    }
    Ok(pdf_annotate::text_note(id, page, rect, text))
}

/// Builds one text-markup annotation over `rect`.
///
/// Shared by the two ways a markup annotation can be born: dragged over a
/// region, or applied to an existing text selection. Keeping one builder is
/// what stops the two paths from drifting into different colours or kinds.
pub(super) fn markup_annotation(
    tool: Tool,
    id: AnnotationId,
    page: PageId,
    rect: Rect,
) -> Result<Annotation, String> {
    match tool {
        Tool::Highlight => Ok(pdf_annotate::highlight(id, page, rect, DEFAULT_COLOR)),
        Tool::Underline => Ok(pdf_annotate::underline(id, page, rect, DEFAULT_COLOR)),
        Tool::Strikeout => Ok(pdf_annotate::strikeout(id, page, rect, DEFAULT_COLOR)),
        other => Err(format!("{} does not mark up text.", other.label())),
    }
}

/// Inserts an image stamp at a PDF-space point through the same command path
/// as every toolbar annotation, preserving permission checks, undo, redraw,
/// and control state.
pub(crate) fn stamp_from_image_bytes(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    image_bytes: Vec<u8>,
) {
    command(viewer, move |session| {
        let id = AnnotationId(session.next_annotation_id);
        // The canvas index names a page id only through the open handle — see
        // `DocumentSession::backend_pages`.
        let page = session
            .backend_page_id(page_index)
            .ok_or_else(|| crate::app::state::PAGE_NO_LONGER_PRESENT.to_string())?;
        let rect = stamp_rect(&image_bytes, point)
            .map_err(|error| format!("Could not use the image: {error}"))?;
        let annotation = pdf_annotate::stamp_from_image_bytes(id, page, &image_bytes, rect)
            .map_err(|error| format!("Could not use the image: {error}"))?;
        {
            let document = model(session)?;
            apply_command(document, Command::AddAnnotation(annotation));
        }
        // The surface is GTK-only session state: PDF save keeps using the
        // annotation's core image data, while this gives the pending edit a
        // live shell preview. A failed shell decode still leaves the valid PDF
        // annotation in place and uses the outline fallback when painted.
        if let Some(surface) = selection::stamp_surface(image_bytes) {
            session.stamp_surfaces.insert(id, surface);
        }
        session.next_annotation_id += 1;
        session.selected_annotation = Some(id);
        Ok("Image stamp added. Changes are pending save.".to_string())
    });
}

/// Default stamp placement anchored at the pointer's top-left corner.
///
/// The size comes from the core, not from here: a dropped or pasted image
/// keeps its own proportions, so the WinUI shell — which reaches the same
/// function through `pdf_ffi::stamp_placement` — cannot disagree with this one
/// about where a given image lands.
fn stamp_rect(image_bytes: &[u8], point: (f64, f64)) -> Result<Rect, pdf_annotate::AnnotateError> {
    pdf_annotate::stamp_placement(image_bytes, point, pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT)
}

/// The annotation to paint for a placement still in progress, or `None` when
/// it cannot be built yet (an ink stroke of one point, say).
///
/// Always the traced rect: while the pointer is down the preview follows it
/// exactly, with no threshold to cross and nothing to pop. The click fallback
/// belongs to `committed_rect`, and a click has no drag to preview anyway.
pub(crate) fn placement_preview(placement: &Placement) -> Option<Annotation> {
    annotation_at(
        placement,
        // Both ids are placeholders: this annotation is painted for the
        // frame and thrown away, never recorded, and `selection::draw_page`
        // has already picked the page by canvas index before calling. The
        // real page id is resolved where the drag *commits*
        // (`gesture::finish_placement`).
        AnnotationId(0),
        PageId(0),
        traced_rect(placement),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::annotations::geometry::{committed_rect, CLICK_SIZE_PT};
    use crate::app::annotations::test_support::{built, click, drag, rect_of};
    use pdf_document::AnnotationKind;

    #[test]
    fn every_toolbar_tool_builds_the_annotation_kind_its_label_promises() {
        for (tool, expected) in [
            (Tool::Highlight, "Highlight"),
            (Tool::Underline, "Underline"),
            (Tool::Strikeout, "Strikeout"),
            (Tool::Ink, "Ink"),
            (Tool::TextNote, "TextNote"),
            // The text box's draft outline; the real FreeText is built from
            // the dialog (see `freetext::tests`).
            (Tool::FreeText, "TextNote"),
            (Tool::Shape, "Shape"),
            (Tool::Stamp, "Stamp"),
        ] {
            let kind = built(&drag(tool, (100.0, 100.0), (200.0, 160.0))).kind;
            let name = match kind {
                AnnotationKind::Highlight { .. } => "Highlight",
                AnnotationKind::Underline { .. } => "Underline",
                AnnotationKind::Strikeout { .. } => "Strikeout",
                AnnotationKind::Ink { .. } => "Ink",
                AnnotationKind::TextNote { .. } => "TextNote",
                AnnotationKind::Shape { .. } => "Shape",
                AnnotationKind::Stamp { .. } => "Stamp",
                _ => "unknown",
            };
            assert_eq!(name, expected, "{tool:?} built the wrong kind");
        }
    }

    /// A wide RGB PNG at 3:1 — deliberately *not* `CLICK_SIZE_PT`'s own 4:1
    /// ratio, which is the one shape where the old fixed rect happened to be
    /// right and so proves nothing.
    fn wide_png() -> Vec<u8> {
        use image::{DynamicImage, ImageFormat, RgbImage};
        use std::io::Cursor;

        let mut buf = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::from_pixel(300, 100, image::Rgb([10, 20, 30])))
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encoding a test png should succeed");
        buf.into_inner()
    }

    #[test]
    fn an_image_stamp_is_anchored_at_its_drop_point() {
        let rect = stamp_rect(&wide_png(), (200.0, 500.0)).expect("valid png");

        assert_eq!((rect.x, rect.y), (200.0, 500.0 - rect.height));
    }

    #[test]
    fn an_image_stamp_keeps_its_own_proportions_rather_than_the_click_size() {
        let rect = stamp_rect(&wide_png(), (200.0, 500.0)).expect("valid png");

        // 3:1 in, 3:1 out. The old behaviour squashed every image into
        // `CLICK_SIZE_PT`, which is a text-annotation size, not an image one.
        assert_eq!((rect.width, rect.height), (144.0, 48.0));
        assert_ne!((rect.width, rect.height), CLICK_SIZE_PT);
    }

    #[test]
    fn a_square_image_is_not_flattened_into_a_strip() {
        use image::{DynamicImage, ImageFormat, RgbImage};
        use std::io::Cursor;

        let mut buf = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 64, image::Rgb([10, 20, 30])))
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encoding a test png should succeed");

        let rect = stamp_rect(&buf.into_inner(), (200.0, 500.0)).expect("valid png");

        assert_eq!(rect.width, rect.height);
    }

    #[test]
    fn an_image_that_cannot_be_decoded_yields_no_rect() {
        assert!(stamp_rect(b"not an image", (200.0, 500.0)).is_err());
    }

    #[test]
    fn an_ink_stroke_keeps_every_point_the_pointer_visited() {
        let mut placement = drag(Tool::Ink, (10.0, 10.0), (30.0, 40.0));
        placement.points = vec![(10.0, 10.0), (20.0, 25.0), (30.0, 40.0)];

        match built(&placement).kind {
            AnnotationKind::Ink { points, .. } => assert_eq!(points, placement.points),
            other => panic!("expected an ink annotation, got {other:?}"),
        }
    }

    /// A tap with the ink tool is not a stroke. Refusing it beats recording an
    /// annotation that paints nothing and cannot be found again.
    #[test]
    fn a_tap_with_the_ink_tool_is_refused_rather_than_recorded() {
        let tap = click(Tool::Ink, (10.0, 10.0));
        let error = annotation_at(&tap, AnnotationId(1), PageId(0), committed_rect(&tap))
            .expect_err("a single point is not a stroke");

        assert_eq!(error, INK_NEEDS_A_DRAG);
    }

    #[test]
    fn a_placement_preview_paints_the_same_annotation_the_drag_will_commit() {
        let placement = drag(Tool::Highlight, (120.0, 400.0), (300.0, 460.0));

        let preview = placement_preview(&placement).expect("a drag previews");

        assert_eq!(rect_of(&preview), rect_of(&built(&placement)));
    }

    #[test]
    fn a_placement_that_cannot_be_built_yet_has_no_preview() {
        assert!(placement_preview(&click(Tool::Ink, (10.0, 10.0))).is_none());
    }

    /// The two ways a markup annotation is born must share one builder.
    ///
    /// Their *geometry* legitimately differs — a text selection supplies the
    /// selected line's own rect, while a freehand rule pins its own height
    /// (see `a_rule_swept_sideways_stays_on_the_press_point`) — so this pins
    /// the thing that must not differ: given the same rect, both produce the
    /// same annotation.
    #[test]
    fn both_creation_paths_share_one_markup_builder() {
        for tool in Tool::ALL.iter().filter(|tool| tool.marks_up_text()) {
            let placement = drag(*tool, (10.0, 20.0), (110.0, 32.0));
            let rect = committed_rect(&placement);

            let from_drag = built(&placement);
            let from_selection = markup_annotation(*tool, AnnotationId(1), PageId(0), rect)
                .expect("a markup tool builds from a rect");

            assert_eq!(
                from_drag, from_selection,
                "{tool:?} must go through the same builder either way"
            );
        }
    }

    #[test]
    fn a_non_markup_tool_is_refused_rather_than_guessed_at() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };

        assert!(markup_annotation(Tool::Shape, AnnotationId(1), PageId(0), rect).is_err());
        assert!(markup_annotation(Tool::Ink, AnnotationId(1), PageId(0), rect).is_err());
    }

    #[test]
    fn the_placeholder_stamp_decodes_as_a_real_image() {
        let stamp = drag(Tool::Stamp, (10.0, 10.0), (100.0, 100.0));

        assert!(annotation_at(&stamp, AnnotationId(1), PageId(0), committed_rect(&stamp)).is_ok());
    }

    #[test]
    fn a_note_keeps_the_text_the_user_typed() {
        let rect = committed_rect(&drag(Tool::TextNote, (10.0, 10.0), (100.0, 60.0)));

        let note = note_annotation(AnnotationId(1), PageId(0), rect, "Call back on Friday")
            .expect("typed text builds a note");

        match note.kind {
            AnnotationKind::TextNote {
                contents, popup, ..
            } => {
                assert_eq!(contents, "Call back on Friday");
                assert_eq!(popup.contents, "Call back on Friday");
            }
            other => panic!("expected a text note, got {other:?}"),
        }
    }

    #[test]
    fn a_blank_note_is_refused_rather_than_recorded() {
        let rect = committed_rect(&click(Tool::TextNote, (10.0, 10.0)));

        for blank in [
            "", "   ", "
	 ",
        ] {
            assert_eq!(
                note_annotation(AnnotationId(1), PageId(0), rect, blank),
                Err(NOTE_NEEDS_TEXT.to_string()),
                "{blank:?} must not become a note"
            );
        }
    }

    #[test]
    fn blankness_is_judged_the_way_the_add_button_judges_it() {
        assert!(is_blank_note(
            "  
"
        ));
        assert!(!is_blank_note(" a "));
    }
}
