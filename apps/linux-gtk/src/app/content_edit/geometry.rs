//! Pure hit-test/drag math for images in content-edit mode — the image twin
//! of `annotations::geometry`, forked rather than shared per T-162's design:
//! images are page content, not `document.annotations`, and generalizing the
//! two would blur that boundary for no shared behavior beyond the shape.
//!
//! Everything here is a pure function over rects and points — no GTK, no
//! session state — same posture as `content_edit::model`.

use pdf_document::Rect;

use crate::app::state::{AnnotationDragMode, Corner, ImageDrag};

/// Floor for each side of a resized image, mirroring
/// `annotations::geometry::MIN_TRACED_PT`: without it, dragging a corner
/// handle past its opposite could shrink an image to a zero-width or
/// zero-height rect that paints nothing and can never be grabbed again.
const MIN_IMAGE_PT: f64 = 4.0;

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
/// `reach` is the grab radius in PDF points, derived by the caller from the
/// zoom so the handle stays the same size under the pointer however far the
/// page is scaled — mirrors `annotations::geometry::corner_at`.
pub(crate) fn corner_at(rect: Rect, point: (f64, f64), reach: f64) -> Option<Corner> {
    Corner::ALL.into_iter().find(|corner| {
        let (corner_x, corner_y) = corner_point(rect, *corner);
        (point.0 - corner_x).abs() <= reach && (point.1 - corner_y).abs() <= reach
    })
}

/// Whether `point` is inside the image's body. Edges count as inside.
pub(crate) fn contains(rect: Rect, point: (f64, f64)) -> bool {
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
/// corner past its opposite flips the rect rather than inverting it — mirrors
/// `annotations::geometry::resized_rect`.
fn resized_rect(rect: Rect, corner: Corner, point: (f64, f64)) -> Rect {
    let (anchor_x, anchor_y) = corner_point(rect, opposite(corner));
    Rect {
        x: anchor_x.min(point.0),
        y: anchor_y.min(point.1),
        width: (point.0 - anchor_x).abs().max(MIN_IMAGE_PT),
        height: (point.1 - anchor_y).abs().max(MIN_IMAGE_PT),
    }
}

/// Applies a drag in progress to `bbox`, producing the rect that would be
/// committed if the drag ended now — the image twin of
/// `annotations::geometry::dragged`, but over a plain [`Rect`] rather than an
/// `Annotation`: an image has no kind that can refuse a resize the way ink
/// does, so this never actually fails. `Option` is kept in the signature to
/// mirror the annotation twin's shape and leave room for a future placement
/// mode that can.
pub(crate) fn dragged_rect(bbox: Rect, drag: &ImageDrag) -> Option<Rect> {
    match drag.mode {
        AnnotationDragMode::Move => {
            let dx = drag.current.0 - drag.origin.0;
            let dy = drag.current.1 - drag.origin.1;
            Some(Rect {
                x: bbox.x + dx,
                y: bbox.y + dy,
                width: bbox.width,
                height: bbox.height,
            })
        }
        AnnotationDragMode::Resize(corner) => Some(resized_rect(bbox, corner, drag.current)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::{AnnotationDragMode, Corner, ImageDrag};
    use pdf_document::{ContentItemId, ImageItem, PageId};

    fn a_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn sample_item() -> ImageItem {
        ImageItem {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: a_rect(100.0, 500.0, 200.0, 40.0),
            resource_xobject_name: "Im1".to_string(),
        }
    }

    #[test]
    fn a_press_inside_an_images_body_hits_it_and_one_outside_does_not() {
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

    #[test]
    fn a_move_drag_translates_the_bbox_by_the_pointer_delta() {
        let bbox = a_rect(100.0, 500.0, 200.0, 40.0);
        let drag = ImageDrag {
            page_index: 0,
            item: sample_item(),
            mode: AnnotationDragMode::Move,
            origin: (150.0, 520.0),
            current: (170.0, 495.0),
        };

        let moved = dragged_rect(bbox, &drag).expect("a move always applies");

        assert_eq!((moved.x, moved.y), (120.0, 475.0));
        assert_eq!((moved.width, moved.height), (200.0, 40.0));
    }

    /// Pulling a corner must hold the opposite one still — otherwise the
    /// image slides while it resizes and the user chases it.
    #[test]
    fn a_resize_drag_holds_the_opposite_corner_still() {
        let bbox = a_rect(100.0, 500.0, 200.0, 40.0);
        let drag = ImageDrag {
            page_index: 0,
            item: sample_item(),
            mode: AnnotationDragMode::Resize(Corner::TopRight),
            origin: (300.0, 540.0),
            current: (400.0, 600.0),
        };

        let resized = dragged_rect(bbox, &drag).expect("a resize always applies");

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
        let drag = ImageDrag {
            page_index: 0,
            item: sample_item(),
            mode: AnnotationDragMode::Resize(Corner::TopRight),
            origin: (300.0, 540.0),
            current: (40.0, 460.0),
        };

        let flipped = dragged_rect(bbox, &drag).expect("a resize always applies");

        assert!(flipped.width > 0.0 && flipped.height > 0.0);
        assert_eq!((flipped.x, flipped.y), (40.0, 460.0));
    }
}
