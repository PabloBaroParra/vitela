//! Annotation edit operations (T-028): move, resize, restyle, delete.
//!
//! These operate directly on `pdf_document::Annotation` / `AnnotationSet`
//! values. Wiring these into `EditLog` commands (for undo/redo) is a later
//! integration concern outside this batch's scope — `pdf-document`'s
//! `Command` enum is `#[non_exhaustive]` precisely so new variants (e.g.
//! `MoveAnnotation`) can be added later without a breaking change.

use crate::error::AnnotateError;
use crate::freetext::{checked_contents, is_drawable, min_free_text_size};
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
        | AnnotationKind::FreeText { rect, .. }
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
        AnnotationKind::FreeText { rect, style, .. } => {
            // A box too small to hold one line would clip every glyph, so
            // the floor is refused here rather than left to each shell.
            let (min_width, min_height) = min_free_text_size(style);
            if !is_drawable(&new_rect) || new_rect.width < min_width || new_rect.height < min_height
            {
                return Err(AnnotateError::InvalidRect);
            }
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

/// Replaces the text of a `FreeText` annotation.
///
/// Runs the same validation as [`crate::builders::free_text`] — blank text
/// and characters WinAnsi cannot show are refused with the annotation left
/// untouched. Setting the text it already has is accepted and changes
/// nothing; callers that record undo entries compare before and after to
/// avoid logging an edit that edited nothing. Any other kind is
/// `UnsupportedOperation`.
pub fn set_annotation_contents(
    annotation: &mut Annotation,
    new_contents: &str,
) -> Result<(), AnnotateError> {
    let AnnotationKind::FreeText { contents, .. } = &mut annotation.kind else {
        return Err(AnnotateError::UnsupportedOperation("set contents"));
    };
    *contents = checked_contents(new_contents)?;
    Ok(())
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

    fn free_text_annotation(contents: &str) -> Annotation {
        Annotation {
            id: AnnotationId(5),
            page: PageId(0),
            kind: AnnotationKind::FreeText {
                rect: Rect {
                    x: 100.0,
                    y: 100.0,
                    width: 200.0,
                    height: 50.0,
                },
                contents: contents.to_string(),
                style: crate::freetext::default_free_text_style(),
            },
        }
    }

    fn free_text_rect(annotation: &Annotation) -> Rect {
        match &annotation.kind {
            AnnotationKind::FreeText { rect, .. } => *rect,
            other => panic!("expected FreeText, got {other:?}"),
        }
    }

    fn free_text_contents(annotation: &Annotation) -> &str {
        match &annotation.kind {
            AnnotationKind::FreeText { contents, .. } => contents,
            other => panic!("expected FreeText, got {other:?}"),
        }
    }

    #[test]
    fn move_translates_a_free_text_box_and_keeps_its_size() {
        let mut annotation = free_text_annotation("hola");
        move_annotation(&mut annotation, 10.0, -20.0).expect("free text supports move");
        let rect = free_text_rect(&annotation);
        assert_eq!((rect.x, rect.y), (110.0, 80.0));
        assert_eq!((rect.width, rect.height), (200.0, 50.0));
    }

    #[test]
    fn resize_replaces_a_free_text_rect() {
        let mut annotation = free_text_annotation("hola");
        let narrower = Rect {
            x: 100.0,
            y: 100.0,
            width: 80.0,
            height: 50.0,
        };
        resize_annotation(&mut annotation, narrower).expect("free text supports resize");
        assert_eq!(free_text_rect(&annotation), narrower);
    }

    #[test]
    fn resize_refuses_a_degenerate_free_text_rect_and_changes_nothing() {
        for (width, height) in [(0.0, 50.0), (-10.0, 50.0), (80.0, 0.0), (80.0, -1.0)] {
            let mut annotation = free_text_annotation("hola");
            let before = annotation.clone();
            let bad = Rect {
                x: 100.0,
                y: 100.0,
                width,
                height,
            };
            assert_eq!(
                resize_annotation(&mut annotation, bad),
                Err(AnnotateError::InvalidRect),
                "{width}x{height}"
            );
            assert_eq!(annotation, before);
        }
    }

    #[test]
    fn resize_refuses_a_free_text_rect_below_one_glyph_line() {
        let min = crate::freetext::min_free_text_size(&crate::freetext::default_free_text_style());
        let mut annotation = free_text_annotation("hola");
        let too_small = Rect {
            x: 0.0,
            y: 0.0,
            width: min.0 - 0.5,
            height: 50.0,
        };
        assert_eq!(
            resize_annotation(&mut annotation, too_small),
            Err(AnnotateError::InvalidRect)
        );
        let just_enough = Rect {
            x: 0.0,
            y: 0.0,
            width: min.0,
            height: min.1,
        };
        resize_annotation(&mut annotation, just_enough).expect("the minimum itself is allowed");
        assert_eq!(free_text_rect(&annotation), just_enough);
    }

    #[test]
    fn restyle_free_text_stays_unsupported() {
        let mut annotation = free_text_annotation("hola");
        assert!(matches!(
            restyle_annotation(&mut annotation, color()),
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn set_contents_replaces_the_text() {
        let mut annotation = free_text_annotation("uno");
        set_annotation_contents(&mut annotation, "dos").expect("free text takes new text");
        assert_eq!(free_text_contents(&annotation), "dos");
    }

    #[test]
    fn set_contents_accepts_accents_and_normalizes_line_breaks() {
        let mut annotation = free_text_annotation("uno");
        set_annotation_contents(&mut annotation, "ñandú\r\nÁrbol").unwrap();
        assert_eq!(free_text_contents(&annotation), "ñandú\nÁrbol");
    }

    #[test]
    fn set_contents_refuses_blank_and_unencodable_text_and_changes_nothing() {
        let mut annotation = free_text_annotation("uno");
        let before = annotation.clone();
        assert!(matches!(
            set_annotation_contents(&mut annotation, "  \n"),
            Err(AnnotateError::UnsupportedOperation(_))
        ));
        assert_eq!(
            set_annotation_contents(&mut annotation, "uno 日"),
            Err(AnnotateError::EncodingGap { character: '日' })
        );
        assert_eq!(annotation, before);
    }

    #[test]
    fn set_contents_on_a_text_note_is_unsupported() {
        let mut annotation = text_note_annotation();
        assert!(matches!(
            set_annotation_contents(&mut annotation, "dos"),
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn set_contents_to_the_same_text_is_an_accepted_no_op() {
        let mut annotation = free_text_annotation("uno");
        let before = annotation.clone();
        set_annotation_contents(&mut annotation, "uno").expect("same text is not an error");
        assert_eq!(annotation, before);
    }
}
