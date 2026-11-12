//! Retyping a paragraph: the new text wrapped to the paragraph's width, one
//! line per run.

use super::{batch, group_text_runs, retype_block, Alignment, TextBlock};
use crate::encoding::wrap::wrap_greedy;
use crate::error::EditError;
use lopdf::{Document, ObjectId};
use pdf_document::{Command, ContentItemId, Rect, TextRun};

/// How much wider than the paragraph's widest line a new line may measure
/// and still fit, in points: floating-point slack, so the paragraph's own
/// text wraps back exactly where it was.
const WIDTH_SLACK: f64 = 1e-6;

/// Retypes the block `runs` make up as one undo step, wrapping `text` to it
/// when it is a paragraph.
///
/// `runs` is the block's runs as [`super::TextBlock`] lists them. When they
/// group into one block of a single line, this is exactly
/// [`retype_block`]: the first run shows all of `text`, `'\n'` included.
///
/// When they group into a paragraph, every `'\n'` in `text` is a line break
/// the reader typed and each piece is greedily wrapped to the paragraph's
/// width — its widest line, as measured in the first run's font — so line
/// `i` of the result sits `i` leadings below the first baseline. An empty
/// line keeps its place and paints nothing. The batch is:
///
/// 1. every run removed that the result does not reuse — the runs after the
///    first on each line, the lines past the new count, a line now blank;
/// 2. a [`Command::InsertTextRunLike`] for each line past the old count,
///    styled after the first run (colour, spacing, size, font);
/// 3. the first run of each remaining line retyped in place.
///
/// The insertions come before the retypes because they resolve the first
/// run by its snapshot when they are written, and a retype changes it. The
/// removals come first for the reason [`retype_block`] gives.
///
/// A centred or right-aligned line moves whenever its width changes, and a
/// retyped run cannot then be moved in the same batch — its new box is an
/// estimate, and a move resolves the run by its exact box. So those
/// paragraphs are painted afresh: every line inserted where it belongs,
/// then every old run removed, last paint first so the ids still waiting
/// are not renumbered under them.
///
/// `with_inserted_font` is refused for a paragraph with
/// [`EditError::CompositeFontNotEditable`]: a composite font cannot be
/// measured to wrap, nor shown in an inserted line. Text the first run's
/// font cannot show fails the wrap with [`EditError::EncodingGap`].
pub fn retype_paragraph(
    document: &Document,
    page_object: ObjectId,
    runs: &[TextRun],
    text: &str,
    with_inserted_font: bool,
) -> Result<Option<Command>, EditError> {
    let blocks = group_text_runs(runs);
    let [block] = blocks.as_slice() else {
        return Ok(retype_block(runs, text, with_inserted_font));
    };
    let (Some(leading), true) = (block.leading, block.lines.len() > 1) else {
        return Ok(retype_block(runs, text, with_inserted_font));
    };
    let lines: Vec<Vec<&TextRun>> = block
        .lines
        .iter()
        .map(|line| {
            line.runs
                .iter()
                .filter_map(|id| run_by_id(runs, *id))
                .collect()
        })
        .collect();
    let anchor = *lines
        .first()
        .and_then(|line| line.first())
        .ok_or(EditError::ItemNotFound(block.runs[0]))?;
    // The look new lines copy, and the font text is measured in: the
    // paragraph's own font, never a fallback stretch that leads a line.
    let template = lines
        .iter()
        .flatten()
        .copied()
        .find(|run| !crate::encoding::fallback::is_fallback_resource(&run.resource_font_name))
        .unwrap_or(anchor);
    if with_inserted_font {
        return Err(EditError::CompositeFontNotEditable {
            resource_font_name: template.resource_font_name.clone(),
        });
    }

    let width_of =
        |line: &str| crate::text_run_bbox(document, page_object, template, line).map(|b| b.width);
    let widest = block
        .lines
        .iter()
        .filter_map(|line| width_of(&line.text).ok())
        .fold(block.bbox.width, f64::max);
    let wrapped = wrap_greedy(text, widest + WIDTH_SLACK, width_of)?;

    let layout = Layout {
        block,
        anchor,
        leading,
    };
    let mut new_lines = Vec::with_capacity(wrapped.len());
    for (index, line) in wrapped.into_iter().enumerate() {
        let width = if line.is_empty() {
            0.0
        } else {
            width_of(&line)?
        };
        new_lines.push((line, layout.place(index, width)));
    }

    Ok(match block.alignment {
        Alignment::Left => retype_in_place(&lines, &new_lines, template),
        Alignment::Center | Alignment::Right => repaint(&lines, &new_lines, template),
    })
}

