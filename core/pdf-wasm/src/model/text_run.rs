//! Editing the text runs a page paints (B26 T-259): retype, move, remove and
//! restyle one run — or restyle several as one step — and the two reads an
//! editor over a run needs: how it looks now (`pdf_save::pending_text_run_look_of`,
//! what the bold / italic / underline toggles show) and the font program its
//! font embeds (`pdf_save::text_run_font_program_of`, what a camouflaged
//! editor draws in).
//!
//! Every rule is the core's. A run is named by its page position and the id
//! [`DocumentModel::read_page_content`] handed out; it is resolved here
//! against the pending read — the run exactly as the reader sees it, which
//! is what `pdf_save::record_content_command` folds against — and the edit is
//! the core command a native shell records, through
//! [`DocumentModel::record_content`]: one undo step, a second retype or move
//! of the same run amends its queued step, and a checkpoint is taken when
//! the queue refuses it. The choice of retype command is the native shells':
//! a composite-font run is retyped in the inserted standard font
//! (`Command::ReplaceTextRunWithInsertedFont`), every other run in its own.

use pdf_document::{Command, FontKind, PageId, Rect, TextRun, TextRunLook};
use pdf_edit::{FontProgram, RunLook};
use pdf_save::ImportedSources;

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// One edit of the text runs on one page. Positions are 0-based; run ids are
/// the ones the page's last content read handed out.
#[derive(Debug, Clone, PartialEq)]
pub enum TextRunEdit {
    /// Shows `text` in place of the run's text, in its font, size and place.
    Retype { page: u32, run: u64, text: String },
    /// Shifts the run by `dx, dy` PDF points (y up).
    Move {
        page: u32,
        run: u64,
        dx: f64,
        dy: f64,
    },
    /// Takes the run off the page.
    Remove { page: u32, run: u64 },
    /// Gives every run of `runs` the look `look` — one undo step for all of
    /// them. A run that already looks that way is left alone.
    Restyle {
        page: u32,
        runs: Vec<u64>,
        look: TextRunLook,
    },
}

impl DocumentModel {
    /// Records `edit` as one undo step, or folds it into the queued step of
    /// the same run. An edit that changes nothing (the same text, a zero
    /// move, the look a run has) records nothing.
    ///
    /// Refused, in this order: `EditNotPermitted` (or `UnsupportedOperation`
    /// for an encryption the rewrite could not keep) when the document's
    /// content cannot be edited; `TextExtractionDenied` when its runs cannot
    /// be read; `PageIndexOutOfBounds`; `UnsupportedOperation` for a run id
    /// the page does not hold now; then whatever the core's write refuses
    /// (`EncodingGap` for a character the run's font cannot show).
    pub fn apply_text_run_edit(&mut self, edit: TextRunEdit) -> Result<(), EngineError> {
        pdf_save::ensure_content_editing(self.document.security.as_ref())?;
        self.ensure_text_extraction_allowed()?;
        let Some(command) = self.text_run_command(edit)? else {
            return Ok(());
        };
        self.record_content(command)
    }

    /// How run `run_id` on page `page_index` looks now, every pending
    /// restyle included, and which styles can come off it
    /// (`pdf_save::pending_text_run_look_of`). `None` when the file cannot
    /// say. Same gates as [`Self::read_page_content`].
    pub fn text_run_look(
        &self,
        page_index: u32,
        run_id: u64,
    ) -> Result<Option<RunLook>, EngineError> {
        self.ensure_text_extraction_allowed()?;
        let run = self.text_run_at(page_index, run_id)?;
        self.look_of(&run)
    }

    /// The font program run `run_id`'s font embeds, or `None` for a font
    /// that embeds none (a standard-14 font) or whose program the core does
    /// not read (`pdf_save::text_run_font_program_of`). Same gates as
    /// [`Self::read_page_content`].
    pub fn text_run_font_program(
        &self,
        page_index: u32,
        run_id: u64,
    ) -> Result<Option<FontProgram>, EngineError> {
        self.ensure_text_extraction_allowed()?;
        let run = self.text_run_at(page_index, run_id)?;
        let sources = self.imported.pairs();
        // A run only a pending insertion holds is in no file yet: it has no
        // program to read.
        Ok(pdf_save::text_run_font_program_of(
            &self.document,
            &self.base,
            ImportedSources::new(&sources),
            &run,
        )
        .ok()
        .flatten())
    }

    /// The core command for `edit`, or `None` when it would change nothing.
    fn text_run_command(&self, edit: TextRunEdit) -> Result<Option<Command>, EngineError> {
        Ok(match edit {
            TextRunEdit::Retype { page, run, text } => {
                let item = self.text_run_at(page, run)?;
                if item.text == text {
                    return Ok(None);
                }
                Some(if item.font_kind == FontKind::EmbeddedComposite {
                    Command::ReplaceTextRunWithInsertedFont { item, after: text }
                } else {
                    Command::ReplaceTextRunContent { item, after: text }
                })
            }
            TextRunEdit::Move { page, run, dx, dy } => {
                if !(dx.is_finite() && dy.is_finite()) {
                    return Err(EngineError::new(
                        ErrorKind::UnsupportedOperation,
                        "a move is a finite distance",
                    ));
                }
                let item = self.text_run_at(page, run)?;
                if dx == 0.0 && dy == 0.0 {
                    return Ok(None);
                }
                let to = Rect {
                    x: item.bbox.x + dx,
                    y: item.bbox.y + dy,
                    ..item.bbox
                };
                Some(Command::MoveTextRun { item, to })
            }
            TextRunEdit::Remove { page, run } => {
                Some(Command::RemoveTextRun(self.text_run_at(page, run)?))
            }
            TextRunEdit::Restyle { page, runs, look } => {
                let mut restyles = Vec::new();
                for run in runs {
                    let item = self.text_run_at(page, run)?;
                    let from = self
                        .look_of(&item)?
                        .map(|read| read.look)
                        .unwrap_or_default();
                    if from != look {
                        restyles.push(Command::RestyleTextRun {
                            item,
                            from,
                            to: look,
                        });
                    }
                }
                match restyles.len() {
                    0 => None,
                    1 => restyles.pop(),
                    _ => Some(Command::Batch(restyles)),
                }
            }
        })
    }

    fn look_of(&self, run: &TextRun) -> Result<Option<RunLook>, EngineError> {
        let sources = self.imported.pairs();
        pdf_save::pending_text_run_look_of(
            &self.document,
            &self.base,
            ImportedSources::new(&sources),
            run,
        )
        .map_err(|err| EngineError::new(ErrorKind::InvalidDocument, err.to_string()))
    }

    /// Run `run_id` on page `page_index` as the reader sees it now.
    fn text_run_at(&self, page_index: u32, run_id: u64) -> Result<TextRun, EngineError> {
        let page: PageId = self
            .document
            .pages
            .get(page_index as usize)
            .map(|page| page.id)
            .ok_or_else(|| EngineError::page_out_of_bounds(page_index))?;
        self.pending_run(page, run_id)
    }
}

#[cfg(test)]
mod tests;
