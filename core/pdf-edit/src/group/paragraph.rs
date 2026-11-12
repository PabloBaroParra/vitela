//! Lines stacked into paragraphs — the second pass of [`super::group_text_runs`].
//!
//! A line joins the paragraph above it when the two read as consecutive
//! lines of one piece of text:
//!
//! - same page, same font resource, sizes within [`SIZE_TOLERANCE`];
//! - the baseline step is a plausible leading, between [`MIN_LEADING`] and
//!   [`MAX_LEADING`] ems, and every step after the first stays within
//!   [`LEADING_TOLERANCE`] of it — a bigger gap is a paragraph break;
//! - the lines share an edge (left, centre or right, within
//!   [`EDGE_TOLERANCE`]), judged on the first pair and then required of
//!   every line, and overlap horizontally;
//! - no other line sits between them, and they are not two rows of a grid —
//!   a table, or columns of prose — where each has a neighbour on its own
//!   baseline and those neighbours stack too.
//!
//! Everything is measured in the paragraph's em: the height of its lines.

use super::{union, Alignment, Line, LINE_TOLERANCE};
use pdf_document::Rect;

/// How far a line's size may differ from the paragraph's, as a fraction of
/// the size — rounding in the generator, not a second size of type.
const SIZE_TOLERANCE: f64 = 0.1;

/// The tightest baseline step that is still two lines, in ems: set solid
/// with a little negative leading.
const MIN_LEADING: f64 = 0.9;

/// The widest baseline step that is still one paragraph, in ems.
///
/// Typeset text sits between 1.0 and 1.5; 2.4 also admits double spacing,
/// which the shipped sample uses (14 pt type on a 32 pt step, 2.29 em) for
/// a sentence that runs on over three lines. A blank line in such a layout
/// doubles the step, which the steady-leading rule then refuses.
const MAX_LEADING: f64 = 2.4;

/// How far a later step may drift from the first, in ems, and still be the
/// same leading.
const LEADING_TOLERANCE: f64 = 0.15;

/// How far two lines' shared edge may be out of line, in ems.
const EDGE_TOLERANCE: f64 = 0.5;

/// A paragraph being built: its lines (indices into the pass's lines) top to
/// bottom, the edge they share, and the first baseline step.
pub(super) struct Paragraph {
    pub(super) lines: Vec<usize>,
    pub(super) alignment: Alignment,
    step: Option<f64>,
}

/// Stacks `lines` into paragraphs. Every line lands in exactly one; a line
/// that joins nothing is a paragraph of one.
pub(super) fn stack(lines: &[Line]) -> Vec<Paragraph> {
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (&lines[a], &lines[b]);
        a.page
            .cmp(&b.page)
            .then(b.line.baseline.total_cmp(&a.line.baseline))
            .then(a.line.bbox.x.total_cmp(&b.line.bbox.x))
    });

    let mut paragraphs: Vec<Paragraph> = Vec::new();
    for index in order {
        let best = paragraphs
            .iter()
            .enumerate()
            .filter_map(|(at, paragraph)| {
                joins(lines, paragraph, index).map(|(step, alignment)| (at, step, alignment))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((at, step, alignment)) => {
                let paragraph = &mut paragraphs[at];
                paragraph.lines.push(index);
                paragraph.step.get_or_insert(step);
                paragraph.alignment = alignment;
            }
            None => paragraphs.push(Paragraph {
                lines: vec![index],
                alignment: Alignment::Left,
                step: None,
            }),
        }
    }
    paragraphs
}

/// The baseline step and shared edge when line `next` continues
/// `paragraph`; `None` when it does not.
fn joins(lines: &[Line], paragraph: &Paragraph, next: usize) -> Option<(f64, Alignment)> {
    let last_index = *paragraph.lines.last()?;
    let (first, last, below) = (&lines[paragraph.lines[0]], &lines[last_index], &lines[next]);
    let em = last.height;
    if em <= 0.0
        || last.page != below.page
        || last.font != below.font
        || (below.height - em).abs() > SIZE_TOLERANCE * em
    {
        return None;
    }

    let step = last.line.baseline - below.line.baseline;
    if !(MIN_LEADING * em..=MAX_LEADING * em).contains(&step) {
        return None;
    }
    if paragraph
        .step
        .is_some_and(|first_step| (step - first_step).abs() > LEADING_TOLERANCE * em)
    {
        return None;
    }
    if overlap(last.line.bbox, below.line.bbox) <= 0.0 {
        return None;
    }

    let alignment = match paragraph.step {
        None => shared_edge(first.line.bbox, below.line.bbox, em)?,
        Some(_) => {
            let kept = paragraph.alignment;
            aligned(kept, first.line.bbox, below.line.bbox, em).then_some(kept)?
        }
    };

    if line_between(lines, last_index, next) || rows_of_a_grid(lines, last_index, next) {
        return None;
    }
    Some((step, alignment))
}

