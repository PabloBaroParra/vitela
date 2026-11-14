//! Underline and strikethrough: a thin filled box in the run's fill colour,
//! under or through its advance, painted in a stream of its own after the
//! page's content — and found again later.
//!
//! A path cannot be painted inside a text object, so the line is not written
//! next to the show operation but appended to the page, in the run's own
//! coordinate system: the page's leftover transform cancelled, then the
//! run's CTM and (upright) text matrix, so a rotated or scaled run gets a
//! rotated or scaled line. The colour is written as device RGB because a
//! colour-space name from a form's resources means nothing at page level.
//!
//! ## Found again by a tag
//!
//! Each line is wrapped in marked content of its own, `/VUnderline BMC … EMC`
//! or `/VStrike BMC … EMC`. Nothing else in a file ties a line to the run it
//! decorates — a producer's underline is just another path, exactly like a
//! table rule — so only a tagged line is read as a run's decoration: it
//! counts when it lies where this crate would draw that run's line (its
//! start and its thickness, not its length, which a retype changes). An
//! untagged line is never reported and never removed.
//!
//! A tagged line moves with its run ([`crate::move_text_run`]) and goes with
//! it ([`crate::remove_text_run`]). A retype keeps the line as long as it
//! was: it is still the run's, and toggling it off and on again redraws it at
//! the new width.

use std::ops::Range;

use lopdf::{Document, ObjectId};
use pdf_document::Rect;

use crate::edit::format_number;
use crate::error::EditError;
use crate::parse::interpreter::{LocatedContent, LocatedTextRun};
use crate::parse::matrix::Matrix;
use crate::parse::{tokenize, Operand};

/// Where an underline sits and how thick it is, in fractions of the font
/// size below / above the baseline.
const UNDERLINE_TOP: f64 = -0.1;
const STRIKE_TOP: f64 = 0.3;
const THICKNESS: f64 = 0.05;

/// How far, in points, a tagged line may sit from where its run's line
/// would be drawn and still be that run's.
const TOLERANCE: f64 = 0.5;

/// The two lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Line {
    Underline,
    Strike,
}

impl Line {
    pub(crate) const ALL: [Line; 2] = [Line::Underline, Line::Strike];

    fn tag(self) -> &'static str {
        match self {
            Line::Underline => "VUnderline",
            Line::Strike => "VStrike",
        }
    }

    fn top(self) -> f64 {
        match self {
            Line::Underline => UNDERLINE_TOP,
            Line::Strike => STRIKE_TOP,
        }
    }

    fn of_tag(tag: &str) -> Option<Line> {
        Line::ALL.into_iter().find(|line| line.tag() == tag)
    }
}

/// What the lines of one run are drawn from.
pub(crate) struct Placement {
    /// Cancels whatever CTM the page's own streams leave behind.
    pub(crate) correction: Matrix,
    pub(crate) ctm: Matrix,
    pub(crate) text_matrix: Matrix,
    pub(crate) font_size: f64,
    /// The run's advance in text space, after any font change.
    pub(crate) advance: f64,
    pub(crate) fill_rgb: [f64; 3],
}

/// The line's box in the run's text space: from its origin across `advance`.
fn text_space_box(line: Line, font_size: f64, advance: f64) -> Rect {
    let size = font_size.abs();
    let thickness = THICKNESS * size;
    Rect {
        x: 0.0,
        y: line.top() * size - thickness,
        width: advance,
        height: thickness,
    }
}

/// The text matrix a line is drawn under: the run's, straightened — a slant
/// leans the glyphs, never the line under them.
fn upright(text_matrix: Matrix) -> Matrix {
    super::synthetic::with_shear(text_matrix, 0.0)
}

/// `/VUnderline BMC q … re f Q EMC`: one tagged line under / through the run.
pub(crate) fn operators(line: Line, at: &Placement) -> Vec<u8> {
    let mut out = format!("\n/{} BMC q ", line.tag()).into_bytes();
    for matrix in [at.correction, at.ctm, upright(at.text_matrix)] {
        out.extend_from_slice(crate::insert::matrix_operator(matrix).as_bytes());
        out.push(b' ');
    }
    let [r, g, b] = at.fill_rgb;
    let rect = text_space_box(line, at.font_size, at.advance);
    out.extend_from_slice(
        format!(
            "{} {} {} rg {} {} {} {} re f Q EMC\n",
            format_number(r),
            format_number(g),
            format_number(b),
            format_number(rect.x),
            format_number(rect.y),
            format_number(rect.width),
            format_number(rect.height),
        )
        .as_bytes(),
    );
    out
}

