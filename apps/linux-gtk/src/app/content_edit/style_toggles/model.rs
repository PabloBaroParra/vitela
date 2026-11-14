//! What the bold / italic / underline toggles show, and the command a press
//! records — kept free of widgets so it can be tested without a display.
//!
//! A toggle acts on every run of its target (the open editor's block or the
//! runs its selection covers, the selected block, or a multi-selection) and
//! shows the first targeted run's state:
//! pressing it turns the style on for all of them when the first run lacks
//! it, and off for all of them when it has it. A run already in the wanted
//! state is left out of the command, so a mixed selection is evened out
//! rather than flipped run by run.

use pdf_document::{Command, Document, TextRun, TextRunLook};

use crate::app::state::DocumentSession;

/// One of the three toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style {
    Bold,
    Italic,
    Underline,
}

impl Style {
    fn of(self, look: TextRunLook) -> bool {
        match self {
            Style::Bold => look.bold,
            Style::Italic => look.italic,
            Style::Underline => look.underline,
        }
    }

    fn set(self, look: TextRunLook, on: bool) -> TextRunLook {
        match self {
            Style::Bold => TextRunLook { bold: on, ..look },
            Style::Italic => TextRunLook { italic: on, ..look },
            Style::Underline => TextRunLook {
                underline: on,
                ..look
            },
        }
    }
}

/// How one run of the target looks now, and which styles can come off it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunState {
    pub(crate) run: TextRun,
    pub(crate) look: TextRunLook,
    pub(crate) can_remove_bold: bool,
    pub(crate) can_remove_italic: bool,
}

/// What the three buttons show for a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Toggles {
    /// The first run's look: what each button shows pressed.
    pub(crate) look: TextRunLook,
    /// Whether pressing Bold may turn it off: false only when the first run
    /// is bold and some run's bold is its font's, with no regular sibling.
    pub(crate) bold_removable: bool,
    pub(crate) italic_removable: bool,
}

/// The buttons for `states`; `None` for an empty target.
pub(crate) fn toggles(states: &[RunState]) -> Option<Toggles> {
    let first = states.first()?;
    Some(Toggles {
        look: first.look,
        bold_removable: !first.look.bold || states.iter().all(|state| state.can_remove_bold),
        italic_removable: !first.look.italic || states.iter().all(|state| state.can_remove_italic),
    })
}

/// The look the target shows once `style` is pressed: the first run's,
/// with that style flipped. What an open editor is redrawn in.
pub(crate) fn pressed(states: &[RunState], style: Style) -> TextRunLook {
    let look = states.first().map(|state| state.look).unwrap_or_default();
    style.set(look, !style.of(look))
}

/// Why a press records nothing, as an i18n key.
pub(crate) const BOLD_FIXED: &str = "edit-style-bold-fixed";
pub(crate) const ITALIC_FIXED: &str = "edit-style-italic-fixed";

/// The command pressing `style` records: one restyle, or a batch of them —
/// `Ok(None)` when every run already looks that way.
pub(crate) fn command(states: &[RunState], style: Style) -> Result<Option<Command>, &'static str> {
    let Some(toggles) = toggles(states) else {
        return Ok(None);
    };
    let on = !style.of(toggles.look);
    if !on {
        match style {
            Style::Bold if !toggles.bold_removable => return Err(BOLD_FIXED),
            Style::Italic if !toggles.italic_removable => return Err(ITALIC_FIXED),
            _ => {}
        }
    }
    let mut restyles: Vec<Command> = states
        .iter()
        .filter(|state| style.of(state.look) != on)
        .map(|state| Command::RestyleTextRun {
            item: state.run.clone(),
            from: state.look,
            to: style.set(state.look, on),
        })
        .collect();
    Ok(match restyles.len() {
        0 => None,
        1 => restyles.pop(),
        _ => Some(Command::Batch(restyles)),
    })
}

/// The look a queued restyle of `run` gives it, and the look it had before
/// that restyle: the last one queued, on its own or in a batch.
fn queued(document: &Document, run: &TextRun) -> Option<(TextRunLook, TextRunLook)> {
    document
        .pending_edits
        .entries()
        .iter()
        .rev()
        .flat_map(|entry| match entry {
            Command::Batch(children) => children.iter().rev().collect::<Vec<_>>(),
            single => vec![single],
        })
        .find_map(|command| match command {
            Command::RestyleTextRun { item, from, to }
                if item.id == run.id && item.page == run.page =>
            {
                Some((*from, *to))
            }
            _ => None,
        })
}