fn run_by_id(runs: &[TextRun], id: ContentItemId) -> Option<&TextRun> {
    runs.iter().find(|run| run.id == id)
}

/// Where the lines of a retyped paragraph go.
struct Layout<'a> {
    block: &'a TextBlock,
    /// The paragraph's first run: where its first line starts.
    anchor: &'a TextRun,
    leading: f64,
}

impl Layout<'_> {
    /// The box of line `index`, `width` wide: `index` leadings below the
    /// first line, against the paragraph's shared edge.
    fn place(&self, index: usize, width: f64) -> Rect {
        let bbox = self.block.bbox;
        let x = match self.block.alignment {
            Alignment::Left => self.anchor.bbox.x,
            Alignment::Center => bbox.x + bbox.width / 2.0 - width / 2.0,
            Alignment::Right => bbox.x + bbox.width - width,
        };
        Rect {
            x,
            y: self.anchor.bbox.y - index as f64 * self.leading,
            width,
            height: self.anchor.bbox.height,
        }
    }
}

/// A new line painted like `template`, at `bbox`.
fn styled_insert(template: &TextRun, text: &str, bbox: Rect) -> Command {
    Command::InsertTextRunLike {
        item: TextRun {
            id: ContentItemId(0),
            bbox,
            text: text.to_string(),
            ..template.clone()
        },
        template: template.clone(),
    }
}

/// Removals of `runs`, last paint first: removing a run renumbers every run
/// painted after it, so going backwards keeps the ids still to come valid.
fn removals<'a>(runs: impl IntoIterator<Item = &'a TextRun>) -> Vec<Command> {
    let mut runs: Vec<&TextRun> = runs.into_iter().collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run.id));
    runs.into_iter()
        .cloned()
        .map(Command::RemoveTextRun)
        .collect()
}

/// The left-aligned batch: see [`retype_paragraph`].
fn retype_in_place(
    lines: &[Vec<&TextRun>],
    new_lines: &[(String, Rect)],
    template: &TextRun,
) -> Option<Command> {
    let kept = lines.len().min(new_lines.len());
    let mut removed: Vec<&TextRun> = Vec::new();
    let mut retypes = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some((first, rest)) = line.split_first() else {
            continue;
        };
        match new_lines.get(index) {
            Some((text, _)) if !text.is_empty() => {
                removed.extend(rest.iter().copied());
                retypes.push(Command::ReplaceTextRunContent {
                    item: (*first).clone(),
                    after: text.clone(),
                });
            }
            _ => removed.extend(line.iter().copied()),
        }
    }
    // The inserts resolve `template` when written, so its own removal (a
    // blank first line) waits until after them. Writers find a run by what
    // it paints and where, so moving it last renumbers no snapshot.
    let template_removed = removed.iter().any(|run| run.id == template.id);
    removed.retain(|run| run.id != template.id);
    let mut children = removals(removed);
    children.extend(
        new_lines[kept..]
            .iter()
            .filter(|(text, _)| !text.is_empty())
            .map(|(text, bbox)| styled_insert(template, text, *bbox)),
    );
    children.extend(retypes);
    if template_removed {
        children.push(Command::RemoveTextRun(template.clone()));
    }
    batch(children)
}