/// A tagged line on the page.
#[derive(Debug, Clone)]
pub(crate) struct Found {
    pub(crate) line: Line,
    pub(crate) stream_index: usize,
    /// `BMC` through `EMC`: what a removal takes out.
    pub(crate) span: Range<usize>,
    /// Just past the line's first `cm` (the correction): where a page-space
    /// translation goes to move it.
    pub(crate) translate_at: usize,
    /// The line's box in page space.
    pub(crate) bbox: Rect,
}

/// Every tagged line painted straight onto the page, which is the only place
/// this crate writes them.
pub(crate) fn find(located: &LocatedContent) -> Vec<Found> {
    let mut found = Vec::new();
    for (stream_index, stream) in located.streams.iter().enumerate() {
        let Ok(operations) = tokenize(&stream.bytes) else {
            continue;
        };
        let mut open: Option<(Line, usize, Option<usize>)> = None;
        for operation in &operations {
            match (operation.operator.as_str(), &mut open) {
                ("BMC", None) => {
                    if let Some(Operand::Name(tag)) = operation.operands.first() {
                        if let Some(line) = Line::of_tag(tag) {
                            open = Some((line, operation.span.start, None));
                        }
                    }
                }
                ("cm", Some((_, _, translate_at @ None))) => {
                    *translate_at = Some(operation.span.end);
                }
                ("EMC", Some(_)) => {
                    let (line, start, translate_at) = open.take().expect("matched Some");
                    let span = start..operation.span.end;
                    let bbox = located
                        .paths
                        .iter()
                        .find(|path| {
                            path.form_path.is_empty()
                                && path.stream_index == stream_index
                                && span.contains(&path.operation_span.start)
                        })
                        .map(|path| path.item.bbox);
                    if let (Some(translate_at), Some(bbox)) = (translate_at, bbox) {
                        found.push(Found {
                            line,
                            stream_index,
                            span,
                            translate_at,
                            bbox,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    found
}

/// The tagged lines `found` holds for `run`: each lying where this crate
/// would draw that line under it now.
pub(crate) fn of_run<'a>(found: &'a [Found], run: &LocatedTextRun) -> Vec<&'a Found> {
    let placement = &run.placement;
    let transform = upright(placement.text_matrix).then(placement.ctm);
    found
        .iter()
        .filter(|candidate| {
            let rect = text_space_box(candidate.line, placement.font_size, placement.advance);
            let expected = transform.bounding_box(rect.x, rect.y, rect.width, rect.height);
            let near = |a: f64, b: f64| (a - b).abs() <= TOLERANCE;
            let bbox = candidate.bbox;
            near(bbox.x, expected.x)
                && near(bbox.y, expected.y)
                && (near(bbox.height, expected.height) || near(bbox.width, expected.width))
        })
        .collect()
}

/// Takes `lines` off the page, or — with `shift` — moves them by that much
/// in page space. Each page stream holding one is rewritten once, in a
/// page-owned copy.
pub(crate) fn rewrite(
    document: &mut Document,
    page_object: ObjectId,
    located: &LocatedContent,
    lines: &[&Found],
    shift: Option<(f64, f64)>,
) -> Result<(), EditError> {
    let mut streams: Vec<usize> = lines.iter().map(|line| line.stream_index).collect();
    streams.sort_unstable();
    streams.dedup();
    for stream_index in streams {
        let Some(stream) = located.streams.get(stream_index) else {
            continue;
        };
        let mut edits: Vec<(Range<usize>, Vec<u8>)> = lines
            .iter()
            .filter(|line| line.stream_index == stream_index)
            .map(|line| match shift {
                None => (line.span.clone(), Vec::new()),
                Some((dx, dy)) => (
                    line.translate_at..line.translate_at,
                    format!(" 1 0 0 1 {} {} cm", format_number(dx), format_number(dy)).into_bytes(),
                ),
            })
            .collect();
        edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));
        let mut bytes = stream.bytes.clone();
        for (span, replacement) in edits {
            bytes.splice(span, replacement);
        }
        let destination =
            crate::edit::own_page_stream(document, page_object, stream_index, stream.object_id)?;
        crate::edit::splice(document, destination, stream, 0..stream.bytes.len(), &bytes)?;
    }
    Ok(())
}
