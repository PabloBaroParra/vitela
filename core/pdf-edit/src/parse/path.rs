//! Vector paths as content items (B25 P1, T-206).
//!
//! A path is a run of construction operators (`m`, `l`, `c`, `v`, `y`, `re`,
//! `h`) closed by one painting operator (`S`, `f`, `B`, …). In a real file
//! that is what a table border, a rule or a shaded box is — loose operators,
//! no object — so this is where they become something a shell can select.
//!
//! The interpreter feeds every operator through [`PathBuilder`] and gets a
//! [`LocatedPath`] back when one is painted. The segments are kept in **user
//! space**, exactly as written, because the writer regenerates the span from
//! them: a move or resize rewrites the coordinates rather than prefixing a
//! `cm`, which is what keeps a stretched frame's line width unchanged.
//!
//! ## What is not an item
//!
//! - A path ended by `n`, which paints nothing.
//! - A path with `W`/`W*` before its painting operator. A clip outlives the
//!   path — it narrows everything painted after it until the enclosing `Q` —
//!   so moving or deleting it would change content that is not this item.
//! - A path with no points at all.

use super::lexer::{Operand, SpannedOperation};
use super::matrix::Matrix;
use super::FormStep;
use pdf_document::{ContentItemId, PathItem, PathPaint, Rect};
use std::ops::Range;

/// One construction operator, its operands in user space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSegment {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    /// `x1 y1 x2 y2 x3 y3 c`.
    CurveTo([f64; 6]),
    /// `x2 y2 x3 y3 v` — the first control point is the current point.
    CurveV([f64; 4]),
    /// `x1 y1 x3 y3 y` — the second control point is the end point.
    CurveY([f64; 4]),
    /// `x y w h re`.
    Rectangle(f64, f64, f64, f64),
    ClosePath,
}

/// A painted path plus where its bytes are.
#[derive(Debug, Clone)]
pub struct LocatedPath {
    pub item: PathItem,
    pub stream_index: usize,
    /// The Form invocations traversed from the page to this stream.
    pub form_path: Vec<FormStep>,
    /// From the first construction operator through the painting operator.
    pub operation_span: Range<usize>,
    pub segments: Vec<PathSegment>,
    /// The painting operator exactly as written (`S`, `f*`, `b`, …), so a
    /// rewrite keeps the fill rule and the implicit close it already had.
    pub paint_operator: String,
    /// The CTM the path was built under. `cm` is not allowed inside a path
    /// object (PDF 32000-1 8.2), so one matrix holds for every segment.
    pub ctm: Matrix,
    /// Operators that put back how the path looked — `gs`, fill and stroke
    /// colours, line width, caps, joins and dash — in the scope's own
    /// resource names (B25 T-209: copying a path out of its stream).
    pub paint_state: Vec<u8>,
}

/// A path under construction.
#[derive(Debug, Clone)]
pub(crate) struct PathBuilder {
    stream_index: usize,
    start: usize,
    segments: Vec<PathSegment>,
    clips: bool,
}

/// What an operator did to the path under construction.
pub(crate) enum PathStep {
    /// Not a path operator — the builder is untouched.
    Other,
    /// Consumed: a construction operator, or a painting one that produced no
    /// item (`n`, a clip, a malformed path).
    Consumed,
    /// A painted path.
    Painted {
        segments: Vec<PathSegment>,
        span: Range<usize>,
        paint: PathPaint,
        paint_operator: String,
    },
}

/// Feeds one operator to the path under construction.
///
/// A construction operator arriving in another stream than the one the path
/// began in starts over: `/Contents` streams concatenate, but a path split
/// across two of them has no single span a rewrite could replace.
pub(crate) fn step(
    builder: &mut Option<PathBuilder>,
    operation: &SpannedOperation,
    stream_index: usize,
) -> PathStep {
    let operator = operation.operator.as_str();
    if let Some(segment) = segment(operator, &operation.operands) {
        let current = match builder {
            Some(current) if current.stream_index == stream_index => current,
            _ => builder.insert(PathBuilder {
                stream_index,
                start: operation.span.start,
                segments: Vec::new(),
                clips: false,
            }),
        };
        current.segments.push(segment);
        return PathStep::Consumed;
    }

    match operator {
        "W" | "W*" => {
            if let Some(current) = builder {
                current.clips = true;
            }
            PathStep::Consumed
        }
        "n" => {
            *builder = None;
            PathStep::Consumed
        }
        _ => {
            let Some(paint) = paint_of(operator) else {
                // Anything else inside a path object is malformed, and a
                // span that held it could not be regenerated from segments
                // without dropping it — so the path is no longer an item.
                *builder = None;
                return PathStep::Other;
            };
            let Some(finished) = builder.take() else {
                return PathStep::Consumed;
            };
            if finished.clips
                || finished.stream_index != stream_index
                || !finished.segments.iter().any(has_points)
            {
                return PathStep::Consumed;
            }
            PathStep::Painted {
                segments: finished.segments,
                span: finished.start..operation.span.end,
                paint,
                paint_operator: operator.to_string(),
            }
        }
    }
}

