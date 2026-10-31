//! This session as `pdf_save::checkpoint` sees it.
//!
//! A page-content edit of an item that already has an unsaved edit — a moved
//! run retyped, a retyped run moved — used to be refused with "save and
//! reopen first", because every queued edit is checked against the file as
//! it was opened. The core can now do that save in memory: it writes the
//! queue into a new base, empties it, and retries the edit there. The new
//! base and its bytes become `save_backing`, the one it replaced is kept in
//! `content_checkpoints`, and undo steps back across it one edit at a time.
//!
//! The policy is the core's, shared with every shell behind the FFI. What is
//! here is only how this session's fields are lent to it: [`Live`] borrows
//! the model, the backing and the checkpoints together, which a
//! `DocumentSession` cannot do through methods of its own.

use std::sync::Arc;

use pdf_document::Document;
use pdf_manip::LopdfDocument;
use pdf_save::{CheckpointSession, Checkpoints};

use crate::app::state::{DocumentSession, ImportedSource, SaveBacking};

/// The parts of a session a checkpoint reads and swaps.
pub(crate) struct Live<'s> {
    document: &'s mut Document,
    backing: &'s mut SaveBacking,
    checkpoints: &'s mut Checkpoints<SaveBacking>,
    imported: &'s [ImportedSource],
}

impl<'s> Live<'s> {
    /// `session`'s model, backing and checkpoints, or `None` when it has no
    /// editable model or nothing to save it against.
    pub(crate) fn of(session: &'s mut DocumentSession) -> Option<Self> {
        let DocumentSession {
            document_model,
            save_backing,
            content_checkpoints,
            imported_sources,
            ..
        } = session;
        Some(Live {
            document: document_model.as_mut()?,
            backing: save_backing.as_mut()?,
            checkpoints: content_checkpoints,
            imported: imported_sources,
        })
    }

    /// Runs one of `pdf_save::record`'s entry points over the live model,
    /// base and imported PDFs.
    pub(crate) fn record(
        &mut self,
        record: impl FnOnce(
            &mut Document,
            &LopdfDocument,
            pdf_save::ImportedSources<'_, '_>,
        ) -> Result<(), pdf_save::RecordError>,
    ) -> Result<(), pdf_save::RecordError> {
        let sources: Vec<_> = self
            .imported
            .iter()
            .map(|source| (source.id, &source.document))
            .collect();
        record(
            self.document,
            &self.backing.base,
            pdf_save::ImportedSources::new(&sources),
        )
    }
}

impl CheckpointSession for Live<'_> {
    type Base = SaveBacking;

    fn document(&self) -> &Document {
        self.document
    }

    fn document_mut(&mut self) -> &mut Document {
        self.document
    }

    fn base(&self) -> &LopdfDocument {
        &self.backing.base
    }

    fn original_bytes(&self) -> Option<&[u8]> {
        Some(&self.backing.original_bytes)
    }

    fn has_imported_sources(&self) -> bool {
        !self.imported.is_empty()
    }

    fn checkpoints(&self) -> &Checkpoints<SaveBacking> {
        self.checkpoints
    }

    fn checkpoints_mut(&mut self) -> &mut Checkpoints<SaveBacking> {
        self.checkpoints
    }

    fn checkpointed_base(&self, base: LopdfDocument, bytes: Vec<u8>) -> SaveBacking {
        SaveBacking {
            base: Arc::new(base),
            original_bytes: Arc::new(bytes),
            // Only an unencrypted file may checkpoint, so this is `None`;
            // carried rather than assumed all the same.
            password: self.backing.password.clone(),
        }
    }

    fn replace_base(&mut self, base: SaveBacking) -> SaveBacking {
        std::mem::replace(self.backing, base)
    }
}

/// Whether a page-content edit `session` refuses because its item already
/// has an unsaved edit would be written into a new in-memory base and
/// retried instead. `false` means the old rule holds: save and reopen first.
pub(crate) fn may_checkpoint(session: &DocumentSession) -> bool {
    let (Some(document), Some(backing)) = (
        session.document_model.as_ref(),
        session.save_backing.as_ref(),
    ) else {
        return false;
    };
    pdf_save::checkpoint::can_checkpoint(
        document,
        &backing.base,
        true,
        !session.imported_sources.is_empty(),
    )
}

