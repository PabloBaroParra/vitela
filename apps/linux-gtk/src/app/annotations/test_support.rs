//! Shared fixtures for the annotation unit tests.
//!
//! `drag` and `click` describe a gesture the way the pointer would, and
//! [`built`] runs it through the same builder the real placement uses — so a
//! test asserts on what the user would actually get rather than on a hand-made
//! rect that could agree with nothing.

use pdf_document::{Annotation, AnnotationId, AnnotationKind, PageId, Rect};

use crate::app::state::{Placement, Tool};

use super::builder::annotation_at;
use super::geometry::committed_rect;

/// A placement drag from `origin` to `end` on page 0, recording every
/// point the way the gesture does for freehand tools.
pub(super) fn drag(tool: Tool, origin: (f64, f64), end: (f64, f64)) -> Placement {
    Placement {
        tool,
        page_index: 0,
        origin,
        current: end,
        points: if tool.is_freehand() {
            vec![origin, end]
        } else {
            Vec::new()
        },
    }
}

/// A click: pointer down and up in the same spot, no drag.
pub(super) fn click(tool: Tool, at: (f64, f64)) -> Placement {
    Placement {
        tool,
        page_index: 0,
        origin: at,
        current: at,
        points: if tool.is_freehand() { vec![at] } else { vec![] },
    }
}

pub(super) fn built(placement: &Placement) -> Annotation {
    annotation_at(
        placement,
        AnnotationId(1),
        PageId(0),
        committed_rect(placement),
    )
    .unwrap_or_else(|error| panic!("{:?} must build from a placement: {error}", placement.tool))
}

pub(super) fn rect_of(annotation: &Annotation) -> Rect {
    match &annotation.kind {
        AnnotationKind::Highlight { rect, .. }
        | AnnotationKind::Underline { rect, .. }
        | AnnotationKind::Strikeout { rect, .. }
        | AnnotationKind::Shape { rect, .. }
        | AnnotationKind::TextNote { rect, .. }
        | AnnotationKind::Stamp { rect, .. } => *rect,
        _ => panic!("kind has no rect"),
    }
}