fn paint_of(operator: &str) -> Option<PathPaint> {
    match operator {
        "S" | "s" => Some(PathPaint::Stroke),
        "f" | "F" | "f*" => Some(PathPaint::Fill),
        "B" | "B*" | "b" | "b*" => Some(PathPaint::FillAndStroke),
        _ => None,
    }
}

fn segment(operator: &str, operands: &[Operand]) -> Option<PathSegment> {
    let numbers = |count: usize| -> Option<Vec<f64>> {
        if operands.len() < count {
            return None;
        }
        operands[operands.len() - count..]
            .iter()
            .map(Operand::as_f64)
            .collect()
    };
    Some(match operator {
        "m" => {
            let n = numbers(2)?;
            PathSegment::MoveTo(n[0], n[1])
        }
        "l" => {
            let n = numbers(2)?;
            PathSegment::LineTo(n[0], n[1])
        }
        "c" => {
            let n = numbers(6)?;
            PathSegment::CurveTo([n[0], n[1], n[2], n[3], n[4], n[5]])
        }
        "v" => {
            let n = numbers(4)?;
            PathSegment::CurveV([n[0], n[1], n[2], n[3]])
        }
        "y" => {
            let n = numbers(4)?;
            PathSegment::CurveY([n[0], n[1], n[2], n[3]])
        }
        "re" => {
            let n = numbers(4)?;
            PathSegment::Rectangle(n[0], n[1], n[2], n[3])
        }
        "h" => PathSegment::ClosePath,
        _ => return None,
    })
}

fn has_points(segment: &PathSegment) -> bool {
    !matches!(segment, PathSegment::ClosePath)
}

/// Every point a segment names — control points included, which makes the
/// box conservative for a curve that does not reach them.
fn points(segment: &PathSegment) -> Vec<(f64, f64)> {
    match *segment {
        PathSegment::MoveTo(x, y) | PathSegment::LineTo(x, y) => vec![(x, y)],
        PathSegment::CurveTo([x1, y1, x2, y2, x3, y3]) => vec![(x1, y1), (x2, y2), (x3, y3)],
        PathSegment::CurveV([a, b, c, d]) | PathSegment::CurveY([a, b, c, d]) => {
            vec![(a, b), (c, d)]
        }
        PathSegment::Rectangle(x, y, w, h) => vec![(x, y), (x + w, y), (x, y + h), (x + w, y + h)],
        PathSegment::ClosePath => Vec::new(),
    }
}