/// The centred and right-aligned batch: see [`retype_paragraph`].
fn repaint(
    lines: &[Vec<&TextRun>],
    new_lines: &[(String, Rect)],
    template: &TextRun,
) -> Option<Command> {
    let mut children: Vec<Command> = new_lines
        .iter()
        .filter(|(text, _)| !text.is_empty())
        .map(|(text, bbox)| styled_insert(template, text, *bbox))
        .collect();
    children.extend(removals(lines.iter().flatten().copied()));
    batch(children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{document_with_content, helvetica_resources};
    use pdf_document::PageId;

    /// A three-line left-aligned paragraph, 10 pt on a 14 pt leading,
    /// coloured and letter-spaced, laid out with `T*` the way reportlab
    /// writes one.
    const PARAGRAPH: &[u8] = b"BT /F1 10 Tf 14 TL 0.2 0.4 0.6 rg 0.5 Tc 100 700 Td \
        (alpha beta gamma) Tj T* (delta epsilon) Tj T* (zeta) Tj ET";

    /// The same lines centred on x = 300.
    fn centred() -> Vec<u8> {
        let lines = ["alpha beta gamma", "delta epsilon", "zeta"];
        let mut content = b"BT /F1 10 Tf ".to_vec();
        let font = crate::encoding::resolve_font(
            &lopdf::Document::new(),
            &helvetica_resources()
                .get(b"Font")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"F1")
                .unwrap()
                .as_dict()
                .unwrap()
                .clone(),
            "F1",
        )
        .unwrap();
        for (index, line) in lines.iter().enumerate() {
            let width = font.width_of(&font.encode(line).unwrap()) * 10.0;
            content.extend_from_slice(
                format!(
                    "1 0 0 1 {} {} Tm ({line}) Tj ",
                    300.0 - width / 2.0,
                    700.0 - 14.0 * index as f64
                )
                .as_bytes(),
            );
        }
        content.extend_from_slice(b"ET");
        content
    }

    fn page_runs(document: &Document, page: ObjectId) -> Vec<TextRun> {
        crate::read_page_object_content(document, page, PageId(0))
            .expect("readable page")
            .text_runs
    }

    fn children(command: Option<Command>) -> Vec<Command> {
        match command {
            Some(Command::Batch(children)) => children,
            other => panic!("a paragraph retype is a batch, got {other:?}"),
        }
    }

    /// Writes a batch the way a save does: child by child, in order.
    fn apply_all(document: &mut Document, page: ObjectId, children: &[Command]) {
        for child in children {
            match child {
                Command::RemoveTextRun(run) => {
                    crate::remove_text_run(document, page, run).expect("removable")
                }
                Command::ReplaceTextRunContent { item, after } => {
                    crate::replace_text_run(document, page, item, after).expect("retypable")
                }
                Command::InsertTextRunLike { item, template } => {
                    crate::insert_text_run_like(document, page, item, template).expect("insertable")
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    fn retyped(content: &[u8], text: &str) -> (Document, ObjectId, Vec<TextRun>, Vec<Command>) {
        let (document, page) = document_with_content(content, helvetica_resources());
        let runs = page_runs(&document, page);
        let children =
            children(retype_paragraph(&document, page, &runs, text, false).expect("retypable"));
        (document, page, runs, children)
    }

    #[test]
    fn fewer_lines_remove_the_surplus_line_and_retype_the_rest_in_place() {
        let (_, _, runs, children) = retyped(PARAGRAPH, "alpha\nbeta");
        assert_eq!(
            children,
            [
                Command::RemoveTextRun(runs[2].clone()),
                Command::ReplaceTextRunContent {
                    item: runs[0].clone(),
                    after: "alpha".to_string(),
                },
                Command::ReplaceTextRunContent {
                    item: runs[1].clone(),
                    after: "beta".to_string(),
                },
            ]
        );
    }

    /// The inserted line goes in before any retype: it is styled after the
    /// first run, which has to still be the run the snapshot names.
    #[test]
    fn more_lines_insert_a_line_styled_after_the_first_one_leading_further_down() {
        let (_, _, runs, children) = retyped(PARAGRAPH, "a\nb\nc\nd");
        let [Command::InsertTextRunLike { item, template }, retypes @ ..] = &children[..] else {
            panic!("an insertion first, got {children:?}");
        };
        assert_eq!(template, &runs[0]);
        assert_eq!(item.text, "d");
        assert_eq!(item.page, runs[0].page);
        assert_eq!(item.resource_font_name, runs[0].resource_font_name);
        assert!((item.bbox.x - runs[0].bbox.x).abs() < 1e-9);
        assert!((item.bbox.y - (runs[0].bbox.y - 42.0)).abs() < 1e-9);
        assert_eq!(item.bbox.height, runs[0].bbox.height);
        let after: Vec<&str> = retypes
            .iter()
            .map(|child| match child {
                Command::ReplaceTextRunContent { after, .. } => after.as_str(),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(after, ["a", "b", "c"]);
    }

    /// The paragraph's widest line sets the width, so its own text wraps
    /// back exactly where it was.
    #[test]
    fn text_without_breaks_wraps_to_the_paragraph_width() {
        let (_, _, _, children) = retyped(PARAGRAPH, "alpha beta gamma alpha beta gamma zeta");
        let texts: Vec<&str> = children
            .iter()
            .map(|child| match child {
                Command::ReplaceTextRunContent { after, .. } => after.as_str(),
                Command::InsertTextRunLike { item, .. } => item.text.as_str(),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(texts, ["alpha beta gamma", "alpha beta gamma", "zeta"]);
    }

    #[test]
    fn a_grown_paragraph_reads_back_as_one_paragraph_in_the_same_look() {
        let (mut document, page, _, children) = retyped(PARAGRAPH, "uno\ndos\ntres\ncuatro\ncinco");
        apply_all(&mut document, page, &children);

        let blocks = group_text_runs(&page_runs(&document, page));
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].text, "uno\ndos\ntres\ncuatro\ncinco");
        assert!((blocks[0].leading.unwrap() - 14.0).abs() < 1e-6);
        let located = crate::parse::read_located_content(&document, page).unwrap();
        let looks: Vec<&[u8]> = located
            .text_runs
            .iter()
            .map(|run| run.paint_state.as_slice())
            .collect();
        assert!(looks.windows(2).all(|pair| pair[0] == pair[1]), "{looks:?}");
    }

    #[test]
    fn a_shrunk_paragraph_reads_back_as_one_paragraph() {
        let (mut document, page, _, children) = retyped(PARAGRAPH, "one\ntwo");
        apply_all(&mut document, page, &children);

        let blocks = group_text_runs(&page_runs(&document, page));
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].text, "one\ntwo");
    }

    /// A centred line changes where it starts with its width, so every line
    /// is painted afresh, centred, and the old ones are removed.
    #[test]
    fn a_centred_paragraph_stays_centred() {
        let (mut document, page, _, children) = retyped(&centred(), "a much longer first\nb");
        apply_all(&mut document, page, &children);

        let blocks = group_text_runs(&page_runs(&document, page));
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].alignment, Alignment::Center);
        assert_eq!(blocks[0].text, "a much longer first\nb");
        for line in &blocks[0].lines {
            let centre = line.bbox.x + line.bbox.width / 2.0;
            assert!((centre - 300.0).abs() < 1e-6, "{line:?}");
        }
    }

    #[test]
    fn a_blank_line_keeps_its_place_and_paints_nothing() {
        let (mut document, page, _, children) = retyped(PARAGRAPH, "one\n\nthree\nfour");
        apply_all(&mut document, page, &children);

        let runs = page_runs(&document, page);
        let texts: Vec<&str> = runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts, ["one", "three", "four"]);
        assert!((runs[0].bbox.y - runs[1].bbox.y - 28.0).abs() < 1e-6);
    }

    /// A blank first line removes the run the inserted lines are styled
    /// after, so that removal has to wait until they are written.
    #[test]
    fn a_leading_blank_line_that_grows_the_paragraph_still_applies() {
        let (mut document, page, _, children) = retyped(
            PARAGRAPH, "
b
c
d",
        );
        apply_all(&mut document, page, &children);

        let runs = page_runs(&document, page);
        let texts: Vec<&str> = runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts, ["b", "c", "d"]);
    }

    #[test]
    fn a_single_line_block_retypes_exactly_as_retype_block_does() {
        let (document, page) = document_with_content(
            b"BT /F1 12 Tf 100 700 Td (PRESUPUES) Tj (TO) Tj ET",
            helvetica_resources(),
        );
        let runs = page_runs(&document, page);
        for with_inserted_font in [false, true] {
            assert_eq!(
                retype_paragraph(&document, page, &runs, "FACTURA\nlarga", with_inserted_font),
                Ok(retype_block(&runs, "FACTURA\nlarga", with_inserted_font))
            );
        }
    }

    #[test]
    fn a_paragraph_cannot_be_retyped_in_a_substituted_font_yet() {
        let (document, page) = document_with_content(PARAGRAPH, helvetica_resources());
        let runs = page_runs(&document, page);
        assert!(matches!(
            retype_paragraph(&document, page, &runs, "x", true),
            Err(EditError::CompositeFontNotEditable { .. })
        ));
    }

    #[test]
    fn text_the_font_cannot_show_is_refused_while_wrapping() {
        let (document, page) = document_with_content(PARAGRAPH, helvetica_resources());
        let runs = page_runs(&document, page);
        assert!(matches!(
            retype_paragraph(&document, page, &runs, "日本 語", false),
            Err(EditError::EncodingGap { .. })
        ));
    }
}