/// Whether `session` has anything to undo, a checkpoint behind included.
pub(crate) fn can_undo(session: &DocumentSession) -> bool {
    session.document_model.as_ref().is_some_and(|document| {
        session
            .content_checkpoints
            .can_undo(&document.pending_edits)
    })
}

/// Whether `session` has anything to redo, a checkpoint ahead included.
pub(crate) fn can_redo(session: &DocumentSession) -> bool {
    session.document_model.as_ref().is_some_and(|document| {
        session
            .content_checkpoints
            .can_redo(&document.pending_edits)
    })
}

/// Forgets every page's parsed content: after the base is swapped, each item
/// read before names the old one.
pub(crate) fn forget_page_content(session: &mut DocumentSession) {
    for page in &mut session.pages {
        page.content = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::content_edit::command::record;
    use crate::app::test_fixtures::{checkpointable_session, saved_runs, shown_runs};
    use pdf_document::{Command, Rect, TextRun};

    fn moved(run: &TextRun, x: f64, y: f64) -> Command {
        Command::MoveTextRun {
            item: run.clone(),
            to: Rect { x, y, ..run.bbox },
        }
    }

    fn retyped(run: &TextRun, after: &str) -> Command {
        Command::ReplaceTextRunContent {
            item: run.clone(),
            after: after.to_string(),
        }
    }

    fn shown(session: &DocumentSession, text: &str) -> TextRun {
        shown_runs(session)
            .into_iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("no run shows {text:?}"))
    }

    /// The refusal this replaces: a run moved and not yet saved could not be
    /// retyped until the reader saved and reopened the file.
    #[test]
    fn a_moved_run_is_retyped_without_a_save() {
        let mut session = checkpointable_session(&["Original text"]);
        let original = shown(&session, "Original text");
        record(&mut session, moved(&original, 80.0, 90.0)).expect("the move records");
        assert!(may_checkpoint(&session));

        let after_move = shown(&session, "Original text");
        record(&mut session, retyped(&after_move, "Changed")).expect("the retype checkpoints");

        assert_eq!(session.content_checkpoints.generation(), 1);
        let entries = session
            .document_model
            .as_ref()
            .expect("a model")
            .pending_edits
            .entries();
        assert_eq!(entries.len(), 1, "the move went into the new base");
        let written = saved_runs(&session);
        assert_eq!(written.len(), 1, "{written:?}");
        assert_eq!(written[0].text, "Changed");
        assert!(
            (written[0].bbox.x - 80.0).abs() < 0.01,
            "{:?}",
            written[0].bbox
        );
        assert!(can_undo(&session));
    }

    /// The trap the memory of the Windows port warns about. Two lines, so the
    /// checkpoint renumbers them — the moved run is painted last once
    /// written — and a run held from before it names the other line. Sent
    /// again, it must fold into the retype the checkpoint queued, not become
    /// a second edit of the same run that no save could replay.
    #[test]
    fn a_run_held_across_a_checkpoint_folds_into_its_retype() {
        let mut session = checkpointable_session(&["First line", "Second line"]);
        let first = shown(&session, "First line");
        record(&mut session, moved(&first, 80.0, 90.0)).expect("the move records");
        let held = shown(&session, "First line");

        record(&mut session, retyped(&held, "Changed")).expect("the retype checkpoints");
        record(&mut session, retyped(&held, "Changed again")).expect("the held run folds");
        record(&mut session, retyped(&held, "Changed for good")).expect("the held run folds");

        let written = saved_runs(&session);
        let texts: Vec<&str> = written.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts.len(), 2, "{texts:?}");
        assert!(texts.contains(&"Changed for good"), "{texts:?}");
        assert!(texts.contains(&"Second line"), "{texts:?}");
        let changed = written
            .iter()
            .find(|run| run.text == "Changed for good")
            .expect("the retyped run");
        assert!((changed.bbox.x - 80.0).abs() < 0.01, "{:?}", changed.bbox);
    }

    /// Undo steps back across the checkpoint one edit at a time, exactly as
    /// if there were none, and redo walks forward again — through the
    /// history step the Undo and Redo actions run.
    #[test]
    fn undo_and_redo_walk_across_a_checkpoint() {
        use crate::app::annotations::command::step_session;

        let mut session = checkpointable_session(&["Original text"]);
        let original = shown_runs(&session);
        record(&mut session, moved(&original[0], 80.0, 90.0)).expect("the move records");
        let after_move = shown(&session, "Original text");
        record(&mut session, retyped(&after_move, "Changed")).expect("the retype checkpoints");

        assert!(
            step_session(&mut session, true).is_some(),
            "undo the retype"
        );
        let undone = shown(&session, "Original text");
        assert!(
            (undone.bbox.x - 80.0).abs() < 0.01,
            "still moved: {:?}",
            undone.bbox
        );
        assert_eq!(session.content_checkpoints.generation(), 1);

        assert!(step_session(&mut session, true).is_some(), "then the move");
        assert_eq!(
            session.content_checkpoints.generation(),
            2,
            "back on the old base"
        );
        assert_eq!(shown_runs(&session), original);
        assert!(!can_undo(&session));
        assert_eq!(step_session(&mut session, true), None);

        assert!(step_session(&mut session, false).is_some(), "redo the move");
        assert!(
            step_session(&mut session, false).is_some(),
            "redo the retype"
        );
        assert!(!can_redo(&session));
        let written = saved_runs(&session);
        assert_eq!(written.len(), 1, "{written:?}");
        assert_eq!(written[0].text, "Changed");
        assert!(
            (written[0].bbox.x - 80.0).abs() < 0.01,
            "{:?}",
            written[0].bbox
        );
    }

    /// Any other edit after stepping back drops the future across the
    /// checkpoint, as a fresh edit always drops the redo stack.
    #[test]
    fn a_fresh_edit_after_stepping_back_drops_the_checkpoint_ahead() {
        use crate::app::annotations::command::step_session;

        let mut session = checkpointable_session(&["Original text"]);
        let original = shown(&session, "Original text");
        record(&mut session, moved(&original, 80.0, 90.0)).expect("the move records");
        let after_move = shown(&session, "Original text");
        record(&mut session, retyped(&after_move, "Changed")).expect("the retype checkpoints");
        step_session(&mut session, true);
        step_session(&mut session, true);

        let back = shown(&session, "Original text");
        record(&mut session, retyped(&back, "Other")).expect("a fresh retype records");

        assert!(!can_redo(&session));
        assert_eq!(step_session(&mut session, false), None);
        let written = saved_runs(&session);
        assert_eq!(written.len(), 1, "{written:?}");
        assert_eq!(written[0].text, "Other");
    }

    /// Where the core may not checkpoint, the refusal stands and the session
    /// is left exactly as it was.
    #[test]
    fn a_session_with_imported_pdfs_keeps_the_refusal() {
        let mut session = checkpointable_session(&["Original text"]);
        let original = shown(&session, "Original text");
        record(&mut session, moved(&original, 80.0, 90.0)).expect("the move records");
        session
            .imported_sources
            .push(crate::app::state::ImportedSource {
                id: pdf_document::ImportedDocumentId(0),
                document: (*session.save_backing.as_ref().expect("a backing").base).clone(),
                name: "imported.pdf".to_string(),
            });
        assert!(!may_checkpoint(&session));

        let after_move = shown(&session, "Original text");
        assert!(record(&mut session, retyped(&after_move, "Changed")).is_err());
        assert_eq!(session.content_checkpoints.generation(), 0);
        assert_eq!(
            session
                .document_model
                .as_ref()
                .expect("a model")
                .pending_edits
                .entries()
                .len(),
            1
        );
    }
}
