//! Annotation edit operations (T-028): move, resize, restyle, delete.
//!
//! These operate directly on `pdf_document::Annotation` / `AnnotationSet`
//! values. Wiring these into `EditLog` commands (for undo/redo) is a later
//! integration concern outside this batch's scope — `pdf-document`'s
//! `Command` enum is `#[non_exhaustive]` precisely so new variants (e.g.
//! `MoveAnnotation`) can be added later without a breaking change.

use crate::error::AnnotateError;
use pdf_document::{Annotation, AnnotationId, AnnotationKind, AnnotationSet, Color, Rect};

/// Translates a rect-based annotation by `(dx, dy)`, or shifts every point
/// of an `Ink` annotation by the same delta.
///
/// Returns `Err(AnnotateError::UnsupportedOperation)` for kinds that carry
/// neither a `rect` nor `points` (there are none today, but the match is
/// exhaustive-by-wildcard because `AnnotationKind` is `#[non_exhaustive]`).
pub fn move_annotation(annotation: &mut Annotation, dx: f64, dy: f64) -> Result<(), AnnotateError> {
    match &mut annotation.kind {
        AnnotationKind::Highlight { rect, .. }
        | AnnotationKind::Underline { rect, .. }
        | AnnotationKind::Strikeout { rect, .. }
        | AnnotationKind::Shape { rect, .. }
        | AnnotationKind::TextNote { rect, .. }
        | AnnotationKind::Stamp { rect, .. } => {
            rect.x += dx;
            rect.y += dy;
            Ok(())
        }
        AnnotationKind::Ink { points, .. } => {
            for point in points.iter_mut() {
                point.0 += dx;
                point.1 += dy;
            }
            Ok(())
        }
        _ => Err(AnnotateError::UnsupportedOperation("move")),
    }
}

/// Replaces the bounding `rect` of a rect-based annotation.
///
/// `Ink` has no single `rect` to replace (its extent is derived from its
/// points), so resizing an `Ink` annotation returns
/// `Err(AnnotateError::UnsupportedOperation)`.
pub fn resize_annotation(annotation: &mut Annotation, new_rect: Rect) -> Result<(), AnnotateError> {
    match &mut annotation.kind {
        AnnotationKind::Highlight { rect, .. }
        | AnnotationKind::Underline { rect, .. }
        | AnnotationKind::Strikeout { rect, .. }
        | AnnotationKind::Shape { rect, .. }
        | AnnotationKind::TextNote { rect, .. }
        | AnnotationKind::Stamp { rect, .. } => {
            *rect = new_rect;
            Ok(())
        }
        _ => Err(AnnotateError::UnsupportedOperation("resize")),
    }
}

/// Replaces the color of a colored annotation (`Highlight`, `Underline`,
/// `Strikeout`, `Ink`, `Shape`).
///
/// `TextNote` and `Stamp` carry no color field, so restyling either returns
/// `Err(AnnotateError::UnsupportedOperation)`.
pub fn restyle_annotation(
    annotation: &mut Annotation,
    new_color: Color,
) -> Result<(), AnnotateError> {
    match &mut annotation.kind {
        AnnotationKind::Highlight { color, .. }
        | AnnotationKind::Underline { color, .. }
        | AnnotationKind::Strikeout { color, .. }
        | AnnotationKind::Ink { color, .. }
        | AnnotationKind::Shape { color, .. } => {
            *color = new_color;
            Ok(())
        }
        _ => Err(AnnotateError::UnsupportedOperation("restyle")),
    }
}