/// The edge two lines share, preferring left, then centre, then right — a
/// pair of equally wide lines shares all three and reads as left-aligned.
fn shared_edge(first: Rect, other: Rect, em: f64) -> Option<Alignment> {
    [Alignment::Left, Alignment::Center, Alignment::Right]
        .into_iter()
        .find(|&alignment| aligned(alignment, first, other, em))
}

fn aligned(alignment: Alignment, first: Rect, other: Rect, em: f64) -> bool {
    let edge = |rect: Rect| match alignment {
        Alignment::Left => rect.x,
        Alignment::Center => rect.x + rect.width / 2.0,
        Alignment::Right => rect.x + rect.width,
    };
    (edge(first) - edge(other)).abs() <= EDGE_TOLERANCE * em
}

/// How much two boxes overlap horizontally; zero or less when they do not.
fn overlap(a: Rect, b: Rect) -> f64 {
    (a.x + a.width).min(b.x + b.width) - a.x.max(b.x)
}

/// Whether another line sits between `upper` and `lower`, over the stretch
/// either of them covers — a caption between two labels, a line in another
/// font. Joining across it would swallow it.
fn line_between(lines: &[Line], upper: usize, lower: usize) -> bool {
    let (top, bottom) = (&lines[upper], &lines[lower]);
    let span = union(top.line.bbox, bottom.line.bbox);
    let margin = LINE_TOLERANCE * top.height;
    lines.iter().enumerate().any(|(index, line)| {
        index != upper
            && index != lower
            && line.page == top.page
            && line.line.baseline < top.line.baseline - margin
            && line.line.baseline > bottom.line.baseline + margin
            && overlap(line.line.bbox, span) > 0.0
    })
}

/// Whether `upper` and `lower` are two rows of a grid: each has a neighbour
/// on its own baseline, and those neighbours stack over each other as well.
///
/// That is a table — "1 | Ventana | 450,00" over "2 | Puerta | 890,00" —
/// whose cells line up exactly like short lines of a paragraph. It is also
/// two columns of prose, which this keeps line by line: conservative, since
/// a line a reader edits on its own is today's behaviour. A cell that wraps
/// beside a one-line row still joins: its second line has no neighbour.
fn rows_of_a_grid(lines: &[Line], upper: usize, lower: usize) -> bool {
    let neighbours = |of: usize| {
        let line = &lines[of];
        let margin = LINE_TOLERANCE * line.height;
        lines.iter().enumerate().filter(move |(index, other)| {
            *index != of
                && other.page == line.page
                && (other.line.baseline - line.line.baseline).abs() <= margin
                && overlap(other.line.bbox, line.line.bbox) <= 0.0
        })
    };
    neighbours(upper).any(|(_, beside_upper)| {
        neighbours(lower)
            .any(|(_, beside_lower)| overlap(beside_upper.line.bbox, beside_lower.line.bbox) > 0.0)
    })
}

#[cfg(test)]
mod tests {
    use super::super::{group_text_runs, TextBlock};
    use super::*;
    use pdf_document::{ContentItemId, FontKind, PageId, Rect, TextRun};