/// How each of `runs` looks now: its queued restyle when it has one, the
/// file's own look otherwise, plain for a run only a pending edit holds.
/// `None` when a run's look cannot be read at all, which disables the
/// toggles.
pub(crate) fn run_states(session: &DocumentSession, runs: &[TextRun]) -> Option<Vec<RunState>> {
    let document = session.document_model.as_ref()?;
    let backing = session.save_backing.as_ref()?;
    let sources: Vec<_> = session
        .imported_sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect();
    runs.iter()
        .map(|run| {
            let read = pdf_save::text_run_look_of(
                document,
                &backing.base,
                pdf_save::ImportedSources::new(&sources),
                run,
            )
            .ok()
            .flatten();
            // A run only a pending edit holds (an insertion, a paste) is in
            // no file yet: it shows plain, and a press goes to the core,
            // which writes the queue into a checkpoint when it has to.
            let file = match read {
                Some(file) => file,
                None if pdf_edit::pending_log_index(run.id).is_some() => pdf_edit::RunLook {
                    look: TextRunLook::default(),
                    can_remove_bold: true,
                    can_remove_italic: true,
                },
                None => return None,
            };
            Some(match queued(document, run) {
                // A style this session put on comes off by folding the
                // restyle back; one the file had needs the file's answer.
                Some((from, to)) => RunState {
                    run: run.clone(),
                    look: to,
                    can_remove_bold: !from.bold || file.can_remove_bold,
                    can_remove_italic: !from.italic || file.can_remove_italic,
                },
                None => RunState {
                    run: run.clone(),
                    look: file.look,
                    can_remove_bold: file.can_remove_bold,
                    can_remove_italic: file.can_remove_italic,
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{ContentItemId, FontKind, PageId, Rect};

    fn run(id: u64) -> TextRun {
        TextRun {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: Rect {
                x: 72.0 + id as f64 * 40.0,
                y: 700.0,
                width: 30.0,
                height: 12.0,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: format!("run{id}"),
        }
    }

    fn state(id: u64, bold: bool) -> RunState {
        RunState {
            run: run(id),
            look: TextRunLook {
                bold,
                ..TextRunLook::default()
            },
            can_remove_bold: true,
            can_remove_italic: true,
        }
    }

    #[test]
    fn a_plain_run_is_turned_bold_with_one_restyle() {
        let command = command(&[state(1, false)], Style::Bold).unwrap();
        assert_eq!(
            command,
            Some(Command::RestyleTextRun {
                item: run(1),
                from: TextRunLook::default(),
                to: TextRunLook {
                    bold: true,
                    ..TextRunLook::default()
                },
            })
        );
    }

    #[test]
    fn a_block_follows_its_first_run_and_skips_runs_already_there() {
        let Some(Command::Batch(children)) = command(
            &[state(1, false), state(2, true), state(3, false)],
            Style::Bold,
        )
        .unwrap() else {
            panic!("several runs are one batch");
        };
        let targets: Vec<u64> = children
            .iter()
            .map(|child| match child {
                Command::RestyleTextRun { item, .. } => item.id.0,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(targets, [1, 3], "the bold run is already bold");
    }

    #[test]
    fn bold_that_cannot_come_off_is_refused_and_shown_fixed() {
        let mut fixed = state(1, true);
        fixed.can_remove_bold = false;
        let toggles = toggles(std::slice::from_ref(&fixed)).unwrap();
        assert!(toggles.look.bold && !toggles.bold_removable);
        assert_eq!(command(&[fixed], Style::Bold), Err(BOLD_FIXED));
    }

    #[test]
    fn underline_toggles_on_and_off() {
        let on = command(&[state(1, false)], Style::Underline)
            .unwrap()
            .unwrap();
        let Command::RestyleTextRun { to, .. } = on else {
            panic!()
        };
        assert!(to.underline);
        let mut underlined = state(1, false);
        underlined.look.underline = true;
        let off = command(&[underlined], Style::Underline).unwrap().unwrap();
        let Command::RestyleTextRun { to, .. } = off else {
            panic!()
        };
        assert!(!to.underline);
    }

    #[test]
    fn no_target_shows_nothing_and_records_nothing() {
        assert_eq!(toggles(&[]), None);
        assert_eq!(command(&[], Style::Italic), Ok(None));
    }
}
