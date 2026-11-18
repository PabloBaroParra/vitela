//! Which point of the edit history the render side shows, so `preview_stale`
//! is exact rather than sticky.
//!
//! A sticky flag ("some step since the last refresh showed") stays set after
//! a shell takes an unshown step back because the refresh failed — and then
//! every later request, even one that records nothing, looks like it needs a
//! refresh, and its failure looks like its own. Instead each step the preview
//! shows (`pdf_save::shows_in_session_preview`) gets a mark, kept in a stack
//! parallel to the `EditLog`'s, and the preview is stale exactly when the
//! newest mark still applied is not the one the render side was opened at.
//! An annotation step gets no mark, so it never stales the preview, and
//! undoing a step that was never shown returns to fresh.
//!
//! The stacks move only where the log does: [`super::DocumentModel`]'s
//! `record`, `undo`, `redo` and its import, the only writers of the log here.

/// The marks of the shown steps in the edit history, and the one the render
/// side shows.
#[derive(Debug, Default)]
pub(super) struct PreviewMarks {
    /// One entry per undoable step, oldest first: its mark when the preview
    /// shows it.
    undo: Vec<Option<u64>>,
    /// One entry per redoable step, the next redo last.
    redo: Vec<Option<u64>>,
    /// The last mark handed out; 0 is the file as opened.
    last: u64,
    /// The mark of the state the render side was last opened at.
    shown: u64,
}

impl PreviewMarks {
    /// A new step was recorded; it clears the redo side, as the log does.
    pub(super) fn recorded(&mut self, shows: bool) {
        let mark = shows.then(|| {
            self.last += 1;
            self.last
        });
        self.undo.push(mark);
        self.redo.clear();
    }

    /// The newest step was undone.
    pub(super) fn undone(&mut self) {
        if let Some(mark) = self.undo.pop() {
            self.redo.push(mark);
        }
    }

    /// The next undone step was redone.
    pub(super) fn redone(&mut self) {
        if let Some(mark) = self.redo.pop() {
            self.undo.push(mark);
        }
    }

    /// Whether the history now stands at a state the render side does not show.
    pub(super) fn stale(&self) -> bool {
        self.current() != self.shown
    }

    /// The render side was just reopened at the current state.
    pub(super) fn fresh(&mut self) {
        self.shown = self.current();
    }

    /// The mark of the newest shown step still applied.
    fn current(&self) -> u64 {
        self.undo.iter().rev().find_map(|mark| *mark).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_the_preview_does_not_show_never_stales_it() {
        let mut marks = PreviewMarks::default();
        marks.recorded(false);
        marks.undone();
        marks.redone();
        assert!(!marks.stale());
    }

    #[test]
    fn stale_follows_the_history_back_to_what_is_shown() {
        let mut marks = PreviewMarks::default();
        marks.recorded(true);
        assert!(marks.stale());
        marks.undone();
        assert!(!marks.stale(), "the file as opened is what is shown");
        marks.redone();
        marks.fresh();
        marks.recorded(false);
        marks.recorded(true);
        marks.undone();
        assert!(
            !marks.stale(),
            "only the annotation step is over the shown one"
        );
    }

    #[test]
    fn a_new_step_in_place_of_an_undone_one_is_a_new_state() {
        let mut marks = PreviewMarks::default();
        marks.recorded(true);
        marks.fresh();
        marks.undone();
        marks.recorded(true);
        assert!(marks.stale());
        marks.redone();
        assert!(marks.stale(), "nothing to redo: the new step cleared it");
    }

    #[test]
    fn undo_and_redo_past_the_ends_change_nothing() {
        let mut marks = PreviewMarks::default();
        marks.undone();
        marks.redone();
        assert!(!marks.stale());
    }
}
