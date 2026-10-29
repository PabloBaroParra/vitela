//! Text runs grouped into the blocks a reader sees (B25 P2, T-207).
//!
//! A real file rarely paints a phrase with one operator. A report generator
//! writes "PRESUPUESTO Nº 3.725/1" as three show-text operators, and to
//! [`crate::parse`] those are three runs, so content edit "selects by word".
//! This module joins runs that sit next to each other on one line into a
//! [`TextBlock`] a shell can select, move, delete and retype as a unit.
//!
//! It is grouping, **not reflow**: nothing here measures text or wraps a
//! line, and B21's "out of scope" on reflow still holds. A block is only a
//! list of the runs it is made of, and every edit of it is a
//! [`Command::Batch`] of ordinary edits of those runs, so a block move or
//! delete is one undo step.
//!
//! Only lines are joined for now. Lines stacked into a paragraph are left as
//! separate blocks: retyping a paragraph puts the whole text in its first
//! run, which would collapse every line onto one.

use pdf_document::{Command, ContentItemId, Rect, TextRun};

/// The widest gap, as a fraction of the line's height, that still reads as
/// the space between two words. A space is about a quarter to a third of an
/// em and a run's box is about an em tall, so this admits one space and
/// keeps apart the much wider gap between two table columns.
const MAX_WORD_GAP: f64 = 0.4;

/// How far two runs may overlap, as a fraction of height, and still be next
/// to each other — kerning and rounding in the generator, not two runs drawn
/// on top of one another.
const MAX_OVERLAP: f64 = 0.2;

/// How far apart two runs' baselines and heights may be, as a fraction of
/// height, and still be one line in one size.
const LINE_TOLERANCE: f64 = 0.2;

/// A gap wider than this fraction of height is a space the file left out of
/// both runs' text, so the block's text gets one back.
const IMPLIED_SPACE: f64 = 0.15;

/// Runs that read as one piece of text: on one line, in one font, with no
/// more than a space between neighbours.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBlock {
    /// The member runs' ids, left to right. A run that joins nothing is a
    /// block of one, so every run is in exactly one block.
    pub runs: Vec<ContentItemId>,
    /// The union of the member runs' boxes.
    pub bbox: Rect,
    /// The member runs' text joined left to right, with a space put back
    /// wherever the file left a word gap out of the text.
    pub text: String,
}

/// Groups `runs` into blocks, in the paint order of each block's first run.
///
/// Every run lands in exactly one block. Runs from different pages are never
/// joined, so passing a whole page's [`pdf_document::PageContent::text_runs`]
/// is the expected use.
pub fn group_text_runs(runs: &[TextRun]) -> Vec<TextBlock> {
    let mut order: Vec<usize> = (0..runs.len()).collect();
    order.sort_by(|&a, &b| runs[a].bbox.x.total_cmp(&runs[b].bbox.x));

    // Each open block is the runs (by index) it holds so far, left to right.
    let mut blocks: Vec<Vec<usize>> = Vec::new();
    for index in order {
        let run = &runs[index];
        let best = blocks
            .iter()
            .enumerate()
            .filter_map(|(at, members)| {
                let last = &runs[*members.last().expect("a block is never empty")];
                word_gap(last, run).map(|gap| (at, gap))
            })
            .min_by(|(_, a), (_, b)| a.total_cmp(b));
        match best {
            Some((at, _)) => blocks[at].push(index),
            None => blocks.push(vec![index]),
        }
    }

    blocks.sort_by_key(|members| members.iter().copied().min());
    blocks
        .into_iter()
        .map(|members| block_of(runs, &members))
        .collect()
}

