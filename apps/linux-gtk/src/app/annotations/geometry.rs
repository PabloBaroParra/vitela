//! Pointer maths for annotations: what a gesture traces, what counts as a drag
//! rather than a click, where the resize handles sit, and what a drag in
//! progress does to the annotation under it.
//!
//! Everything here is a pure function over rects and points — no GTK, no
//! session state, no borrowing. That is what makes the whole module testable
//! without a display.

use pdf_document::{Annotation, AnnotationKind, Rect};

use crate::app::state::{AnnotationDrag, AnnotationDragMode, Corner, Placement};

/// Size given to an annotation the user placed with a click rather than a
/// drag, in PDF points.
pub(super) const CLICK_SIZE_PT: (f64, f64) = (144.0, 36.0);
/// How far the pointer must travel from the press point before the gesture
/// counts as a drag rather than a click.
///
/// Without a click affordance, a press that wobbles by one pixel would produce
/// a sliver the user can neither see nor grab again — and it would be
/// effectively unrecoverable, since deleting an annotation needs it selected.
const MIN_DRAG_PT: f64 = 8.0;
/// Floor for each side of a traced rect, so a flat sweep still leaves
/// something visible and selectable behind.
const MIN_TRACED_PT: f64 = 4.0;
/// Height of the band a freehand rule occupies, in PDF points. Thin enough to
/// read as a line, tall enough that the rect stays a real target for the edit
/// buttons rather than a degenerate one.
const RULE_BAND_PT: f64 = 2.0;

/// Exactly the rect the pointer traced, normalised so dragging in any of the
/// four directions produces the same rectangle.
///
/// Each side is floored at [`MIN_TRACED_PT`] so a perfectly flat sweep — which
/// is what highlighting a line of text *is* — cannot collapse into a
/// zero-height band that paints nothing and can never be selected again. A
/// floor only stops the rect shrinking; it never moves it, so the preview
/// keeps tracking the pointer.
pub(super) fn traced_rect(placement: &Placement) -> Rect {
    let (origin_x, origin_y) = placement.origin;
    let (current_x, current_y) = placement.current;
    let width = (current_x - origin_x).abs().max(MIN_TRACED_PT);

    if placement.tool.draws_a_rule() {
        // Pinned to the press point: the rule stays exactly where the pointer
        // went down, however much the hand drifted while sweeping sideways.
        return Rect {
            x: origin_x.min(current_x),
            y: origin_y,
            width,
            height: RULE_BAND_PT,
        };
    }
    Rect {
        x: origin_x.min(current_x),
        y: origin_y.min(current_y),
        width,
        height: (current_y - origin_y).abs().max(MIN_TRACED_PT),
    }
}

/// A default-sized rect with the press point at its top-left corner. PDF space
/// has a bottom-left origin, so "top-left" is `y - height`.
fn click_rect(placement: &Placement) -> Rect {
    let (origin_x, origin_y) = placement.origin;
    let (width, height) = CLICK_SIZE_PT;

    if placement.tool.draws_a_rule() {
        // A default-length rule *on* the press point, not a box hanging below
        // it — dropping the line 36pt away from the click would be a surprise.
        return Rect {
            x: origin_x,
            y: origin_y,
            width,
            height: RULE_BAND_PT,
        };
    }
    Rect {
        x: origin_x,
        y: origin_y - height,
        width,
        height,
    }
}

/// Whether the pointer travelled far enough for this to be a drag rather than
/// a click.
///
/// Measured as distance from the press point, **not** per axis. Highlighting a
/// line of text is a deliberately wide, shallow sweep whose height stays under
/// any sane threshold; judging that axis on its own called a real drag a
/// click, so the preview jumped to a default-sized box, and crossing the
/// threshold while moving up and down made it flip back and forth.
fn is_a_drag(placement: &Placement) -> bool {
    let (origin_x, origin_y) = placement.origin;
    let (current_x, current_y) = placement.current;
    let across = current_x - origin_x;

    if placement.tool.draws_a_rule() {
        // Only travel along the rule counts, for the same reason its height is
        // pinned: vertical movement changes nothing about the mark, so letting
        // it decide drag-versus-click would contradict what the user sees.
        return across.abs() >= MIN_DRAG_PT;
    }
    across.hypot(current_y - origin_y) >= MIN_DRAG_PT
}

/// The rect to commit: what the pointer traced, or a default-sized box when it
/// barely moved.
///
/// Decided here, at the end of the gesture, rather than continuously while it
/// runs. A mid-drag decision means the shape under the pointer can change
/// identity as the threshold is crossed, which reads as the preview popping.
pub(super) fn committed_rect(placement: &Placement) -> Rect {
    if is_a_drag(placement) {
        traced_rect(placement)
    } else {
        click_rect(placement)
    }
}

