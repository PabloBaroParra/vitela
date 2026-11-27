//! The page-content model over the core (B26 T-258): what a page paints —
//! its text runs, images and vector paths — as the reader sees it, the text
//! blocks and paragraphs those runs make, a run's style, the page's fonts,
//! and recording a content edit through the session's checkpoints.
//!
//! Every rule is the core's, asked the way `pdf-ffi` asks it:
//!
//! - **The gates.** Every read is behind the text-extraction permission, as
//!   `pdf-ffi`'s `read_page_content` is; editing is
//!   `pdf_save::content_editing_allowed` (the modify bit, and whether the
//!   full rewrite a content edit forces could keep the file's encryption).
//! - **Pending-aware reads.** `pdf_save::read_pending_page_content_of` lays
//!   every queued content edit over the parse, so a retyped run reads with
//!   its new text and an inserted one with a synthetic id. The parse is
//!   never cached here (Batch 21 decision 2): a shell asks per page, and
//!   caches by page id, paint revision and base generation.
//! - **Grouping** is `pdf_edit::group_text_lines` (one line per block) and
//!   `pdf_edit::group_text_paragraphs` (stacked lines joined), with members
//!   mapped back to the runs by id, as `pdf-ffi`'s `text_blocks.rs` does.
//! - **Recording** is `pdf_save::content_edit::record_content` over this
//!   session as a `CheckpointSession` ([`super::checkpoint`]).
//!
//! What stays here is the boundary's vocabulary: items name page
//! *positions*, and a run is asked about by its id on its page.

use std::collections::BTreeMap;

use pdf_document::{Command, FontKind, ImageSource, PageId, PathPaint, Rect, TextRun};
use pdf_edit::{Alignment, TextBlock};
use pdf_save::{EditClass, ImportedSources};

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// How a run looks on its page (`pdf_edit::TextRunStyle`): its font's
/// `/BaseFont`, the size a reader sees, the fill colour, spacing and face.
pub type TextRunStyleView = pdf_edit::TextRunStyle;

/// One text run as the reader sees it, on the page position it was asked by.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentRunView {
    /// Valid against the parse it came from: re-read after an edit, or when
    /// [`DocumentModel::content_base_generation`] changes. Below 2^53, so it
    /// crosses to JavaScript as a number.
    pub id: u64,
    pub page: u32,
    /// Page space, points, bottom-left origin.
    pub bbox: Rect,
    /// The font's key in the page's `/Resources /Font`.
    pub resource_font_name: String,
    pub font_kind: FontKind,
    pub text: String,
    /// Whether the run's text decoded fully
    /// (`pdf_save::text_run_text_is_decoded`): `false` when its font has no
    /// usable `/ToUnicode` for some of its codes, so a retype could not know
    /// what it replaces.
    pub editable: bool,
}

/// One image a page paints.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentImageView {
    pub id: u64,
    pub page: u32,
    pub bbox: Rect,
    /// Its samples sit in the content stream (`BI … EI`), not in an XObject.
    pub inline: bool,
}

/// One vector path a page paints: a rule, a border, a shaded box.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentPathView {
    pub id: u64,
    pub page: u32,
    /// The geometry's box, the stroke's width left out: a hairline rule has a
    /// box of zero height.
    pub bbox: Rect,
    pub paint: PathPaint,
}

/// Everything one page paints, in paint order per kind.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageContentView {
    pub runs: Vec<ContentRunView>,
    pub images: Vec<ContentImageView>,
    pub paths: Vec<ContentPathView>,
}

/// One line of a [`TextBlockView`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextLineView {
    /// The member runs' ids, left to right.
    pub run_ids: Vec<u64>,
    pub bbox: Rect,
    pub baseline: f64,
    pub text: String,
}

/// A block of runs a reader sees as one piece of text: one line, or a
/// paragraph of stacked lines.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBlockView {
    /// The member runs' ids, line by line from the top.
    pub run_ids: Vec<u64>,
    pub bbox: Rect,
    /// The lines' text joined with `'\n'`.
    pub text: String,
    pub lines: Vec<TextLineView>,
    /// The mean baseline-to-baseline distance; `None` for one line.
    pub leading: Option<f64>,
    pub alignment: Alignment,
}

/// The gate's class: a content edit rewrites a page's own content.
const CONTENT: EditClass = EditClass {
    annotation: false,
    document_assembly: false,
    structural_form: false,
    content: true,
    page_structure: false,
};

impl DocumentModel {
    /// Whether the document lets its page content be rewritten
    /// (`pdf_save::content_editing_allowed`).
    pub fn content_editing_allowed(&self) -> bool {
        pdf_save::content_editing_allowed(self.document.security.as_ref())
    }

