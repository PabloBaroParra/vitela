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
//!
//! The same stacks answer a second question (B26 T-256): which repaint of
//! the pages *in place* the history stands at. A page-content step (a
//! watermark, and the text and image edits after it) changes what a page
//! shows while the page keeps its id and its turn, so a shell that keys its
//! bitmaps on those would keep showing the old pixels. Each such step gets a
//! paint mark too, and [`PreviewMarks::paint_revision`] is the newest one
//! still applied: in a linear history that one mark names the whole set of
//! applied content steps, so an undo returns to the earlier revision (whose
//! bitmaps are right again) and a new step in place of an undone one gets a
//! revision of its own.
//!
//! **Page-content edits (B26 T-258).** Two things they do that no other
//! step does, both handled without a stack per base:
//!
//! - A content edit may **amend** a queued one instead of adding a step
//!   (`pdf_save::record_content_command` folds a second retype of a run into
//!   the first, and a second move of an image into its queued move). The log
//!   keeps its depth but the pixels change — not only at the newest step: an
//!   amended entry further back changes every state from it on. So
//!   [`PreviewMarks::amended`] gives that step and every newer one fresh
//!   marks, and drops the redo side as the log's `amend` does. Undoing past
//!   them then lands on marks that still name the states they were handed
//!   out for.
//! - A **checkpoint** moves the queue into a base behind and bumps the base
//!   generation; undo and redo then walk across it one step at a time
//!   (`pdf_save::checkpoint`). Each such walk is still exactly one step of
//!   the reader's history, and a checkpoint's retried edit is exactly one
//!   new step, so the marks stay one flat stack across every base: the
//!   steps behind a checkpoint keep their marks, and the queue the
//!   checkpoint starts begins on top of them. No frame per generation is
//!   needed, because nothing a checkpoint does changes how many steps there
//!   are or what any of them shows.

/// The marks of the shown steps in the edit history, and the one the render
/// side shows.
#[derive(Debug, Default)]
pub(super) struct PreviewMarks {
    /// One entry per undoable step, oldest first: its marks.
    undo: Vec<StepMarks>,
    /// One entry per redoable step, the next redo last.
    redo: Vec<StepMarks>,
    /// The last mark handed out; 0 is the file as opened.
    last: u64,
    /// The mark of the state the render side was last opened at.
    shown: u64,
}

/// One step's marks: when the preview shows it, and when it repaints the
/// pages in place. Both come from the same counter, so each is unique.
#[derive(Debug, Clone, Copy, Default)]
struct StepMarks {
    shown: Option<u64>,
    paint: Option<u64>,
}

impl PreviewMarks {
    /// A new step was recorded; it clears the redo side, as the log does.
    /// `paints_in_place` only counts for a step the preview `shows`.
    pub(super) fn recorded(&mut self, shows: bool, paints_in_place: bool) {
        let mark = shows.then(|| {
            self.last += 1;
            self.last
        });
        self.undo.push(StepMarks {
            shown: mark,
            paint: mark.filter(|_| paints_in_place),
        });
        self.redo.clear();
    }

    /// The newest `count` steps were rewritten in place — an edit amended
    /// the oldest of them, which changes what every newer one shows too. Each
    /// of them that the preview shows gets a fresh mark (and a fresh paint
    /// mark where it had one); the redo side is dropped, as the log drops it.
    pub(super) fn amended(&mut self, count: usize) {
        let from = self.undo.len().saturating_sub(count);
        let mut last = self.last;
        for step in &mut self.undo[from..] {
            if step.shown.is_some() {
                last += 1;
                step.paint = step.paint.map(|_| last);
                step.shown = Some(last);
            }
        }
        self.last = last;
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
        self.undo
            .iter()
            .rev()
            .find_map(|step| step.shown)
            .unwrap_or(0)
    }

    /// The newest in-place repaint still applied; 0 is the file as opened.
    /// Equal revisions show the same pixels on a page with the same id and
    /// turn.
    pub(super) fn paint_revision(&self) -> u64 {
        self.undo
            .iter()
            .rev()
            .find_map(|step| step.paint)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_the_preview_does_not_show_never_stales_it() {
        let mut marks = PreviewMarks::default();
        marks.recorded(false, false);
        marks.undone();
        marks.redone();
        assert!(!marks.stale());
    }

    #[test]
    fn stale_follows_the_history_back_to_what_is_shown() {
        let mut marks = PreviewMarks::default();
        marks.recorded(true, false);
        assert!(marks.stale());
        marks.undone();
        assert!(!marks.stale(), "the file as opened is what is shown");
        marks.redone();
        marks.fresh();
        marks.recorded(false, false);
        marks.recorded(true, false);
        marks.undone();
        assert!(
            !marks.stale(),
            "only the annotation step is over the shown one"
        );
    }

    #[test]
    fn a_new_step_in_place_of_an_undone_one_is_a_new_state() {
        let mut marks = PreviewMarks::default();
        marks.recorded(true, false);
        marks.fresh();
        marks.undone();
        marks.recorded(true, false);
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
        assert_eq!(marks.paint_revision(), 0);
    }

    #[test]
    fn the_paint_revision_follows_the_content_steps_only() {
        let mut marks = PreviewMarks::default();
        // A page move: shown, but every page keeps its own pixels.
        marks.recorded(true, false);
        assert_eq!(marks.paint_revision(), 0);
        marks.recorded(true, true);
        let stamped = marks.paint_revision();
        assert_ne!(stamped, 0);
        // An annotation and another move leave the pixels of each page alone.
        marks.recorded(false, false);
        marks.recorded(true, false);
        assert_eq!(marks.paint_revision(), stamped);
        marks.undone();
        marks.undone();
        marks.undone();
        assert_eq!(marks.paint_revision(), 0, "back to the file's pixels");
        marks.redone();
        assert_eq!(marks.paint_revision(), stamped, "the same stamp again");
        marks.undone();
        marks.recorded(true, true);
        let other = marks.paint_revision();
        assert_ne!(other, 0);
        assert_ne!(other, stamped, "a new stamp in place of an undone one");
    }

    #[test]
    fn an_amended_step_repaints_and_drops_the_redo_side() {
        let mut marks = PreviewMarks::default();
        marks.recorded(true, true);
        let first = marks.paint_revision();
        marks.recorded(true, true);
        marks.undone();
        marks.fresh();
        marks.amended(1);
        let amended = marks.paint_revision();
        assert_ne!(amended, first, "the same step shows other pixels now");
        assert!(marks.stale(), "the preview no longer shows it");
        marks.redone();
        assert_eq!(
            marks.paint_revision(),
            amended,
            "the amend cleared the redo"
        );
        marks.undone();
        assert_eq!(marks.paint_revision(), 0);
    }

    #[test]
    fn amending_an_older_step_remarks_every_newer_one() {
        let mut marks = PreviewMarks::default();
        marks.recorded(true, true);
        let first = marks.paint_revision();
        // An annotation, then a page move: neither repaints a page.
        marks.recorded(false, false);
        marks.recorded(true, false);
        assert_eq!(marks.paint_revision(), first);
        marks.amended(3);
        let amended = marks.paint_revision();
        assert_ne!(amended, first);
        marks.undone();
        marks.undone();
        assert_eq!(
            marks.paint_revision(),
            amended,
            "the amended step's own pixels, not the ones it had before"
        );
    }

    #[test]
    fn an_unshown_step_never_repaints() {
        let mut marks = PreviewMarks::default();
        marks.recorded(false, true);
        assert_eq!(marks.paint_revision(), 0);
    }
}
