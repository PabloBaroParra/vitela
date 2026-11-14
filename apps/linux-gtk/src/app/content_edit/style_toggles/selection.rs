//! Which runs of an open editor's text a toggle restyles, from the text the
//! reader has selected in it — kept free of widgets so it can be tested
//! without a display.
//!
//! The core restyles whole runs only, so a selection is mapped onto the
//! runs it covers. The editor shows the block's text as the core grouped it
//! (`pdf_edit::group`): the runs line by line, left to right, joined by
//! nothing, an implied space, or a line break. Each run is found in that
//! text in turn, where the previous one ended or after the whitespace that
//! follows it.
//!
//! A run counts only when the selection covers all of it. Whitespace at
//! either end of the selection or of a run does not count, so "Referencia:"
//! selected with or without the line break after it is the same selection.
//! A selection that covers no run whole is refused rather than rounded out
//! to the runs it touches.

/// What a toggle pressed with `selection` acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Selected {
    /// Every run: no selection (a caret only), or one over the whole text.
    Whole,
    /// Only these runs, by index into the block's runs, in order.
    Runs(Vec<usize>),
    /// A selection that covers no run whole.
    NoWholeRun,
}

/// The runs of `runs` (their texts, in the block's order) that `selection`
/// covers in `text`, the editor's text. `selection` is a pair of character
/// offsets, in either order; `None` or an empty pair is a caret.
///
/// A run that cannot be found where it should be — the reader retyped it —
/// and every run after it count as not covered.
pub(crate) fn selected_runs(
    text: &str,
    runs: &[&str],
    selection: Option<(usize, usize)>,
) -> Selected {
    let chars: Vec<char> = text.chars().collect();
    let Some((start, end)) = selection
        .map(|(a, b)| (a.min(b).min(chars.len()), a.max(b).min(chars.len())))
        .filter(|(start, end)| start < end)
    else {
        return Selected::Whole;
    };
    let Some(selected) = trim(&chars, (start, end)) else {
        return Selected::NoWholeRun;
    };
    if trim(&chars, (0, chars.len())).is_some_and(|all| contains(selected, all)) {
        return Selected::Whole;
    }
    let covered: Vec<usize> = locate(&chars, runs)
        .into_iter()
        .enumerate()
        .filter_map(|(index, span)| {
            let span = span.filter(|(start, end)| start < end)?;
            // A run of spaces counts by its own extent.
            let span = trim(&chars, span).unwrap_or(span);
            contains(selected, span).then_some(index)
        })
        .collect();
    match covered.len() {
        0 => Selected::NoWholeRun,
        n if n == runs.len() => Selected::Whole,
        _ => Selected::Runs(covered),
    }
}

/// Where each run sits in `chars`, as character spans; `None` from the
/// first run that is not where the text before it says it should be.
fn locate(chars: &[char], runs: &[&str]) -> Vec<Option<(usize, usize)>> {
    let mut cursor = 0;
    let mut lost = false;
    runs.iter()
        .map(|run| {
            if lost {
                return None;
            }
            let run: Vec<char> = run.chars().collect();
            let at = |position: usize| chars[position..].starts_with(&run);
            let after_gap = cursor
                + chars[cursor..]
                    .iter()
                    .take_while(|c| c.is_whitespace())
                    .count();
            let found = [cursor, after_gap].into_iter().find(|&p| at(p));
            match found {
                Some(position) => {
                    cursor = position + run.len();
                    Some((position, cursor))
                }
                None => {
                    lost = true;
                    None
                }
            }
        })
        .collect()
}

/// `span` without the whitespace at either end; `None` when nothing else is
/// left.
fn trim(chars: &[char], (start, end): (usize, usize)) -> Option<(usize, usize)> {
    let inner = &chars[start..end];
    let first = inner.iter().position(|c| !c.is_whitespace())?;
    let last = inner.iter().rposition(|c| !c.is_whitespace())?;
    Some((start + first, start + last + 1))
}