/// Removes and returns the annotation with the given id from `set`.
///
/// Thin wrapper around [`AnnotationSet::remove`] — kept here so callers have
/// one cohesive "annotation ops" surface (move/resize/restyle/delete)
/// instead of reaching into `pdf-document` directly for the delete half.
pub fn delete_annotation(set: &mut AnnotationSet, id: AnnotationId) -> Option<Annotation> {
    set.remove(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{PageId, Popup};

    fn rect() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        }
    }

    fn color() -> Color {
        Color { r: 1, g: 2, b: 3 }
    }

    fn highlight_annotation() -> Annotation {
        Annotation {
            id: AnnotationId(1),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: rect(),
                color: color(),
            },
        }
    }

    fn ink_annotation() -> Annotation {
        Annotation {
            id: AnnotationId(2),
            page: PageId(0),
            kind: AnnotationKind::Ink {
                points: vec![(0.0, 0.0), (1.0, 1.0)],
                color: color(),
            },
        }
    }

    fn stamp_annotation() -> Annotation {
        Annotation {
            id: AnnotationId(3),
            page: PageId(0),
            kind: AnnotationKind::Stamp {
                rect: rect(),
                image_bytes: vec![],
                has_alpha: false,
            },
        }
    }

    fn text_note_annotation() -> Annotation {
        Annotation {
            id: AnnotationId(4),
            page: PageId(0),
            kind: AnnotationKind::TextNote {
                rect: rect(),
                contents: "note".to_string(),
                popup: Popup {
                    is_open: false,
                    contents: "note".to_string(),
                },
            },
        }
    }

    #[test]
    fn move_shifts_rect_based_annotation() {
        let mut annotation = highlight_annotation();
        move_annotation(&mut annotation, 5.0, -3.0).expect("highlight supports move");
        match annotation.kind {
            AnnotationKind::Highlight { rect, .. } => {
                assert_eq!(rect.x, 5.0);
                assert_eq!(rect.y, -3.0);
            }
            other => panic!("expected Highlight, got {other:?}"),
        }
    }

    #[test]
    fn move_shifts_every_ink_point() {
        let mut annotation = ink_annotation();
        move_annotation(&mut annotation, 2.0, 3.0).expect("ink supports move");
        match annotation.kind {
            AnnotationKind::Ink { points, .. } => {
                assert_eq!(points, vec![(2.0, 3.0), (3.0, 4.0)]);
            }
            other => panic!("expected Ink, got {other:?}"),
        }
    }

    #[test]
    fn resize_replaces_rect() {
        let mut annotation = stamp_annotation();
        let new_rect = Rect {
            x: 1.0,
            y: 1.0,
            width: 20.0,
            height: 30.0,
        };
        resize_annotation(&mut annotation, new_rect).expect("stamp supports resize");
        match annotation.kind {
            AnnotationKind::Stamp { rect, .. } => assert_eq!(rect, new_rect),
            other => panic!("expected Stamp, got {other:?}"),
        }
    }

    #[test]
    fn resize_ink_is_unsupported() {
        let mut annotation = ink_annotation();
        let result = resize_annotation(&mut annotation, rect());
        assert!(matches!(
            result,
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn restyle_replaces_color() {
        let mut annotation = highlight_annotation();
        let new_color = Color { r: 9, g: 9, b: 9 };
        restyle_annotation(&mut annotation, new_color).expect("highlight supports restyle");
        match annotation.kind {
            AnnotationKind::Highlight { color, .. } => assert_eq!(color, new_color),
            other => panic!("expected Highlight, got {other:?}"),
        }
    }

    #[test]
    fn restyle_stamp_is_unsupported() {
        let mut annotation = stamp_annotation();
        let result = restyle_annotation(&mut annotation, color());
        assert!(matches!(
            result,
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn restyle_text_note_is_unsupported() {
        let mut annotation = text_note_annotation();
        let result = restyle_annotation(&mut annotation, color());
        assert!(matches!(
            result,
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn delete_removes_from_set() {
        let mut set = AnnotationSet::new();
        set.insert(highlight_annotation());

        let removed = delete_annotation(&mut set, AnnotationId(1));
        assert!(removed.is_some());
        assert!(set.is_empty());
    }

    #[test]
    fn delete_missing_id_returns_none() {
        let mut set = AnnotationSet::new();
        assert!(delete_annotation(&mut set, AnnotationId(999)).is_none());
    }
}