    /// Why this document's content cannot be edited, as the core's
    /// sentence, or `None` when it can. Two gates, in this order: the edit
    /// itself (`pdf_save::check_edit` for a content edit — the same answer
    /// as [`Self::content_editing_allowed`], worded), then the reads an
    /// editor needs, which the text-extraction permission withholds.
    pub fn content_editing_refusal(&self) -> Option<String> {
        if let Err(refusal) = pdf_save::check_edit(self.document.security.as_ref(), CONTENT) {
            return Some(refusal.to_string());
        }
        self.ensure_text_extraction_allowed()
            .err()
            .map(|refused| refused.message)
    }

    /// What page `page_index` paints, every pending content edit laid on
    /// top. Refused with `TextExtractionDenied` when the document withholds
    /// text extraction.
    pub fn read_page_content(&self, page_index: u32) -> Result<PageContentView, EngineError> {
        self.ensure_text_extraction_allowed()?;
        let page = self.content_page_id(page_index)?;
        let sources = self.imported.pairs();
        let content = pdf_save::read_pending_page_content_of(
            &self.document,
            page,
            &self.base,
            ImportedSources::new(&sources),
        )
        .map_err(read_failure)?;
        Ok(PageContentView {
            runs: content
                .text_runs
                .iter()
                .map(|run| run_view(run, page_index))
                .collect(),
            images: content
                .images
                .iter()
                .map(|image| ContentImageView {
                    id: image.id.0,
                    page: page_index,
                    bbox: image.bbox,
                    inline: image.source == ImageSource::Inline,
                })
                .collect(),
            paths: content
                .paths
                .iter()
                .map(|path| ContentPathView {
                    id: path.id.0,
                    page: page_index,
                    bbox: path.bbox,
                    paint: path.paint,
                })
                .collect(),
        })
    }

    /// The text blocks on page `page_index`, one line each
    /// (`pdf_edit::group_text_lines`). Same gate as
    /// [`Self::read_page_content`].
    pub fn read_text_blocks(&self, page_index: u32) -> Result<Vec<TextBlockView>, EngineError> {
        self.read_blocks(page_index, pdf_edit::group_text_lines)
    }

    /// The text blocks on page `page_index` with stacked lines joined into
    /// paragraphs (`pdf_edit::group_text_paragraphs`; a paragraph with a
    /// composite-font run reads one block per line). Same gate as
    /// [`Self::read_page_content`].
    pub fn read_paragraph_blocks(
        &self,
        page_index: u32,
    ) -> Result<Vec<TextBlockView>, EngineError> {
        self.read_blocks(page_index, pdf_edit::group_text_paragraphs)
    }

    /// How run `run_id` on page `page_index` looks
    /// (`pdf_save::text_run_style_of`), or `None` when the file cannot say:
    /// a run only a pending insertion added, or a page with no content.
    /// `UnsupportedOperation` for an id the page does not hold now.
    pub fn text_run_style(
        &self,
        page_index: u32,
        run_id: u64,
    ) -> Result<Option<TextRunStyleView>, EngineError> {
        self.ensure_text_extraction_allowed()?;
        let page = self.content_page_id(page_index)?;
        let run = self.pending_run(page, run_id)?;
        let sources = self.imported.pairs();
        Ok(pdf_save::text_run_style_of(
            &self.document,
            &self.base,
            ImportedSources::new(&sources),
            &run,
        )
        .ok()
        .flatten())
    }

    /// The `/BaseFont` of each font page `page_index` declares, keyed by the
    /// resource name its runs report — raw, subset prefix included
    /// (`pdf_save::page_font_families_of`). Same gate as
    /// [`Self::read_page_content`].
    pub fn page_font_families(
        &self,
        page_index: u32,
    ) -> Result<BTreeMap<String, String>, EngineError> {
        self.ensure_text_extraction_allowed()?;
        let page = self.content_page_id(page_index)?;
        let sources = self.imported.pairs();
        pdf_save::page_font_families_of(
            &self.document,
            page,
            &self.base,
            ImportedSources::new(&sources),
        )
        .map_err(read_failure)
    }

