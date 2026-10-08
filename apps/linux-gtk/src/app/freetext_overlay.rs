//! Painting a text box on the page overlay.
//!
//! A text box is not in pdfium's raster while it is pending: the preview is
//! built by `pdf_save::save_preview`, which leaves the annotation layer out,
//! so the overlay is the only thing that paints it and the text appears once.
//!
//! The overlay never wraps text itself. Every line it draws, and where, comes
//! from `pdf_annotate::layout` — the function the saved `/AP` stream is built
//! from — so the line breaks the user sees while editing are the ones every
//! other viewer shows after saving. The shell's own font (`sans-serif`, usually
//! Liberation Sans or DejaVu) differs from Helvetica in width, which moves a
//! line's *end*, never where it breaks.
//!
//! What is computed here is plain data ([`drawn_text`], [`upright_pose`]) so
//! it can be asserted without a display; [`paint_text`] only replays it onto a
//! cairo context.

use gtk::cairo;
use pdf_document::{Annotation, AnnotationKind, Rect};
use pdf_render::{place_point, PagePlacement};

/// Where a rect's own upright frame starts on the canvas, and how far that
/// frame is turned.
///
/// The frame's origin is the rect's top-left corner *as the content sees it*
/// (PDF `y + height`), whichever way the page is turned, and its axes follow
/// the page's rotation. Text laid out along the frame's x axis therefore reads
/// along the page, which on a page turned 90 degrees means down the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct UprightPose {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) radians: f64,
}

/// The pose of `rect`'s upright frame on `page`. Shared with every overlay
/// that paints something with an orientation, so they cannot disagree about
/// where a turned page's content starts.
pub(super) fn upright_pose(rect: Rect, page: PagePlacement) -> UprightPose {
    let (x, y) = place_point((rect.x, rect.y + rect.height), page);
    UprightPose {
        x,
        y,
        radians: page.rotation.radians(),
    }
}

/// One line to draw, in the box's upright frame, in device pixels.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DrawnLine {
    pub(super) text: String,
    pub(super) x: f64,
    pub(super) baseline: f64,
}

/// Everything needed to paint a text box's text at a given zoom.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DrawnText {
    pub(super) font_size: f64,
    /// The clip: the box's own size at this zoom.
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) lines: Vec<DrawnLine>,
}

/// The text of `annotation` as it is drawn at `scale` display units per point,
/// or `None` when it is not a text box (or its text cannot be laid out, which
/// the model's invariants rule out).
pub(super) fn drawn_text(annotation: &Annotation, scale: f64) -> Option<DrawnText> {
    let AnnotationKind::FreeText {
        rect,
        contents,
        style,
    } = &annotation.kind
    else {
        return None;
    };
    let laid_out = pdf_annotate::layout_free_text(contents, style, rect.width, rect.height).ok()?;
    Some(DrawnText {
        // A floor, because cairo draws a zero-size font as nothing and a
        // collapsed page scale must not turn the text into an error.
        font_size: (laid_out.font_size_pt * scale).max(1.0),
        width: rect.width * scale,
        height: rect.height * scale,
        lines: laid_out
            .lines
            .into_iter()
            .map(|line| DrawnLine {
                text: line.text,
                x: line.x_pt * scale,
                baseline: line.baseline_from_top_pt * scale,
            })
            .collect(),
    })
}