    /// A run whose baseline is `baseline`, in the parser's box convention:
    /// one em tall, the bottom edge a quarter em below the baseline.
    fn run(id: u64, x: f64, baseline: f64, width: f64, height: f64, text: &str) -> TextRun {
        TextRun {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: Rect {
                x,
                y: baseline - 0.25 * height,
                width,
                height,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: text.to_string(),
        }
    }

    fn texts(blocks: &[TextBlock]) -> Vec<&str> {
        blocks.iter().map(|block| block.text.as_str()).collect()
    }

    #[test]
    fn lines_with_a_steady_leading_and_one_left_edge_are_one_paragraph() {
        let runs = [
            run(1, 100.0, 700.0, 200.0, 10.0, "first line of"),
            run(2, 100.0, 686.0, 180.0, 10.0, "the paragraph and"),
            run(3, 100.0, 672.0, 90.0, 10.0, "its end."),
        ];
        let blocks = group_text_runs(&runs);
        assert_eq!(
            texts(&blocks),
            ["first line of\nthe paragraph and\nits end."]
        );
        let block = &blocks[0];
        assert_eq!(block.lines.len(), 3);
        assert_eq!(block.leading, Some(14.0));
        assert_eq!(block.alignment, Alignment::Left);
        assert_eq!(
            block.runs,
            [ContentItemId(1), ContentItemId(2), ContentItemId(3)]
        );
        let baselines: Vec<f64> = block.lines.iter().map(|line| line.baseline).collect();
        assert_eq!(baselines, [700.0, 686.0, 672.0]);
        assert_eq!(
            block.bbox,
            Rect {
                x: 100.0,
                y: 669.5,
                width: 200.0,
                height: 38.0
            }
        );
    }

    #[test]
    fn a_single_line_is_a_block_of_one_line_with_no_leading() {
        let blocks = group_text_runs(&[run(1, 100.0, 700.0, 50.0, 10.0, "alone")]);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 1);
        assert_eq!(blocks[0].leading, None);
        assert_eq!(blocks[0].lines[0].text, "alone");
    }

    #[test]
    fn a_heading_far_above_the_body_is_a_block_of_its_own() {
        let runs = [
            run(1, 100.0, 740.0, 120.0, 10.0, "HEADING"),
            run(2, 100.0, 700.0, 200.0, 10.0, "body one"),
            run(3, 100.0, 686.0, 200.0, 10.0, "body two"),
        ];
        assert_eq!(
            texts(&group_text_runs(&runs)),
            ["HEADING", "body one\nbody two"]
        );
    }

    #[test]
    fn a_bigger_heading_right_above_the_body_is_a_block_of_its_own() {
        let runs = [
            run(1, 100.0, 718.0, 120.0, 14.0, "HEADING"),
            run(2, 100.0, 700.0, 200.0, 10.0, "body one"),
            run(3, 100.0, 686.0, 200.0, 10.0, "body two"),
        ];
        assert_eq!(
            texts(&group_text_runs(&runs)),
            ["HEADING", "body one\nbody two"]
        );
    }

    /// A gap that changes the leading is a paragraph break.
    #[test]
    fn a_change_of_leading_starts_a_new_paragraph() {
        let runs = [
            run(1, 100.0, 700.0, 200.0, 10.0, "a"),
            run(2, 100.0, 686.0, 200.0, 10.0, "b"),
            run(3, 100.0, 672.0, 200.0, 10.0, "c"),
            run(4, 100.0, 650.0, 200.0, 10.0, "d"),
            run(5, 100.0, 636.0, 200.0, 10.0, "e"),
        ];
        assert_eq!(texts(&group_text_runs(&runs)), ["a\nb\nc", "d\ne"]);
    }

