//! The edit session of an open document: page edits, undo/redo, save, and
//! the bytes a preview is reopened from.
//!
//! Every rule is the core's, asked in the order `pdf-ffi`'s `apply_edit`
//! asks it: the permission gate (`pdf_save::check_edit`) first, then the
//! command constructors that name an out-of-range position
//! (`Command::move_pages`, `Command::rotate_pages`, `Command::remove_pages`),
//! then the model's own refusal on apply. Saving and the preview snapshot are
//! `pdf_save::save_document` and `pdf_save::session_preview` over the bytes
//! the document was opened from and its imported-source registry, exactly as
//! `pdf-ffi` builds its `SaveInput`.
//!
//! What stays here is only what has no core home: holding the session's
//! state and the `EditLog` borrow dance (`EditLog::apply` needs `&mut` to
//! both the log and the document that owns it).

use pdf_document::{Command, Document, EditLog};
use pdf_save::{EditClass, ImportedSources, SaveInput, SaveIntent, SignatureAcknowledgement};

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// A page edit the web shell can ask for — the subset of `pdf-ffi`'s
/// `FfiEditCommand` that Organize needs. Positions are 0-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageEdit {
    /// Moves the `count` pages starting at `from` so the first of them ends
    /// up at `to`.
    Move { from: u32, count: u32, to: u32 },
    /// Turns each of the `count` pages starting at `from` by
    /// `delta_degrees`, as one undo step.
    Rotate {
        from: u32,
        count: u32,
        delta_degrees: i32,
    },
    /// Removes the `count` pages starting at `index`, with whatever is
    /// anchored to them, as one undo step.
    Remove { index: u32, count: u32 },
}

impl PageEdit {
    /// What this edit is, in the terms `pdf_save::check_edit` asks about —
    /// known before the command is built, so a refused edit is refused even
    /// when its position is also wrong (`pdf-ffi`'s order). Must agree with
    /// `EditClass::of` on the built command
    /// (`each_edit_classifies_like_the_command_it_builds`).
    pub fn class(self) -> EditClass {
        let page_structure = match self {
            PageEdit::Move { .. } | PageEdit::Remove { .. } => true,
            // A turn changes neither membership nor order: it stays on the
            // incremental writer.
            PageEdit::Rotate { .. } => false,
        };
        EditClass {
            document_assembly: true,
            page_structure,
            ..EditClass::default()
        }
    }

    /// The core command for this edit against `document` as it is now, or
    /// the position that does not exist — named the way `pdf-ffi` names it.
    fn command(self, document: &Document) -> Result<Command, EngineError> {
        match self {
            PageEdit::Move { from, count, to } => Ok(Command::move_pages(
                document,
                from as usize,
                count as usize,
                to as usize,
            )?),
            PageEdit::Rotate {
                from,
                count,
                delta_degrees,
            } => Ok(Command::rotate_pages(
                document,
                from as usize,
                count as usize,
                delta_degrees,
            )?),
            // `remove_pages` refuses an empty or out-of-range run as `None`;
            // the start is what the error can name, as in `pdf-ffi`.
            PageEdit::Remove { index, count } => {
                Command::remove_pages(document, index as usize, count as usize)
                    .ok_or_else(|| EngineError::page_out_of_bounds(index))
            }
        }
    }
}

impl DocumentModel {
    /// Gates, builds and records one page edit as one undo step. Nothing is
    /// recorded when any step refuses.
    pub fn apply_edit(&mut self, edit: PageEdit) -> Result<(), EngineError> {
        pdf_save::check_edit(self.document.security.as_ref(), edit.class())?;
        let command = edit.command(&self.document)?;
        if !step_log(&mut self.document, |log, document| {
            log.apply(document, command)
        }) {
            return Err(EngineError::new(
                ErrorKind::UnsupportedOperation,
                "this edit command was rejected against the open document",
            ));
        }
        Ok(())
    }

    /// Undoes the last edit. `false` when there was nothing to undo.
    pub fn undo(&mut self) -> bool {
        step_log(&mut self.document, EditLog::undo)
    }

    /// Redoes the last undone edit. `false` when there was nothing to redo.
    pub fn redo(&mut self) -> bool {
        step_log(&mut self.document, EditLog::redo)
    }

    pub fn can_undo(&self) -> bool {
        self.document.pending_edits.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.document.pending_edits.can_redo()
    }

    /// The document with every edit applied, as a complete PDF computed from
    /// the bytes it was opened from — `pdf-ffi`'s `save_to_bytes` with the
    /// default intent (keep the document's own encryption).
    ///
    /// A save that would break a signature the file carries is refused with
    /// `SignaturesWouldBeInvalidated` unless `invalidate_signatures` says the
    /// user was told.
    pub fn save_to_bytes(&self, invalidate_signatures: bool) -> Result<Vec<u8>, EngineError> {
        let signatures = if invalidate_signatures {
            SignatureAcknowledgement::ProceedAndInvalidate
        } else {
            SignatureAcknowledgement::Unacknowledged
        };
        let sources = self.imported.pairs();
        Ok(pdf_save::save_document(SaveInput {
            document: &self.document,
            base: &self.base,
            original_bytes: Some(&self.original_bytes),
            intent: SaveIntent::Default,
            signatures,
            imported_sources: ImportedSources::new(&sources),
        })?)
    }