/// Paints the text of `annotation` into the box's upright frame, which the
/// caller has already entered (`selection::enter_upright_frame`), clipped to
/// the box so an overflowing line never spills onto the page.
pub(super) fn paint_text(context: &cairo::Context, annotation: &Annotation, scale: f64) {
    let Some(text) = drawn_text(annotation, scale) else {
        return;
    };
    let AnnotationKind::FreeText { style, .. } = &annotation.kind else {
        return;
    };
    let _ = context.save();
    context.rectangle(0.0, 0.0, text.width, text.height);
    context.clip();
    context.set_source_rgb(
        f64::from(style.color.r) / 255.0,
        f64::from(style.color.g) / 255.0,
        f64::from(style.color.b) / 255.0,
    );
    context.select_font_face(
        super::selection::cairo_font_family(style.font),
        cairo::FontSlant::Normal,
        cairo::FontWeight::Normal,
    );
    context.set_font_size(text.font_size);
    for line in &text.lines {
        context.move_to(line.x, line.baseline);
        let _ = context.show_text(&line.text);
    }
    let _ = context.restore();
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{AnnotationId, PageId};
    use pdf_render::PageRotation;

    const BOX_RECT: Rect = Rect {
        x: 100.0,
        y: 600.0,
        width: 90.0,
        height: 80.0,
    };
    const WRAP_SENSITIVE: &str = "Canci\u{F3}n de a\u{F1}os y m\u{E1}s palabras para envolver";

    fn text_box(contents: &str) -> Annotation {
        pdf_annotate::free_text(AnnotationId(1), PageId(0), BOX_RECT, contents)
            .expect("valid text box")
    }

    fn core_lines(contents: &str) -> Vec<pdf_annotate::FreeTextLine> {
        pdf_annotate::layout_free_text(
            contents,
            &pdf_annotate::default_free_text_style(),
            BOX_RECT.width,
            BOX_RECT.height,
        )
        .expect("layout")
        .lines
    }

    #[test]
    fn the_overlay_draws_exactly_the_lines_the_core_lays_out() {
        let drawn = drawn_text(&text_box(WRAP_SENSITIVE), 1.0).expect("a text box");

        let drawn_strings: Vec<_> = drawn.lines.iter().map(|line| line.text.as_str()).collect();
        let core = core_lines(WRAP_SENSITIVE);
        let core_strings: Vec<_> = core.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(drawn_strings, core_strings);
        assert!(drawn.lines.len() > 1, "the fixture must actually wrap");
    }

    #[test]
    fn accented_letters_reach_the_draw_call_intact() {
        let drawn = drawn_text(&text_box("Canci\u{F3}n a\u{F1}os"), 1.0).expect("a text box");
        let all: String = drawn.lines.iter().map(|line| line.text.as_str()).collect();

        assert!(all.contains('\u{F3}') && all.contains('\u{F1}'), "{all}");
    }

    #[test]
    fn an_explicit_line_break_is_two_drawn_lines() {
        let drawn = drawn_text(&text_box("one\ntwo"), 1.0).expect("a text box");

        let strings: Vec<_> = drawn.lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(strings, ["one", "two"]);
    }

    #[test]
    fn doubling_the_zoom_doubles_size_positions_and_clip() {
        let annotation = text_box(WRAP_SENSITIVE);
        let one = drawn_text(&annotation, 1.0).expect("a text box");
        let two = drawn_text(&annotation, 2.0).expect("a text box");

        assert_eq!(two.font_size, one.font_size * 2.0);
        assert_eq!((two.width, two.height), (one.width * 2.0, one.height * 2.0));
        for (a, b) in one.lines.iter().zip(&two.lines) {
            assert_eq!(a.text, b.text, "zoom must not re-wrap");
            assert_eq!((b.x, b.baseline), (a.x * 2.0, a.baseline * 2.0));
        }
    }

    #[test]
    fn the_first_baseline_sits_one_ascent_below_the_padded_top() {
        let drawn = drawn_text(&text_box("one"), 1.0).expect("a text box");

        assert_eq!(
            drawn.lines[0].baseline,
            pdf_annotate::FREE_TEXT_PADDING_PT + pdf_annotate::FREE_TEXT_ASCENT * 12.0
        );
        assert_eq!(drawn.lines[0].x, pdf_annotate::FREE_TEXT_PADDING_PT);
    }

    #[test]
    fn a_note_has_no_drawn_text() {
        let note = pdf_annotate::text_note(AnnotationId(1), PageId(0), BOX_RECT, "hi");

        assert!(drawn_text(&note, 1.0).is_none());
    }

    fn page(rotation: PageRotation, width_pt: f32, height_pt: f32, scale: f64) -> PagePlacement {
        PagePlacement {
            width_pt,
            height_pt,
            rotation,
            scale,
        }
    }

    /// The box's top-left in PDF space is (100, 680). On the drawn page that
    /// corner lands where `place_point` says, and the frame turns with the page.
    #[test]
    fn an_unrotated_page_starts_the_frame_at_the_boxs_top_left() {
        let pose = upright_pose(BOX_RECT, page(PageRotation::None, 612.0, 792.0, 1.0));

        assert_eq!((pose.x, pose.y, pose.radians), (100.0, 792.0 - 680.0, 0.0));
    }

    #[test]
    fn a_quarter_turn_moves_the_origin_and_turns_the_frame() {
        let pose = upright_pose(BOX_RECT, page(PageRotation::Clockwise90, 792.0, 612.0, 1.0));

        assert_eq!((pose.x, pose.y), (680.0, 100.0));
        assert_eq!(pose.radians, 90f64.to_radians());
    }

    #[test]
    fn a_half_turn_puts_the_origin_on_the_far_side() {
        let pose = upright_pose(
            BOX_RECT,
            page(PageRotation::Clockwise180, 612.0, 792.0, 1.0),
        );

        assert_eq!((pose.x, pose.y), (612.0 - 100.0, 680.0));
        assert_eq!(pose.radians, 180f64.to_radians());
    }

    #[test]
    fn three_quarters_turn_the_other_way() {
        let pose = upright_pose(
            BOX_RECT,
            page(PageRotation::Clockwise270, 792.0, 612.0, 1.0),
        );

        assert_eq!((pose.x, pose.y), (792.0 - 680.0, 612.0 - 100.0));
        assert_eq!(pose.radians, 270f64.to_radians());
    }

    #[test]
    fn the_pose_scales_with_the_zoom() {
        let one = upright_pose(BOX_RECT, page(PageRotation::None, 612.0, 792.0, 1.0));
        let two = upright_pose(BOX_RECT, page(PageRotation::None, 612.0, 792.0, 2.0));

        assert_eq!((two.x, two.y), (one.x * 2.0, one.y * 2.0));
    }
}
