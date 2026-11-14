//! Rebinding a run a fallback write split into several.
//!
//! A retype whose text holds a letter the run's font lacks is written as a
//! line of runs: stretches in the run's own font and stretches in its
//! standard-14 fallback (`VFb{n}.{owner}`, see `pdf_edit::encoding::fallback`).
//! The pending run a shell holds is still one run with the whole text, so
//! after a checkpoint no run of the new base shows that text. Here the line
//! it became is found instead, and the edit is spread over that line's runs
//! the way a block edit is.

use pdf_document::{Command, PageContent, PageId, Rect, TextRun};
use pdf_edit::encoding::fallback::owner_of;

use super::{Item, REBIND_TOLERANCE};

/// `command`, or each child of a batch, with every run edit whose item the
/// new base no longer holds whole spread over the runs it was written as.
///
/// A retype spread this way is aliased to the first run of its line: a shell
/// resending the run it held then folds into the queued retype of the line.
/// A move or a removal is not, since folding one into the first run alone
/// would leave the rest of the line behind.
pub(super) fn spread_split_runs(
    command: Command,
    content: &mut impl FnMut(PageId) -> Option<PageContent>,
    aliases: &mut Vec<(Item, Item)>,
) -> Command {
    match command {
        Command::Batch(children) => Command::Batch(
            children
                .into_iter()
                .flat_map(|child| spread(child, content, aliases))
                .collect(),
        ),
        single => {
            let mut spread = spread(single, content, aliases);
            match spread.len() {
                1 => spread.remove(0),
                _ => Command::Batch(spread),
            }
        }
    }
}

fn spread(
    command: Command,
    content: &mut impl FnMut(PageId) -> Option<PageContent>,
    aliases: &mut Vec<(Item, Item)>,
) -> Vec<Command> {
    let mut line_of =
        |held: &TextRun| content(held.page).and_then(|page| split_line(&page.text_runs, held));
    match command {
        Command::ReplaceTextRunContent { item, after } => match line_of(&item) {
            Some(line) => {
                aliases.push((Item::Run(item), Item::Run(line[0].clone())));
                match pdf_edit::retype_block(&line, &after, false) {
                    Some(Command::Batch(children)) => children,
                    _ => unreachable!("a line of two or more runs retypes as a batch"),
                }
            }
            None => vec![Command::ReplaceTextRunContent { item, after }],
        },
        // Every stretch of the line takes the look the held run asked for.
        Command::RestyleTextRun { item, from, to } => match line_of(&item) {
            Some(line) => line
                .into_iter()
                .map(|run| Command::RestyleTextRun {
                    item: run,
                    from,
                    to,
                })
                .collect(),
            None => vec![Command::RestyleTextRun { item, from, to }],
        },
        Command::RemoveTextRun(item) => match line_of(&item) {
            Some(line) => line.into_iter().map(Command::RemoveTextRun).collect(),
            None => vec![Command::RemoveTextRun(item)],
        },
        Command::MoveTextRun { item, to } => match line_of(&item) {
            Some(line) => line
                .into_iter()
                .map(|run| {
                    // The line moves as one: the displacement the held run
                    // asked for, from wherever each run starts.
                    let to = Rect {
                        x: run.bbox.x + to.x - item.bbox.x,
                        y: run.bbox.y + to.y - item.bbox.y,
                        ..run.bbox
                    };
                    Command::MoveTextRun { item: run, to }
                })
                .collect(),
            None => vec![Command::MoveTextRun { item, to }],
        },
        other => vec![other],
    }
}

/// The runs, two or more, a fallback write made of `held`: consecutive in
/// paint order, on its line, each in its font or that font's fallback,
/// starting at its origin and spelling its text. `None` when `held` is in
/// `runs` whole, or unless exactly one such line exists.
fn split_line(runs: &[TextRun], held: &TextRun) -> Option<Vec<TextRun>> {
    if runs.contains(held) {
        return None;
    }
    let font = line_font(&held.resource_font_name);
    let on_line = |run: &TextRun| {
        line_font(&run.resource_font_name) == font
            && (run.bbox.y - held.bbox.y).abs() <= held.bbox.height.max(REBIND_TOLERANCE)
    };
    let mut lines = (0..runs.len())
        .filter(|&start| {
            (runs[start].bbox.x - held.bbox.x).abs() <= REBIND_TOLERANCE
                && (runs[start].bbox.y - held.bbox.y).abs() <= REBIND_TOLERANCE
        })
        .filter_map(|start| {
            let mut text = String::new();
            for (count, run) in runs[start..].iter().enumerate() {
                text.push_str(&run.text);
                if !on_line(run) || !held.text.starts_with(&text) {
                    return None;
                }
                if text == held.text {
                    return (count > 0).then(|| runs[start..=start + count].to_vec());
                }
            }
            None
        });
    match (lines.next(), lines.next()) {
        (Some(line), None) => Some(line),
        _ => None,
    }
}

/// The font a run's line is in: the owner of a fallback stretch, else its own.
fn line_font(resource_name: &str) -> &str {
    owner_of(resource_name).unwrap_or(resource_name)
}