    /// The bytes pdfium is reopened from to show the unsaved edits
    /// (`pdf_save::session_preview`). The session itself is untouched.
    pub fn preview_bytes(&self) -> Result<Vec<u8>, EngineError> {
        let sources = self.imported.pairs();
        Ok(pdf_save::session_preview(
            &self.document,
            &self.base,
            Some(&self.original_bytes),
            ImportedSources::new(&sources),
        )?)
    }
}

/// Runs `step` over the document's own log: the take/step/restore dance
/// `EditLog`'s `&mut self, &mut Document` signature forces.
fn step_log(
    document: &mut Document,
    step: impl FnOnce(&mut EditLog, &mut Document) -> bool,
) -> bool {
    let mut log = std::mem::take(&mut document.pending_edits);
    let stepped = step(&mut log, document);
    document.pending_edits = log;
    stepped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::pdfs;
    use pdf_manip::OpenCredentials;

    fn open(bytes: &[u8]) -> DocumentModel {
        DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
    }

    fn three_pages() -> DocumentModel {
        open(&pdfs::text_document(&["one", "two", "three"]))
    }

    /// Each page's string, in page order, read back from saved bytes with
    /// lopdf — independent of everything this crate does. Reads the `Tj`
    /// operand rather than `extract_text`, whose glyph-name table is a
    /// function too large for a debug wasm32 module to load.
    fn page_texts(bytes: &[u8]) -> Vec<String> {
        let document = lopdf::Document::load_mem(bytes).expect("saved bytes parse");
        document
            .get_pages()
            .into_values()
            .map(|page| {
                let content = document.get_and_decode_page_content(page).expect("content");
                let shown = content
                    .operations
                    .iter()
                    .find(|operation| operation.operator == "Tj")
                    .expect("one Tj per page");
                let text = shown.operands[0].as_str().expect("a literal string");
                String::from_utf8_lossy(text).into_owned()
            })
            .collect()
    }

    fn rotations(model: &DocumentModel) -> Vec<u32> {
        model
            .page_sizes()
            .iter()
            .map(|page| page.rotation_degrees)
            .collect()
    }

    #[test]
    fn each_edit_classifies_like_the_command_it_builds() {
        let model = three_pages();
        // Exhaustive by construction: a new `PageEdit` variant fails to
        // compile here until it is added to the parity check.
        let every_edit = |edit: PageEdit| match edit {
            PageEdit::Move { .. } | PageEdit::Rotate { .. } | PageEdit::Remove { .. } => edit,
        };
        for edit in [
            PageEdit::Move {
                from: 0,
                count: 2,
                to: 1,
            },
            PageEdit::Rotate {
                from: 1,
                count: 2,
                delta_degrees: 90,
            },
            PageEdit::Remove { index: 0, count: 2 },
        ]
        .map(every_edit)
        {
            let command = edit.command(&model.document).expect("builds");
            assert_eq!(edit.class(), EditClass::of(&command), "{edit:?}");
        }
    }

    #[test]
    fn a_move_is_saved_in_its_new_order() {
        let mut model = three_pages();
        model
            .apply_edit(PageEdit::Move {
                from: 0,
                count: 1,
                to: 2,
            })
            .expect("moves");
        let saved = model.save_to_bytes(false).expect("saves");
        assert_eq!(page_texts(&saved), ["two", "three", "one"]);
    }

    #[test]
    fn a_rotation_is_saved_and_reopens_turned() {
        let mut model = three_pages();
        model
            .apply_edit(PageEdit::Rotate {
                from: 1,
                count: 2,
                delta_degrees: 90,
            })
            .expect("rotates");
        assert_eq!(rotations(&model), [0, 90, 90], "the model turns at once");

        let reopened = open(&model.save_to_bytes(false).expect("saves"));
        assert_eq!(rotations(&reopened), [0, 90, 90]);
    }

    #[test]
    fn a_removal_is_saved_without_its_pages() {
        let mut model = three_pages();
        model
            .apply_edit(PageEdit::Remove { index: 0, count: 2 })
            .expect("removes");
        assert_eq!(model.page_count(), 1);

        let saved = model.save_to_bytes(false).expect("saves");
        assert_eq!(page_texts(&saved), ["three"]);
    }

    #[test]
    fn undo_and_redo_step_the_edits_and_the_save_follows() {
        let mut model = three_pages();
        assert!(!model.can_undo());
        assert!(!model.undo(), "nothing to undo");
        model
            .apply_edit(PageEdit::Remove { index: 1, count: 1 })
            .unwrap();
        assert!(model.can_undo());

        assert!(model.undo());
        assert_eq!(model.page_count(), 3);
        assert!(model.can_redo());
        assert_eq!(
            page_texts(&model.save_to_bytes(false).unwrap()),
            ["one", "two", "three"]
        );

        assert!(model.redo());
        assert!(!model.redo(), "nothing left to redo");
        assert_eq!(
            page_texts(&model.save_to_bytes(false).unwrap()),
            ["one", "three"]
        );
    }

    #[test]
    fn a_position_past_the_end_is_refused_by_kind_and_records_nothing() {
        let mut model = three_pages();
        for edit in [
            PageEdit::Move {
                from: 3,
                count: 1,
                to: 0,
            },
            PageEdit::Move {
                from: 0,
                count: 1,
                to: 3,
            },
            PageEdit::Rotate {
                from: 2,
                count: 2,
                delta_degrees: 90,
            },
            PageEdit::Remove { index: 3, count: 1 },
            PageEdit::Remove {
                index: u32::MAX,
                count: 2,
            },
        ] {
            let err = model.apply_edit(edit).expect_err("out of range");
            assert_eq!(err.kind, ErrorKind::PageIndexOutOfBounds, "{edit:?}");
        }
        assert!(!model.can_undo(), "no refused edit was recorded");
    }

    #[test]
    fn a_move_names_the_position_that_does_not_exist() {
        let mut model = three_pages();
        let err = model
            .apply_edit(PageEdit::Move {
                from: 0,
                count: 1,
                to: 7,
            })
            .unwrap_err();
        assert!(err.message.contains('7'), "{}", err.message);
    }

    #[test]
    fn a_document_without_the_assembly_permission_refuses_every_page_edit() {
        let mut model = DocumentModel::open(
            &pdfs::restricted_document(),
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        for edit in [
            PageEdit::Move {
                from: 0,
                count: 1,
                to: 0,
            },
            PageEdit::Rotate {
                from: 0,
                count: 1,
                delta_degrees: 90,
            },
            PageEdit::Remove { index: 0, count: 1 },
            // The permission is asked first: an out-of-range position on a
            // refusing document is still a permission refusal.
            PageEdit::Remove { index: 9, count: 1 },
        ] {
            let err = model.apply_edit(edit).expect_err("refused");
            assert_eq!(err.kind, ErrorKind::EditNotPermitted, "{edit:?}");
        }
        assert!(!model.can_undo());
    }

    #[test]
    fn an_edit_the_model_rejects_is_unsupported() {
        let mut model = three_pages();
        // In range, so the constructor builds it; the model refuses a move
        // onto itself and a zero-page run on apply.
        for edit in [
            PageEdit::Move {
                from: 1,
                count: 1,
                to: 1,
            },
            PageEdit::Move {
                from: 0,
                count: 0,
                to: 1,
            },
        ] {
            let err = model.apply_edit(edit).unwrap_err();
            assert_eq!(err.kind, ErrorKind::UnsupportedOperation, "{edit:?}");
        }
        assert!(!model.can_undo());
    }

    #[test]
    fn the_owner_can_edit_and_the_save_keeps_the_encryption() {
        let mut model = DocumentModel::open(
            &pdfs::restricted_document(),
            &OpenCredentials::UserAndOwner {
                user: pdfs::USER_PASSWORD.into(),
                owner: pdfs::OWNER_PASSWORD.into(),
            },
        )
        .unwrap();
        model
            .apply_edit(PageEdit::Rotate {
                from: 0,
                count: 1,
                delta_degrees: 180,
            })
            .expect("the owner may turn pages");
        let saved = model.save_to_bytes(false).unwrap();

        let err = DocumentModel::open(&saved, &OpenCredentials::NoPassword)
            .err()
            .expect("still encrypted");
        assert_eq!(err.kind, ErrorKind::PasswordRequired);
        let reopened = DocumentModel::open(
            &saved,
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        assert_eq!(rotations(&reopened), [180]);
    }

    #[test]
    fn the_preview_shows_the_edits_and_leaves_the_session_alone() {
        let mut model = three_pages();
        model
            .apply_edit(PageEdit::Move {
                from: 2,
                count: 1,
                to: 0,
            })
            .unwrap();
        let preview = model.preview_bytes().expect("previews");
        assert_eq!(page_texts(&preview), ["three", "one", "two"]);
        assert!(model.can_undo(), "the edit is still queued");
        assert!(model.undo());
        assert_eq!(
            page_texts(&model.preview_bytes().unwrap()),
            ["one", "two", "three"]
        );
    }

    #[test]
    fn a_save_with_no_edits_reopens_as_it_was() {
        let model = three_pages();
        let saved = model.save_to_bytes(false).unwrap();
        assert_eq!(page_texts(&saved), ["one", "two", "three"]);
    }
}