/// The rectangle an annotation occupies, whatever its kind.
///
/// Ink has no rect of its own, so it reports the bounding box of its stroke —
/// enough to hit-test and to move, though not to resize (`pdf-annotate`
/// refuses that, and squashing a polyline into a box would be a guess).
pub(crate) fn bounds(annotation: &Annotation) -> Option<Rect> {
    match &annotation.kind {
        AnnotationKind::Highlight { rect, .. }
        | AnnotationKind::Underline { rect, .. }
        | AnnotationKind::Strikeout { rect, .. }
        | AnnotationKind::Shape { rect, .. }
        | AnnotationKind::TextNote { rect, .. }
        | AnnotationKind::Stamp { rect, .. } => Some(*rect),
        AnnotationKind::Ink { points, .. } => {
            let (first_x, first_y) = *points.first()?;
            let (mut min_x, mut min_y) = (first_x, first_y);
            let (mut max_x, mut max_y) = (first_x, first_y);
            for &(x, y) in points {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            Some(Rect {
                x: min_x,
                y: min_y,
                width: max_x - min_x,
                height: max_y - min_y,
            })
        }
        _ => None,
    }
}

/// Where a corner sits, in PDF page space.
fn corner_point(rect: Rect, corner: Corner) -> (f64, f64) {
    let (left, bottom) = (rect.x, rect.y);
    let (right, top) = (rect.x + rect.width, rect.y + rect.height);
    match corner {
        Corner::BottomLeft => (left, bottom),
        Corner::BottomRight => (right, bottom),
        Corner::TopLeft => (left, top),
        Corner::TopRight => (right, top),
    }
}

/// The corner whose handle is under `point`, if any.
///
/// `reach` is the grab radius in PDF points. The caller derives it from the
/// zoom so the handle stays the same size under the pointer however far the
/// page is scaled — a handle that shrank with the page would become
/// impossible to hit when zoomed out.
pub(super) fn corner_at(rect: Rect, point: (f64, f64), reach: f64) -> Option<Corner> {
    Corner::ALL.into_iter().find(|corner| {
        let (corner_x, corner_y) = corner_point(rect, *corner);
        (point.0 - corner_x).abs() <= reach && (point.1 - corner_y).abs() <= reach
    })
}

/// Whether `point` is inside the annotation's body.
pub(super) fn contains(rect: Rect, point: (f64, f64)) -> bool {
    point.0 >= rect.x
        && point.0 <= rect.x + rect.width
        && point.1 >= rect.y
        && point.1 <= rect.y + rect.height
}

/// The rect a resize drag produces: the grabbed corner follows the pointer,
/// the opposite corner stays put, and the result is normalised so dragging a
/// corner past its opposite flips the rect rather than inverting it.
fn resized_rect(rect: Rect, corner: Corner, point: (f64, f64)) -> Rect {
    let (anchor_x, anchor_y) = corner_point(rect, opposite(corner));
    Rect {
        x: anchor_x.min(point.0),
        y: anchor_y.min(point.1),
        width: (point.0 - anchor_x).abs().max(MIN_TRACED_PT),
        height: (point.1 - anchor_y).abs().max(MIN_TRACED_PT),
    }
}

fn opposite(corner: Corner) -> Corner {
    match corner {
        Corner::BottomLeft => Corner::TopRight,
        Corner::BottomRight => Corner::TopLeft,
        Corner::TopLeft => Corner::BottomRight,
        Corner::TopRight => Corner::BottomLeft,
    }
}

/// Applies a drag in progress to a copy of the annotation, for previewing and
/// for the value finally committed. One function, so what the user drags is
/// what lands in the `EditLog`.
pub(crate) fn dragged(annotation: &Annotation, drag: &AnnotationDrag) -> Option<Annotation> {
    let mut moved = annotation.clone();
    match drag.mode {
        AnnotationDragMode::Move => {
            let dx = drag.current.0 - drag.origin.0;
            let dy = drag.current.1 - drag.origin.1;
            pdf_annotate::move_annotation(&mut moved, dx, dy).ok()?;
        }
        AnnotationDragMode::Resize(corner) => {
            let rect = resized_rect(bounds(annotation)?, corner, drag.current);
            pdf_annotate::resize_annotation(&mut moved, rect).ok()?;
        }
    }
    Some(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::annotations::test_support::{built, click, drag, rect_of};
    use crate::app::state::Tool;

    /// The whole point of the placement gesture: the annotation lands where
    /// the user dragged, not at a fixed rect.
    #[test]
    fn a_drag_places_the_annotation_where_it_was_drawn() {
        let rect = rect_of(&built(&drag(
            Tool::Highlight,
            (120.0, 400.0),
            (300.0, 460.0),
        )));

        assert_eq!((rect.x, rect.y), (120.0, 400.0));
        assert_eq!((rect.width, rect.height), (180.0, 60.0));
    }

    /// PDF space has a bottom-left origin, so a drag "down and left" on screen
    /// still has to normalise into the same rectangle.
    #[test]
    fn a_drag_in_any_direction_produces_the_same_rect() {
        let forward = rect_of(&built(&drag(Tool::Shape, (120.0, 400.0), (300.0, 460.0))));
        let backward = rect_of(&built(&drag(Tool::Shape, (300.0, 460.0), (120.0, 400.0))));

        assert_eq!((forward.x, forward.y), (backward.x, backward.y));
        assert_eq!(
            (forward.width, forward.height),
            (backward.width, backward.height)
        );
    }

    /// A click is a legitimate way to drop an annotation; it must not produce
    /// a zero-size one the user can never select again.
    #[test]
    fn a_click_places_a_default_sized_annotation_at_the_pointer() {
        let rect = rect_of(&built(&click(Tool::TextNote, (200.0, 500.0))));
        let (width, height) = CLICK_SIZE_PT;

        assert_eq!((rect.width, rect.height), (width, height));
        // Anchored by its top-left corner, which in PDF space is `y - height`.
        assert_eq!((rect.x, rect.y), (200.0, 500.0 - height));
    }

    #[test]
    fn a_press_that_barely_moves_is_treated_as_a_click() {
        let barely = drag(Tool::Shape, (200.0, 500.0), (201.0, 500.5));
        let rect = rect_of(&built(&barely));

        assert_eq!((rect.width, rect.height), CLICK_SIZE_PT);
    }

    /// The bug this pair exists for: sweeping a highlight along a line of text
    /// is deliberately wide and shallow. Judging each axis on its own called
    /// that a click and swapped in a default-sized box mid-gesture, and moving
    /// up and down across the threshold made the preview flip back and forth.
    #[test]
    fn a_wide_shallow_sweep_is_a_drag_not_a_click() {
        let sweep = drag(Tool::Highlight, (100.0, 500.0), (400.0, 502.0));

        assert!(is_a_drag(&sweep));
        let rect = rect_of(&built(&sweep));
        assert_eq!(rect.x, 100.0);
        assert_eq!(rect.width, 300.0);
        assert_ne!((rect.width, rect.height), CLICK_SIZE_PT);
    }

    /// Every point of a sweep past the click threshold must keep tracing, so
    /// the preview never changes identity while the pointer is down.
    #[test]
    fn a_sweep_never_flips_back_to_a_click_as_it_grows() {
        for x in [110.0, 150.0, 200.0, 300.0, 400.0] {
            for y in [498.0, 500.0, 502.0, 505.0] {
                let sweep = drag(Tool::Highlight, (100.0, 500.0), (x, y));
                assert!(is_a_drag(&sweep), "sweep to ({x}, {y}) must stay a drag");
            }
        }
    }

    /// A rule is a line. Sweeping right with a shaky hand must still leave a
    /// straight one at the height the pointer went down.
    #[test]
    fn a_rule_swept_sideways_stays_on_the_press_point() {
        for tool in [Tool::Underline, Tool::Strikeout] {
            let shaky = drag(tool, (100.0, 500.0), (300.0, 537.0));
            let rect = rect_of(&built(&shaky));

            assert_eq!(rect.y, 500.0, "{tool:?} must not follow vertical drift");
            assert_eq!(rect.height, RULE_BAND_PT, "{tool:?} must stay a line");
            assert_eq!(rect.width, 200.0, "{tool:?} must follow the sweep");
        }
    }

    #[test]
    fn vertical_drift_changes_nothing_about_a_rule() {
        let straight = rect_of(&built(&drag(
            Tool::Underline,
            (100.0, 500.0),
            (300.0, 500.0),
        )));

        for drift in [-40.0, -5.0, 5.0, 40.0] {
            let wobbled = rect_of(&built(&drag(
                Tool::Underline,
                (100.0, 500.0),
                (300.0, 500.0 + drift),
            )));

            assert_eq!(
                (wobbled.x, wobbled.y, wobbled.width, wobbled.height),
                (straight.x, straight.y, straight.width, straight.height),
                "a drift of {drift} must not move the rule"
            );
        }
    }

    /// The rule treatment must not leak into the band-shaped tools.
    #[test]
    fn a_highlight_still_uses_both_axes() {
        let rect = rect_of(&built(&drag(
            Tool::Highlight,
            (100.0, 500.0),
            (300.0, 540.0),
        )));

        assert_eq!(rect.height, 40.0);
    }

    #[test]
    fn a_clicked_rule_lands_on_the_pointer_not_below_it() {
        let rect = rect_of(&built(&click(Tool::Underline, (200.0, 500.0))));

        assert_eq!(rect.y, 500.0);
        assert_eq!(rect.height, RULE_BAND_PT);
        assert_eq!(rect.width, CLICK_SIZE_PT.0);
    }

    #[test]
    fn a_flat_sweep_still_leaves_something_selectable() {
        let flat = drag(Tool::Highlight, (100.0, 500.0), (400.0, 500.0));
        let rect = traced_rect(&flat);

        assert_eq!(rect.width, 300.0);
        assert_eq!(rect.height, MIN_TRACED_PT);
    }

    fn a_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_press_inside_an_annotation_hits_it_and_one_outside_does_not() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);

        assert!(contains(rect, (150.0, 520.0)));
        assert!(contains(rect, (100.0, 500.0)), "edges count as inside");
        assert!(!contains(rect, (99.0, 520.0)));
        assert!(!contains(rect, (150.0, 541.0)));
    }

    #[test]
    fn each_corner_has_its_own_handle() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);
        let reach = 5.0;

        assert_eq!(
            corner_at(rect, (100.0, 500.0), reach),
            Some(Corner::BottomLeft)
        );
        assert_eq!(
            corner_at(rect, (300.0, 500.0), reach),
            Some(Corner::BottomRight)
        );
        assert_eq!(
            corner_at(rect, (100.0, 540.0), reach),
            Some(Corner::TopLeft)
        );
        assert_eq!(
            corner_at(rect, (300.0, 540.0), reach),
            Some(Corner::TopRight)
        );
        assert_eq!(
            corner_at(rect, (200.0, 520.0), reach),
            None,
            "the body is not a handle"
        );
    }

    /// Pulling a corner must hold the opposite one still — otherwise the
    /// annotation slides while it resizes and the user chases it.
    #[test]
    fn a_resize_holds_the_opposite_corner_still() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);

        let grown = resized_rect(rect, Corner::TopRight, (400.0, 600.0));

        assert_eq!((grown.x, grown.y), (100.0, 500.0), "anchor must not move");
        assert_eq!((grown.width, grown.height), (300.0, 100.0));
    }

    #[test]
    fn dragging_a_corner_past_its_opposite_flips_rather_than_inverts() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);

        // TopRight dragged below-left of the anchored BottomLeft corner.
        let flipped = resized_rect(rect, Corner::TopRight, (40.0, 460.0));

        assert!(flipped.width > 0.0 && flipped.height > 0.0);
        assert_eq!((flipped.x, flipped.y), (40.0, 460.0));
    }

    #[test]
    fn a_move_drag_shifts_the_annotation_by_the_pointer_delta() {
        let annotation = built(&drag(Tool::Shape, (100.0, 500.0), (200.0, 560.0)));
        let moved = dragged(
            &annotation,
            &AnnotationDrag {
                id: annotation.id,
                mode: AnnotationDragMode::Move,
                origin: (150.0, 520.0),
                current: (170.0, 495.0),
            },
        )
        .expect("a shape can be moved");

        let before = bounds(&annotation).expect("shape has bounds");
        let after = bounds(&moved).expect("shape has bounds");

        assert_eq!((after.x - before.x, after.y - before.y), (20.0, -25.0));
        assert_eq!((after.width, after.height), (before.width, before.height));
    }

    /// Ink has no rect to reshape, so it reports bounds for hit-testing and
    /// moving but must refuse a resize rather than squash its polyline.
    #[test]
    fn ink_can_be_moved_but_not_resized() {
        let ink = built(&drag(Tool::Ink, (10.0, 10.0), (60.0, 40.0)));
        let handle = AnnotationDrag {
            id: ink.id,
            mode: AnnotationDragMode::Resize(Corner::TopRight),
            origin: (60.0, 40.0),
            current: (90.0, 70.0),
        };

        assert!(bounds(&ink).is_some(), "ink still reports bounds");
        assert!(dragged(&ink, &handle).is_none(), "ink refuses a resize");
        assert!(dragged(
            &ink,
            &AnnotationDrag {
                mode: AnnotationDragMode::Move,
                ..handle
            }
        )
        .is_some());
    }

    #[test]
    fn ink_bounds_cover_every_point_of_the_stroke() {
        let mut placement = drag(Tool::Ink, (10.0, 10.0), (60.0, 40.0));
        placement.points = vec![(10.0, 30.0), (35.0, 10.0), (60.0, 40.0)];
        let ink = built(&placement);

        let rect = bounds(&ink).expect("ink has bounds");

        assert_eq!((rect.x, rect.y), (10.0, 10.0));
        assert_eq!((rect.width, rect.height), (50.0, 30.0));
    }
}