    /// Gates and records the page-content edit `command` as one undo step —
    /// or folded into the queued edit of the same item — checkpointing once
    /// when the queue refuses it and the session may take one
    /// (`pdf_save::content_edit::record_content`). A `Command::Batch` goes
    /// through the batch recorder. Nothing is recorded when it refuses.
    pub fn record_content(&mut self, command: Command) -> Result<(), EngineError> {
        pdf_save::ensure_content_editing(self.document.security.as_ref())?;
        let shows = pdf_save::shows_in_session_preview(&command);
        let paints_in_place = command.is_content_edit();
        let generation = self.checkpoints.generation();
        // Kept to find which entry an amend rewrote: the log only says that
        // its depth did not change.
        let before = self.document.pending_edits.entries().to_vec();
        pdf_save::content_edit::record_content(self, command, apply_content)?;
        let after = self.document.pending_edits.entries();
        if self.checkpoints.generation() != generation || after.len() > before.len() {
            // A step of its own — appended, or the retried edit a checkpoint
            // starts its queue with.
            self.preview.recorded(shows, paints_in_place);
        } else if let Some(index) = before.iter().zip(after).position(|(was, now)| was != now) {
            self.preview.amended(after.len() - index);
        }
        Ok(())
    }

    fn read_blocks(
        &self,
        page_index: u32,
        group: fn(&[TextRun]) -> Vec<TextBlock>,
    ) -> Result<Vec<TextBlockView>, EngineError> {
        let runs = self.read_page_content(page_index)?.runs;
        let core: Vec<TextRun> = runs.iter().map(core_run).collect();
        let known = |ids: &[pdf_document::ContentItemId]| -> Vec<u64> {
            ids.iter()
                .map(|id| id.0)
                .filter(|id| runs.iter().any(|run| run.id == *id))
                .collect()
        };
        Ok(group(&core)
            .into_iter()
            .map(|block| TextBlockView {
                run_ids: known(&block.runs),
                bbox: block.bbox,
                text: block.text,
                lines: block
                    .lines
                    .into_iter()
                    .map(|line| TextLineView {
                        run_ids: known(&line.runs),
                        bbox: line.bbox,
                        baseline: line.baseline,
                        text: line.text,
                    })
                    .collect(),
                leading: block.leading,
                alignment: block.alignment,
            })
            .collect())
    }

    /// Run `run_id` on `page` as the reader sees it now.
    fn pending_run(&self, page: PageId, run_id: u64) -> Result<TextRun, EngineError> {
        let sources = self.imported.pairs();
        pdf_save::read_pending_page_content_of(
            &self.document,
            page,
            &self.base,
            ImportedSources::new(&sources),
        )
        .map_err(read_failure)?
        .text_runs
        .into_iter()
        .find(|run| run.id.0 == run_id)
        .ok_or_else(|| {
            EngineError::new(
                ErrorKind::UnsupportedOperation,
                "the page has no text run with that id now: read its content again",
            )
        })
    }

    fn content_page_id(&self, page_index: u32) -> Result<PageId, EngineError> {
        self.document
            .pages
            .get(page_index as usize)
            .map(|page| page.id)
            .ok_or_else(|| EngineError::page_out_of_bounds(page_index))
    }
}

/// The recording path a content edit takes, single or batch — what
/// `pdf-ffi`'s `apply_content_command` and `apply_batch_command` do. Leaves
/// the session untouched when it refuses, which is what lets a checkpoint
/// retry it.
fn apply_content(model: &mut DocumentModel, command: Command) -> Result<(), EngineError> {
    let sources = model.imported.pairs();
    let sources = ImportedSources::new(&sources);
    let recorded = if matches!(command, Command::Batch(_)) {
        pdf_save::record_content_batch(&mut model.document, &model.base, sources, command)
    } else {
        pdf_save::record_content_command(&mut model.document, &model.base, sources, command)
    };
    recorded.map_err(Into::into)
}

fn run_view(run: &TextRun, page_index: u32) -> ContentRunView {
    ContentRunView {
        id: run.id.0,
        page: page_index,
        bbox: run.bbox,
        resource_font_name: run.resource_font_name.clone(),
        font_kind: run.font_kind,
        text: run.text.clone(),
        editable: pdf_save::text_run_text_is_decoded(run),
    }
}

/// The core run a view was made from. The page is only grouped, never
/// resolved, so its position stands in for its id.
fn core_run(view: &ContentRunView) -> TextRun {
    TextRun {
        id: pdf_document::ContentItemId(view.id),
        page: PageId(view.page),
        bbox: view.bbox,
        resource_font_name: view.resource_font_name.clone(),
        font_kind: view.font_kind,
        text: view.text.clone(),
    }
}

/// A page whose content the core could not read: not a save, so not
/// `SaveFailed`.
fn read_failure(err: pdf_save::SaveError) -> EngineError {
    EngineError::new(ErrorKind::InvalidDocument, err.to_string())
}

#[cfg(test)]
mod tests;
