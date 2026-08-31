//! Pointer maths for form fields (T-141): what a placement drag traces,
//! where the resize handles of a selected field sit, and what a drag in
//! progress does to the field under it — the form-field twin of
//! `annotations::geometry`, forked rather than shared per `content_edit`'s
//! own precedent for images: a form field lives in `document.form_fields`,
//! not `document.annotations`, and unifying the two would blur that boundary
//! for no shared behavior beyond the shape.
//!
//! Everything here is a pure function over rects and points — no GTK, no
//! session state — same posture as `annotations::geometry`.

use pdf_document::Rect;

use crate::app::state::{AnnotationDragMode, Corner, FormFieldDrag, FormPlacement};

/// Size given to a field the user placed with a click rather than a drag, in
/// PDF points. Narrower than `annotations::geometry::CLICK_SIZE_PT`'s own
/// 144x36: a form field's default height is closer to a line of text than to
/// a markup annotation's band.
pub(super) const CLICK_SIZE_PT: (f64, f64) = (144.0, 24.0);
/// How far the pointer must travel from the press point before the gesture
/// counts as a drag rather than a click — mirrors
/// `annotations::geometry::MIN_DRAG_PT`.
const MIN_DRAG_PT: f64 = 8.0;
/// Floor for each side of a traced or resized rect, so a flat sweep or a
/// corner dragged onto its opposite still leaves something visible and
/// selectable behind — mirrors `annotations::geometry::MIN_TRACED_PT`.
const MIN_TRACED_PT: f64 = 4.0;

/// Exactly the rect the pointer traced, normalised so dragging in any of the
/// four directions produces the same rectangle. Each side is floored at
/// [`MIN_TRACED_PT`] for the same reason `annotations::geometry::traced_rect`
/// floors its own.
pub(crate) fn traced_rect(placement: &FormPlacement) -> Rect {
    let (origin_x, origin_y) = placement.origin;
    let (current_x, current_y) = placement.current;
    Rect {
        x: origin_x.min(current_x),
        y: origin_y.min(current_y),
        width: (current_x - origin_x).abs().max(MIN_TRACED_PT),
        height: (current_y - origin_y).abs().max(MIN_TRACED_PT),
    }
}

/// A default-sized rect with the press point at its top-left corner. PDF
/// space has a bottom-left origin, so "top-left" is `y - height`.
fn click_rect(placement: &FormPlacement) -> Rect {
    let (origin_x, origin_y) = placement.origin;
    let (width, height) = CLICK_SIZE_PT;
    Rect {
        x: origin_x,
        y: origin_y - height,
        width,
        height,
    }
}

/// Whether the pointer travelled far enough for this to be a drag rather
/// than a click. Distance from the press point, not per axis — mirrors
/// `annotations::geometry::is_a_drag`'s own reasoning.
fn is_a_drag(placement: &FormPlacement) -> bool {
    let (origin_x, origin_y) = placement.origin;
    let (current_x, current_y) = placement.current;
    (current_x - origin_x).hypot(current_y - origin_y) >= MIN_DRAG_PT
}