    #[test]
    fn centred_lines_are_a_centred_paragraph() {
        let runs = [
            run(1, 200.0, 700.0, 200.0, 10.0, "a wide first line"),
            run(2, 240.0, 686.0, 120.0, 10.0, "a middle"),
            run(3, 270.0, 672.0, 60.0, 10.0, "end"),
        ];
        let blocks = group_text_runs(&runs);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].alignment, Alignment::Center);
    }

    #[test]
    fn right_aligned_lines_are_a_right_aligned_paragraph() {
        let runs = [
            run(1, 200.0, 700.0, 200.0, 10.0, "Calle Mayor 1"),
            run(2, 280.0, 686.0, 120.0, 10.0, "28001 Madrid"),
        ];
        let blocks = group_text_runs(&runs);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].alignment, Alignment::Right);
    }

    #[test]
    fn lines_on_no_common_edge_are_not_joined() {
        let runs = [
            run(1, 100.0, 700.0, 200.0, 10.0, "a"),
            run(2, 150.0, 686.0, 300.0, 10.0, "b"),
        ];
        assert_eq!(group_text_runs(&runs).len(), 2);
    }

    #[test]
    fn a_line_in_another_font_between_two_lines_keeps_them_apart() {
        let mut between = run(2, 100.0, 690.0, 200.0, 10.0, "other font");
        between.resource_font_name = "F2".to_string();
        let runs = [
            run(1, 100.0, 700.0, 200.0, 10.0, "above"),
            between,
            run(3, 100.0, 680.0, 200.0, 10.0, "below"),
        ];
        assert_eq!(group_text_runs(&runs).len(), 3);
    }

    /// Two columns of prose: no paragraph crosses the gutter.
    #[test]
    fn side_by_side_columns_are_never_joined_to_each_other() {
        let mut runs = Vec::new();
        for (row, baseline) in [700.0, 686.0, 672.0].into_iter().enumerate() {
            let row = row as u64;
            runs.push(run(10 + row, 72.0, baseline, 200.0, 10.0, "left column"));
            runs.push(run(20 + row, 320.0, baseline, 200.0, 10.0, "right column"));
        }
        for block in group_text_runs(&runs) {
            let left = block.runs.iter().all(|id| id.0 < 20);
            let right = block.runs.iter().all(|id| id.0 >= 20);
            assert!(left || right, "{block:?} crosses the gutter");
        }
    }

    /// A table's cells stack exactly like short lines of a paragraph; a row
    /// with cells beside it in step with the row below is a grid, not prose.
    #[test]
    fn table_cells_stay_blocks_of_their_own() {
        let mut runs = Vec::new();
        for (row, baseline) in [700.0, 686.0, 672.0].into_iter().enumerate() {
            let row = row as u64 * 10;
            runs.push(run(row + 1, 72.0, baseline, 30.0, 10.0, "1"));
            runs.push(run(row + 2, 150.0, baseline, 80.0, 10.0, "Ventana"));
            runs.push(run(row + 3, 300.0, baseline, 40.0, 10.0, "450,00"));
        }
        let blocks = group_text_runs(&runs);
        assert_eq!(blocks.len(), 9);
        assert!(blocks.iter().all(|block| block.lines.len() == 1));
    }

    /// A description that wraps inside a table cell is still a paragraph:
    /// only its first line has the quantity and price beside it.
    #[test]
    fn a_wrapped_cell_beside_a_single_line_row_is_a_paragraph() {
        let runs = [
            run(1, 72.0, 700.0, 30.0, 10.0, "1"),
            run(2, 150.0, 700.0, 120.0, 10.0, "Ventana corredera"),
            run(3, 300.0, 700.0, 40.0, 10.0, "450,00"),
            run(4, 150.0, 686.0, 110.0, 10.0, "de dos hojas"),
        ];
        let blocks = group_text_runs(&runs);
        assert!(texts(&blocks).contains(&"Ventana corredera\nde dos hojas"));
    }

    /// The shipped sample sets 14 pt body text 32 pt apart — a leading of
    /// 2.29 em, and still a paragraph a reader edits as one.
    #[test]
    fn a_double_spaced_paragraph_is_one_paragraph() {
        let runs = [
            run(
                1,
                72.0,
                604.0,
                300.0,
                14.0,
                "This file ships inside the app.",
            ),
            run(2, 72.0, 572.0, 290.0, 14.0, "can scroll, search, and print"),
            run(3, 72.0, 540.0, 200.0, 14.0, "no document of your own."),
        ];
        let blocks = group_text_runs(&runs);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].leading, Some(32.0));
    }

    #[test]
    fn the_shipped_samples_first_page_reads_as_heading_paragraph_and_closing_line() {
        let bytes = include_bytes!("../../../../assets/sample/vitela-sample.pdf");
        let document = lopdf::Document::load_mem(bytes).expect("the sample parses");
        let page = crate::page_object_id(&document, PageId(0)).expect("page one");
        let runs = crate::read_page_object_content(&document, page, PageId(0))
            .expect("readable page")
            .text_runs;
        let blocks = group_text_runs(&runs);
        assert_eq!(
            texts(&blocks),
            [
                "Vitela",
                "A sample document",
                "This file ships inside the app. It is here so you\n\
                 can scroll, search, and print straight away, with\n\
                 no document of your own required.",
                "Vitela is Spanish for vellum.",
            ]
        );
    }

    #[test]
    fn runs_on_different_pages_are_never_one_paragraph() {
        let mut other_page = run(2, 100.0, 686.0, 200.0, 10.0, "b");
        other_page.page = PageId(1);
        let runs = [run(1, 100.0, 700.0, 200.0, 10.0, "a"), other_page];
        assert_eq!(group_text_runs(&runs).len(), 2);
    }
}