/// The gap from `left`'s end to `right`'s start, when `right` continues the
/// same line as the next word or fragment; `None` when it does not.
fn word_gap(left: &TextRun, right: &TextRun) -> Option<f64> {
    let height = left.bbox.height.max(right.bbox.height);
    if height <= 0.0
        || left.page != right.page
        || left.resource_font_name != right.resource_font_name
        || (left.bbox.y - right.bbox.y).abs() > LINE_TOLERANCE * height
        || (left.bbox.height - right.bbox.height).abs() > LINE_TOLERANCE * height
    {
        return None;
    }
    let gap = right.bbox.x - (left.bbox.x + left.bbox.width);
    (gap >= -MAX_OVERLAP * height && gap <= MAX_WORD_GAP * height).then_some(gap)
}

fn block_of(runs: &[TextRun], members: &[usize]) -> TextBlock {
    let first = &runs[members[0]];
    let mut bbox = first.bbox;
    let mut text = first.text.clone();
    for pair in members.windows(2) {
        let (left, right) = (&runs[pair[0]], &runs[pair[1]]);
        let gap = right.bbox.x - (left.bbox.x + left.bbox.width);
        let spaced =
            text.ends_with(char::is_whitespace) || right.text.starts_with(char::is_whitespace);
        if gap > IMPLIED_SPACE * left.bbox.height.max(right.bbox.height) && !spaced {
            text.push(' ');
        }
        text.push_str(&right.text);
        bbox = union(bbox, right.bbox);
    }
    TextBlock {
        runs: members.iter().map(|&index| runs[index].id).collect(),
        bbox,
        text,
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect {
        x,
        y,
        width: (a.x + a.width).max(b.x + b.width) - x,
        height: (a.y + a.height).max(b.y + b.height) - y,
    }
}

/// Moves every run of a block by the same offset, as one undo step.
///
/// `None` for an empty block — there is no command with nothing in it.
pub fn move_block(runs: &[TextRun], dx: f64, dy: f64) -> Option<Command> {
    batch(
        runs.iter()
            .map(|run| Command::MoveTextRun {
                item: run.clone(),
                to: Rect {
                    x: run.bbox.x + dx,
                    y: run.bbox.y + dy,
                    ..run.bbox
                },
            })
            .collect(),
    )
}

/// Removes every run of a block, as one undo step.
pub fn remove_block(runs: &[TextRun]) -> Option<Command> {
    batch(runs.iter().cloned().map(Command::RemoveTextRun).collect())
}

/// Retypes a block: the first run (left to right) shows all of `text`, and
/// the others are removed, as one undo step.
///
/// The removals come **first**. Retyping the first run to something longer
/// pushes the text cursor, so a run after it in the same text object is no
/// longer where its snapshot says, and a save could not find it to remove.
///
/// `with_inserted_font` picks [`Command::ReplaceTextRunWithInsertedFont`] for
/// the first run instead of a retype in its own font — the same choice a
/// shell makes for a single composite-font run.
pub fn retype_block(runs: &[TextRun], text: &str, with_inserted_font: bool) -> Option<Command> {
    let (first, rest) = runs.split_first()?;
    let item = first.clone();
    let after = text.to_string();
    let retype = if with_inserted_font {
        Command::ReplaceTextRunWithInsertedFont { item, after }
    } else {
        Command::ReplaceTextRunContent { item, after }
    };
    let mut children: Vec<Command> = rest.iter().cloned().map(Command::RemoveTextRun).collect();
    children.push(retype);
    batch(children)
}

fn batch(children: Vec<Command>) -> Option<Command> {
    (!children.is_empty()).then_some(Command::Batch(children))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{FontKind, PageId};

    fn run(id: u64, x: f64, width: f64, text: &str) -> TextRun {
        TextRun {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: Rect {
                x,
                y: 700.0,
                width,
                height: 10.0,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: text.to_string(),
        }
    }

    fn ids(block: &TextBlock) -> Vec<u64> {
        block.runs.iter().map(|id| id.0).collect()
    }

    /// The reference file's heading: three operators, one phrase.
    #[test]
    fn touching_fragments_on_one_line_are_one_block() {
        let runs = [
            run(1, 100.0, 50.0, "PRESUPUES"),
            run(2, 150.0, 40.0, "TO Nº 3"),
            run(3, 190.0, 30.0, ".725/1"),
        ];
        let blocks = group_text_runs(&runs);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "PRESUPUESTO Nº 3.725/1");
        assert_eq!(ids(&blocks[0]), [1, 2, 3]);
        assert_eq!(
            blocks[0].bbox,
            Rect {
                x: 100.0,
                y: 700.0,
                width: 120.0,
                height: 10.0
            }
        );
    }

    #[test]
    fn a_word_gap_joins_and_puts_the_space_back() {
        let runs = [run(1, 100.0, 30.0, "Hola"), run(2, 133.0, 30.0, "mundo")];
        let blocks = group_text_runs(&runs);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "Hola mundo");
    }

    #[test]
    fn a_space_already_in_the_text_is_not_doubled() {
        let runs = [run(1, 100.0, 30.0, "Hola "), run(2, 133.0, 30.0, "mundo")];
        assert_eq!(group_text_runs(&runs)[0].text, "Hola mundo");
    }

    /// A table: "Cantidad" and "Precio" are columns, not one phrase.
    #[test]
    fn a_column_gap_keeps_runs_apart() {
        let runs = [
            run(1, 100.0, 40.0, "Cantidad"),
            run(2, 160.0, 30.0, "Precio"),
        ];
        assert_eq!(group_text_runs(&runs).len(), 2);
    }

    #[test]
    fn runs_on_different_lines_are_not_joined() {
        let mut below = run(2, 130.0, 30.0, "abajo");
        below.bbox.y = 685.0;
        let runs = [run(1, 100.0, 30.0, "arriba"), below];
        assert_eq!(group_text_runs(&runs).len(), 2);
    }

    #[test]
    fn a_change_of_font_ends_the_block() {
        let mut bold = run(2, 130.0, 30.0, "negrita");
        bold.resource_font_name = "F2".to_string();
        let runs = [run(1, 100.0, 30.0, "normal"), bold];
        assert_eq!(group_text_runs(&runs).len(), 2);
    }

    #[test]
    fn a_change_of_size_ends_the_block() {
        let mut big = run(2, 130.0, 30.0, "grande");
        big.bbox.height = 16.0;
        let runs = [run(1, 100.0, 30.0, "chico"), big];
        assert_eq!(group_text_runs(&runs).len(), 2);
    }

    /// Paint order and reading order disagree: a generator may draw the end
    /// of a line first. The block still reads left to right.
    #[test]
    fn a_block_reads_left_to_right_whatever_the_paint_order() {
        let runs = [run(1, 133.0, 30.0, "mundo"), run(2, 100.0, 30.0, "Hola")];
        let blocks = group_text_runs(&runs);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "Hola mundo");
        assert_eq!(ids(&blocks[0]), [2, 1]);
    }

    #[test]
    fn every_run_is_in_exactly_one_block_in_paint_order() {
        let mut other_line = run(3, 100.0, 30.0, "c");
        other_line.bbox.y = 600.0;
        let runs = [
            run(1, 100.0, 30.0, "a"),
            run(2, 300.0, 30.0, "b"),
            other_line,
            run(4, 130.0, 30.0, "d"),
        ];
        let blocks = group_text_runs(&runs);
        let all: Vec<Vec<u64>> = blocks.iter().map(ids).collect();
        assert_eq!(all, vec![vec![1, 4], vec![2], vec![3]]);
    }

    #[test]
    fn a_zero_height_run_stays_on_its_own() {
        let mut flat = run(2, 130.0, 30.0, "x");
        flat.bbox.height = 0.0;
        let mut first = run(1, 100.0, 30.0, "y");
        first.bbox.height = 0.0;
        assert_eq!(group_text_runs(&[first, flat]).len(), 2);
    }

    #[test]
    fn nothing_groups_into_nothing() {
        assert!(group_text_runs(&[]).is_empty());
    }

    #[test]
    fn a_block_moves_every_run_by_the_same_offset() {
        let runs = [run(1, 100.0, 30.0, "Hola"), run(2, 133.0, 30.0, "mundo")];
        let Some(Command::Batch(children)) = move_block(&runs, 10.0, -5.0) else {
            panic!("a block move is a batch");
        };
        let destinations: Vec<(f64, f64)> = children
            .iter()
            .map(|child| match child {
                Command::MoveTextRun { to, .. } => (to.x, to.y),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(destinations, [(110.0, 695.0), (143.0, 695.0)]);
    }

    #[test]
    fn a_block_removal_removes_every_run() {
        let runs = [run(1, 100.0, 30.0, "Hola"), run(2, 133.0, 30.0, "mundo")];
        assert_eq!(
            remove_block(&runs),
            Some(Command::Batch(vec![
                Command::RemoveTextRun(runs[0].clone()),
                Command::RemoveTextRun(runs[1].clone()),
            ]))
        );
    }

    #[test]
    fn a_block_retype_removes_the_rest_before_retyping_the_first() {
        let runs = [run(1, 100.0, 30.0, "Hola"), run(2, 133.0, 30.0, "mundo")];
        assert_eq!(
            retype_block(&runs, "Chau mundo", false),
            Some(Command::Batch(vec![
                Command::RemoveTextRun(runs[1].clone()),
                Command::ReplaceTextRunContent {
                    item: runs[0].clone(),
                    after: "Chau mundo".to_string(),
                },
            ]))
        );
        assert!(matches!(
            retype_block(&runs[..1], "x", true),
            Some(Command::Batch(children))
                if matches!(children[..], [Command::ReplaceTextRunWithInsertedFont { .. }])
        ));
    }

    /// The grouping on boxes the parser measured, not hand-made ones: the
    /// reference file's split heading, then a column far to its right.
    #[test]
    fn a_parsed_heading_is_one_block_and_a_far_column_is_another() {
        let (document, page) = crate::fixture::document_with_content(
            b"BT /F1 12 Tf 100 700 Td (PRESUPUES) Tj (TO N 3) Tj (.725/1) Tj               200 0 Td (Total) Tj ET",
            crate::fixture::helvetica_resources(),
        );
        let runs = crate::read_page_object_content(&document, page, PageId(0))
            .expect("readable page")
            .text_runs;
        assert_eq!(runs.len(), 4);

        let texts: Vec<String> = group_text_runs(&runs)
            .into_iter()
            .map(|block| block.text)
            .collect();
        assert_eq!(texts, ["PRESUPUESTO N 3.725/1", "Total"]);
    }

    /// The whole retype, written: the block reads back as one run with all
    /// of the text, and nothing else is left on the line.
    #[test]
    fn a_retyped_block_reads_back_as_one_run() {
        let (mut document, page) = crate::fixture::document_with_content(
            b"BT /F1 12 Tf 100 700 Td (PRESUPUES) Tj (TO) Tj ET",
            crate::fixture::helvetica_resources(),
        );
        let runs = crate::read_page_object_content(&document, page, PageId(0))
            .expect("readable page")
            .text_runs;
        let Some(Command::Batch(children)) = retype_block(&runs, "FACTURA", false) else {
            panic!("a retype is a batch");
        };
        for child in &children {
            match child {
                Command::RemoveTextRun(run) => {
                    crate::remove_text_run(&mut document, page, run).expect("removable")
                }
                Command::ReplaceTextRunContent { item, after } => {
                    crate::replace_text_run(&mut document, page, item, after).expect("retypable")
                }
                other => panic!("unexpected {other:?}"),
            }
        }

        let after = crate::read_page_object_content(&document, page, PageId(0))
            .expect("readable page")
            .text_runs;
        let texts: Vec<&str> = after.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts, ["FACTURA"]);
    }

    #[test]
    fn an_empty_block_builds_no_command() {
        assert_eq!(move_block(&[], 1.0, 1.0), None);
        assert_eq!(remove_block(&[]), None);
        assert_eq!(retype_block(&[], "x", false), None);
    }
}