fn contains(outer: (usize, usize), inner: (usize, usize)) -> bool {
    outer.0 <= inner.0 && inner.1 <= outer.1
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAGRAPH: &str = "PRESUPUESTO Nº 3.725/1\nReferencia:";
    const PARAGRAPH_RUNS: [&str; 3] = ["PRESUPUESTO Nº", "3.725/1", "Referencia:"];

    /// The character offsets of `needle` in `text`.
    fn span_of(text: &str, needle: &str) -> (usize, usize) {
        let byte = text.find(needle).expect("in the text");
        let start = text[..byte].chars().count();
        (start, start + needle.chars().count())
    }

    #[test]
    fn a_caret_or_no_selection_is_the_whole_block() {
        assert_eq!(
            selected_runs(PARAGRAPH, &PARAGRAPH_RUNS, None),
            Selected::Whole
        );
        assert_eq!(
            selected_runs(PARAGRAPH, &PARAGRAPH_RUNS, Some((5, 5))),
            Selected::Whole
        );
    }

    #[test]
    fn the_whole_text_selected_is_the_whole_block() {
        let all = PARAGRAPH.chars().count();
        assert_eq!(
            selected_runs(PARAGRAPH, &PARAGRAPH_RUNS, Some((0, all))),
            Selected::Whole
        );
        assert_eq!(
            selected_runs(PARAGRAPH, &PARAGRAPH_RUNS, Some((all, 0))),
            Selected::Whole,
            "selected backwards"
        );
    }

    #[test]
    fn one_run_of_a_single_line_is_found_past_its_implied_space() {
        let text = "Hello world";
        let runs = ["Hello", "world"];
        assert_eq!(
            selected_runs(text, &runs, Some(span_of(text, "world"))),
            Selected::Runs(vec![1])
        );
        assert_eq!(
            selected_runs(text, &runs, Some(span_of(text, "Hello"))),
            Selected::Runs(vec![0])
        );
    }

    #[test]
    fn the_second_line_of_a_paragraph_is_its_own_runs() {
        assert_eq!(
            selected_runs(
                PARAGRAPH,
                &PARAGRAPH_RUNS,
                Some(span_of(PARAGRAPH, "Referencia:"))
            ),
            Selected::Runs(vec![2])
        );
        assert_eq!(
            selected_runs(
                PARAGRAPH,
                &PARAGRAPH_RUNS,
                Some(span_of(PARAGRAPH, "PRESUPUESTO Nº 3.725/1"))
            ),
            Selected::Runs(vec![0, 1]),
            "a whole first line"
        );
    }

    #[test]
    fn whitespace_around_the_selection_does_not_count() {
        let (start, end) = span_of(PARAGRAPH, "Referencia:");
        assert_eq!(
            selected_runs(PARAGRAPH, &PARAGRAPH_RUNS, Some((start - 1, end))),
            Selected::Runs(vec![2]),
            "with the line break before it"
        );
        let text = "Referencia: \nnext";
        let runs = ["Referencia: ", "next"];
        let (start, end) = span_of(text, "Referencia:");
        assert_eq!(
            selected_runs(text, &runs, Some((start, end + 2))),
            Selected::Runs(vec![0]),
            "with the trailing space and line break"
        );
        assert_eq!(
            selected_runs(text, &runs, Some((start, end))),
            Selected::Runs(vec![0]),
            "without the run's own trailing space"
        );
    }

    #[test]
    fn part_of_a_run_is_refused_not_rounded_out() {
        assert_eq!(
            selected_runs(
                PARAGRAPH,
                &PARAGRAPH_RUNS,
                Some(span_of(PARAGRAPH, "Refer"))
            ),
            Selected::NoWholeRun
        );
        assert_eq!(
            selected_runs(PARAGRAPH, &PARAGRAPH_RUNS, Some(span_of(PARAGRAPH, "\n"))),
            Selected::NoWholeRun,
            "only whitespace"
        );
    }

    #[test]
    fn a_selection_covering_one_run_and_part_of_another_takes_the_whole_one() {
        assert_eq!(
            selected_runs(
                PARAGRAPH,
                &PARAGRAPH_RUNS,
                Some(span_of(PARAGRAPH, "3.725/1\nRefer"))
            ),
            Selected::Runs(vec![1])
        );
    }

    #[test]
    fn a_retyped_run_and_the_runs_after_it_are_not_covered() {
        let text = "PRESUPUESTO Nº 9/1\nReferencia:";
        assert_eq!(
            selected_runs(text, &PARAGRAPH_RUNS, Some(span_of(text, "Referencia:"))),
            Selected::NoWholeRun
        );
        assert_eq!(
            selected_runs(text, &PARAGRAPH_RUNS, Some(span_of(text, "PRESUPUESTO Nº"))),
            Selected::Runs(vec![0])
        );
    }
}