/// The page-space box of `segments` drawn under `ctm`.
pub(crate) fn bounding_box(segments: &[PathSegment], ctm: Matrix) -> Rect {
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (x, y) in segments.iter().flat_map(points) {
        let (x, y) = ctm.apply(x, y);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

pub(crate) fn located_path(
    step: PathStep,
    ctm: Matrix,
    stream_index: usize,
    form_path: &[FormStep],
    index: usize,
) -> Option<LocatedPath> {
    let PathStep::Painted {
        segments,
        span,
        paint,
        paint_operator,
    } = step
    else {
        return None;
    };
    Some(LocatedPath {
        item: PathItem {
            id: ContentItemId(index as u64),
            page: super::interpreter::UNSTAMPED_PAGE,
            bbox: bounding_box(&segments, ctm),
            paint,
        },
        stream_index,
        form_path: form_path.to_vec(),
        operation_span: span,
        segments,
        paint_operator,
        ctm,
        paint_state: Vec::new(),
    })
}

/// The page-space map taking `from` onto `to`: scale about `from`'s corner,
/// then translate.
///
/// An axis `from` has no extent along — a horizontal rule's height — cannot
/// be scaled, only moved, so it keeps a factor of one rather than dividing by
/// zero.
pub(crate) fn page_mapping(from: Rect, to: Rect) -> Matrix {
    const FLAT: f64 = 1e-9;
    let scale_x = if from.width.abs() > FLAT {
        to.width / from.width
    } else {
        1.0
    };
    let scale_y = if from.height.abs() > FLAT {
        to.height / from.height
    } else {
        1.0
    };
    Matrix::translate(-from.x, -from.y)
        .then(Matrix::new(scale_x, 0.0, 0.0, scale_y, 0.0, 0.0))
        .then(Matrix::translate(to.x, to.y))
}

/// The box a path ends up with once [`page_mapping`] has taken `from` onto
/// `to` — `to` itself, except along an axis `from` is flat on, which stays
/// flat. What the pending overlay reports before the rewrite has happened.
pub fn reshaped_bbox(from: Rect, to: Rect) -> Rect {
    page_mapping(from, to).bounding_box(from.x, from.y, from.width, from.height)
}

/// The bytes that draw `segments` pushed through `user_map`, ended by
/// `paint_operator` — the replacement for the path's whole span.
///
/// A `re` stays a `re` while `user_map` keeps the axes (no rotation or skew
/// in it), which is every move and every resize of a path on an unrotated
/// CTM. Otherwise it is written out as the closed four-sided subpath it is
/// defined to be (PDF 32000-1 8.5.2.1), since a rotated rectangle has no `re`.
pub(crate) fn rewrite(segments: &[PathSegment], user_map: Matrix, paint_operator: &str) -> Vec<u8> {
    use crate::edit::format_number as n;

    let point = |x: f64, y: f64| {
        let (x, y) = user_map.apply(x, y);
        format!("{} {}", n(x), n(y))
    };
    let keeps_axes = user_map.b.abs() < 1e-12 && user_map.c.abs() < 1e-12;

    let mut out = String::new();
    for segment in segments {
        let written = match *segment {
            PathSegment::MoveTo(x, y) => format!("{} m", point(x, y)),
            PathSegment::LineTo(x, y) => format!("{} l", point(x, y)),
            PathSegment::CurveTo([x1, y1, x2, y2, x3, y3]) => {
                format!("{} {} {} c", point(x1, y1), point(x2, y2), point(x3, y3))
            }
            PathSegment::CurveV([x2, y2, x3, y3]) => {
                format!("{} {} v", point(x2, y2), point(x3, y3))
            }
            PathSegment::CurveY([x1, y1, x3, y3]) => {
                format!("{} {} y", point(x1, y1), point(x3, y3))
            }
            PathSegment::Rectangle(x, y, w, h) if keeps_axes => format!(
                "{} {} {} re",
                point(x, y),
                n(w * user_map.a),
                n(h * user_map.d)
            ),
            PathSegment::Rectangle(x, y, w, h) => format!(
                "{} m {} l {} l {} l h",
                point(x, y),
                point(x + w, y),
                point(x + w, y + h),
                point(x, y + h)
            ),
            PathSegment::ClosePath => "h".to_string(),
        };
        out.push_str(&written);
        out.push('\n');
    }
    out.push_str(paint_operator);
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_mapping_takes_one_box_onto_the_other() {
        let mapping = page_mapping(rect(10.0, 20.0, 100.0, 50.0), rect(0.0, 0.0, 200.0, 25.0));

        assert_eq!(mapping.apply(10.0, 20.0), (0.0, 0.0));
        assert_eq!(mapping.apply(110.0, 70.0), (200.0, 25.0));
    }

    /// A rule has no height to scale. Dividing by it would send every point
    /// to infinity; it moves instead.
    #[test]
    fn a_flat_axis_is_moved_not_scaled() {
        let mapping = page_mapping(rect(10.0, 20.0, 100.0, 0.0), rect(10.0, 30.0, 50.0, 0.0));

        assert_eq!(mapping.apply(110.0, 20.0), (60.0, 30.0));
    }

    #[test]
    fn a_rectangle_keeps_its_operator_under_an_axis_aligned_map() {
        let bytes = rewrite(
            &[PathSegment::Rectangle(1.0, 2.0, 3.0, 4.0)],
            Matrix::new(2.0, 0.0, 0.0, 3.0, 10.0, 0.0),
            "S",
        );

        assert_eq!(String::from_utf8(bytes).unwrap(), "12 6 6 12 re\nS");
    }

    #[test]
    fn a_rectangle_under_a_rotation_is_written_as_its_four_sides() {
        let bytes = rewrite(
            &[PathSegment::Rectangle(0.0, 0.0, 1.0, 1.0)],
            Matrix::new(0.0, 1.0, -1.0, 0.0, 0.0, 0.0),
            "f",
        );

        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "0 0 m 0 1 l -1 1 l -1 0 l h\nf"
        );
    }
}
