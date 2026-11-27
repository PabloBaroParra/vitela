//! Editing text as blocks (B26 T-261): retype a whole text block — one line
//! of runs, or a paragraph of stacked lines — as one undo step, and insert a
//! new line of text where the reader points.
//!
//! Every rule is the core's, asked the way `pdf-ffi` asks it:
//!
//! - **A block retype** is `pdf_save::retype_text_block_of` over the runs
//!   [`DocumentModel::read_paragraph_blocks`] grouped, resolved against the
//!   pending read (so a paragraph retyped once is retyped again from what
//!   the reader sees, as `pdf-ffi`'s `read_page_paragraph_blocks` asks of a
//!   shell): a one-line block puts all of the text in its first run, a
//!   paragraph wraps each typed line to its width, lines it grows painted
//!   like its own (`Command::InsertTextRunLike`). The batch goes through
//!   [`DocumentModel::record_content`], so a second retype of the same
//!   paragraph folds into its step and a refusal takes a checkpoint. A
//!   one-line block led by a composite-font run is retyped in the inserted
//!   standard font, as the native shells retype one (`with_inserted_font`).
//!   Text with nothing to show takes the block off the page
//!   (`pdf_edit::remove_block`), as an emptied run editor does.
//! - **An insertion** is `Command::InsertTextRun`: one line in Helvetica
//!   (`pdf_edit`'s inserted standard font) at the size given, its top-left
//!   corner at the point, as the Android shell anchors it. Its `/Font` name
//!   is picked here, as the Android and Windows adapters pick one: random,
//!   and none the page's resources declare or a queued insertion claims —
//!   the core *reuses* a font already registered under the name it is
//!   given (`pdf_edit::ensure_font_resource`), which would paint the new
//!   line in that font. The box's width is the core's to measure when the
//!   pending read lays the insertion on (`pdf_edit::inserted_text_run_bbox`).

use pdf_document::{Command, ContentItemId, FontKind, PageId, Rect, TextRun};
use pdf_save::{IdGenerator, ImportedSources, RandomIdGenerator};

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// One text-block edit on one page. Positions are 0-based; run ids are the
/// ones the page's last content read handed out. Geometry is in PDF points
/// (y up).
#[derive(Debug, Clone, PartialEq)]
pub enum TextBlockEdit {
    /// Shows `text` in place of the block made of `runs` (its runs as
    /// [`DocumentModel::read_paragraph_blocks`] lists them, line by line
    /// from the top). Every `'\n'` is a line break the reader typed.
    Retype {
        page: u32,
        runs: Vec<u64>,
        text: String,
    },
    /// Paints `text` — one line — on the page in Helvetica at `size`
    /// points, its top-left corner at `at`.
    Insert {
        page: u32,
        at: (f64, f64),
        text: String,
        size: f64,
    },
}

/// The prefix of the `/Font` name an inserted line is registered under.
const INSERTED_FONT_PREFIX: &str = "VitelaText";

/// The sizes an inserted line may take, in points: the Windows shell's
/// bounds for its own insertion.
const INSERT_SIZE_PT: std::ops::RangeInclusive<f64> = 1.0..=72.0;

/// About how wide a Helvetica glyph is for its size: the width an insertion
/// is queued with, before the core measures it.
const AVERAGE_ADVANCE: f64 = 0.5;

impl DocumentModel {
    /// Records `edit` as one undo step, or folds a block retype into the
    /// queued step of the same block. A retype to the text the block shows
    /// records nothing.
    ///
    /// Refused, in this order: `EditNotPermitted` (or `UnsupportedOperation`
    /// for an encryption the rewrite could not keep) when the document's
    /// content cannot be edited; `TextExtractionDenied` when its runs cannot
    /// be read; `PageIndexOutOfBounds`; `UnsupportedOperation` for runs that
    /// are not one block of the page now, an insertion with nothing to show,
    /// a line break, a size outside 1–72 pt or a point that is not finite;
    /// then whatever the core's write refuses (`EncodingGap` for a character
    /// the font cannot show).
    pub fn apply_text_block_edit(&mut self, edit: TextBlockEdit) -> Result<(), EngineError> {
        pdf_save::ensure_content_editing(self.document.security.as_ref())?;
        self.ensure_text_extraction_allowed()?;
        let Some(command) = self.text_block_command(edit)? else {
            return Ok(());
        };
        self.record_content(command)
    }