/// The rect to commit: what the pointer traced, or a default-sized box when
/// it barely moved. Decided here, at the end of the gesture, so the preview
/// painted mid-drag (always [`traced_rect`]) never pops between shapes.
pub(super) fn committed_rect(placement: &FormPlacement) -> Rect {
    if is_a_drag(placement) {
        traced_rect(placement)
    } else {
        click_rect(placement)
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

/// The corner whose handle is under `point`, if any. `reach` is the grab
/// radius in PDF points, derived by the caller from the zoom so the handle
/// stays the same size under the pointer however far the page is scaled.
pub(super) fn corner_at(rect: Rect, point: (f64, f64), reach: f64) -> Option<Corner> {
    Corner::ALL.into_iter().find(|corner| {
        let (corner_x, corner_y) = corner_point(rect, *corner);
        (point.0 - corner_x).abs() <= reach && (point.1 - corner_y).abs() <= reach
    })
}

/// Whether `point` is inside the field's body. Edges count as inside.
pub(super) fn contains(rect: Rect, point: (f64, f64)) -> bool {
    point.0 >= rect.x
        && point.0 <= rect.x + rect.width
        && point.1 >= rect.y
        && point.1 <= rect.y + rect.height
}

fn opposite(corner: Corner) -> Corner {
    match corner {
        Corner::BottomLeft => Corner::TopRight,
        Corner::BottomRight => Corner::TopLeft,
        Corner::TopLeft => Corner::BottomRight,
        Corner::TopRight => Corner::BottomLeft,
    }
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

/// Applies a drag in progress to `bbox`, producing the rect that would be
/// committed if the drag ended now. Infallible, unlike
/// `annotations::geometry::dragged`: `move_field`/`resize_field` never
/// refuse (`pdf-form::ops`'s own doc), because `rect` is unconditional on
/// every `FormField` regardless of kind.
pub(crate) fn dragged_rect(bbox: Rect, drag: &FormFieldDrag) -> Rect {
    match drag.mode {
        AnnotationDragMode::Move => {
            let dx = drag.current.0 - drag.origin.0;
            let dy = drag.current.1 - drag.origin.1;
            Rect {
                x: bbox.x + dx,
                y: bbox.y + dy,
                width: bbox.width,
                height: bbox.height,
            }
        }
        AnnotationDragMode::Resize(corner) => resized_rect(bbox, corner, drag.current),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::FieldKind;

    fn a_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn placement(origin: (f64, f64), current: (f64, f64)) -> FormPlacement {
        FormPlacement {
            kind: FieldKind::Text,
            page_index: 0,
            origin,
            current,
        }
    }

    #[test]
    fn a_drag_traces_the_rect_the_pointer_swept() {
        let rect = traced_rect(&placement((120.0, 400.0), (300.0, 460.0)));

        assert_eq!((rect.x, rect.y), (120.0, 400.0));
        assert_eq!((rect.width, rect.height), (180.0, 60.0));
    }

    #[test]
    fn a_click_commits_the_default_sized_rect_anchored_at_the_pointer() {
        let rect = committed_rect(&placement((200.0, 500.0), (200.0, 500.0)));
        let (width, height) = CLICK_SIZE_PT;

        assert_eq!((rect.width, rect.height), (width, height));
        assert_eq!((rect.x, rect.y), (200.0, 500.0 - height));
    }

    #[test]
    fn a_press_that_barely_moves_is_treated_as_a_click() {
        let rect = committed_rect(&placement((200.0, 500.0), (201.0, 500.5)));

        assert_eq!((rect.width, rect.height), CLICK_SIZE_PT);
    }

    #[test]
    fn a_real_drag_commits_the_traced_rect_not_the_click_default() {
        let rect = committed_rect(&placement((120.0, 400.0), (300.0, 460.0)));

        assert_ne!((rect.width, rect.height), CLICK_SIZE_PT);
        assert_eq!((rect.width, rect.height), (180.0, 60.0));
    }

    #[test]
    fn a_flat_sweep_still_leaves_something_selectable() {
        let rect = traced_rect(&placement((100.0, 500.0), (400.0, 500.0)));

        assert_eq!(rect.width, 300.0);
        assert_eq!(rect.height, MIN_TRACED_PT);
    }

    #[test]
    fn a_press_inside_a_fields_body_hits_it_and_one_outside_does_not() {
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

    fn a_field_drag(
        mode: AnnotationDragMode,
        origin: (f64, f64),
        current: (f64, f64),
    ) -> FormFieldDrag {
        FormFieldDrag {
            id: pdf_document::FormFieldId(1),
            mode,
            origin,
            current,
        }
    }

    #[test]
    fn a_move_drag_translates_the_bbox_by_the_pointer_delta() {
        let bbox = a_rect(100.0, 500.0, 200.0, 40.0);
        let drag = a_field_drag(AnnotationDragMode::Move, (150.0, 520.0), (170.0, 495.0));

        let moved = dragged_rect(bbox, &drag);

        assert_eq!((moved.x, moved.y), (120.0, 475.0));
        assert_eq!((moved.width, moved.height), (200.0, 40.0));
    }

    #[test]
    fn a_resize_drag_holds_the_opposite_corner_still() {
        let bbox = a_rect(100.0, 500.0, 200.0, 40.0);
        let drag = a_field_drag(
            AnnotationDragMode::Resize(Corner::TopRight),
            (300.0, 540.0),
            (400.0, 600.0),
        );

        let resized = dragged_rect(bbox, &drag);

        assert_eq!(
            (resized.x, resized.y),
            (100.0, 500.0),
            "anchor must not move"
        );
        assert_eq!((resized.width, resized.height), (300.0, 100.0));
    }

    #[test]
    fn dragging_a_corner_past_its_opposite_flips_rather_than_inverts() {
        let bbox = a_rect(100.0, 500.0, 200.0, 40.0);
        let drag = a_field_drag(
            AnnotationDragMode::Resize(Corner::TopRight),
            (300.0, 540.0),
            (40.0, 460.0),
        );

        let flipped = dragged_rect(bbox, &drag);

        assert!(flipped.width > 0.0 && flipped.height > 0.0);
        assert_eq!((flipped.x, flipped.y), (40.0, 460.0));
    }
}