    /// The core command for `edit`, or `None` when it would change nothing.
    fn text_block_command(&self, edit: TextBlockEdit) -> Result<Option<Command>, EngineError> {
        match edit {
            TextBlockEdit::Retype { page, runs, text } => self.block_retype(page, &runs, &text),
            TextBlockEdit::Insert {
                page,
                at,
                text,
                size,
            } => self.text_insertion(page, at, text, size).map(Some),
        }
    }

    fn block_retype(
        &self,
        page: u32,
        run_ids: &[u64],
        text: &str,
    ) -> Result<Option<Command>, EngineError> {
        let page_id = self.text_block_page_id(page)?;
        let block = self
            .read_paragraph_blocks(page)?
            .into_iter()
            .find(|block| block.run_ids == run_ids)
            .ok_or_else(|| {
                unsupported("the page has no text block of those runs now: read its content again")
            })?;
        if block.text == text {
            return Ok(None);
        }
        let sources = self.imported.pairs();
        let pending = pdf_save::read_pending_page_content_of(
            &self.document,
            page_id,
            &self.base,
            ImportedSources::new(&sources),
        )
        .map_err(|err| EngineError::new(ErrorKind::InvalidDocument, err.to_string()))?
        .text_runs;
        let runs: Vec<TextRun> = block
            .run_ids
            .iter()
            .filter_map(|id| pending.iter().find(|run| run.id.0 == *id).cloned())
            .collect();
        if text.trim().is_empty() {
            return Ok(pdf_edit::remove_block(&runs));
        }
        let with_inserted_font = block.lines.len() == 1
            && runs
                .first()
                .is_some_and(|first| first.font_kind == FontKind::EmbeddedComposite);
        pdf_save::retype_text_block_of(
            &self.document,
            &self.base,
            ImportedSources::new(&sources),
            &runs,
            text,
            with_inserted_font,
        )
        .map_err(EngineError::from)
    }

    fn text_insertion(
        &self,
        page: u32,
        at: (f64, f64),
        text: String,
        size: f64,
    ) -> Result<Command, EngineError> {
        let page_id = self.text_block_page_id(page)?;
        if text.trim().is_empty() {
            return Err(unsupported("an insertion has text to show"));
        }
        if text.contains(['\n', '\r']) {
            return Err(unsupported("an insertion is one line of text"));
        }
        if !INSERT_SIZE_PT.contains(&size) {
            return Err(unsupported(
                "an insertion's size is between 1 and 72 points",
            ));
        }
        if !(at.0.is_finite() && at.1.is_finite()) {
            return Err(unsupported("text is inserted at a finite point"));
        }
        let width = text.chars().count() as f64 * size * AVERAGE_ADVANCE;
        Ok(Command::InsertTextRun(TextRun {
            id: ContentItemId(0),
            page: page_id,
            bbox: Rect {
                x: at.0,
                y: at.1 - size,
                width,
                height: size,
            },
            resource_font_name: self.unused_font_name(page_id)?,
            font_kind: FontKind::Standard14,
            text,
        }))
    }

    /// A `/Font` name page `page` does not declare and no queued insertion
    /// claims, random so it misses what an undone insertion left behind.
    fn unused_font_name(&self, page: PageId) -> Result<String, EngineError> {
        let sources = self.imported.pairs();
        let declared = pdf_save::page_font_families_of(
            &self.document,
            page,
            &self.base,
            ImportedSources::new(&sources),
        )
        .map_err(|err| EngineError::new(ErrorKind::InvalidDocument, err.to_string()))?;
        let claimed = |name: &str| {
            declared.contains_key(name)
                || self.document.pending_edits.entries().iter().any(|command| {
                    matches!(
                        command,
                        Command::InsertTextRun(run) if run.resource_font_name == name
                    )
                })
        };
        let ids = RandomIdGenerator::new();
        loop {
            let suffix: String = ids
                .next_id()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let name = format!("{INSERTED_FONT_PREFIX}{suffix}");
            if !claimed(&name) {
                return Ok(name);
            }
        }
    }

    fn text_block_page_id(&self, page_index: u32) -> Result<PageId, EngineError> {
        self.document
            .pages
            .get(page_index as usize)
            .map(|page| page.id)
            .ok_or_else(|| EngineError::page_out_of_bounds(page_index))
    }
}

fn unsupported(message: &str) -> EngineError {
    EngineError::new(ErrorKind::UnsupportedOperation, message)
}

#[cfg(test)]
mod tests;
