//! Editing several content items at once (B26 T-262): what a marquee or a
//! SHIFT/CTRL+click gathers on one page — text runs, images, vector paths —
//! moved or removed as one undo step. `pdf-ffi`'s `move_content_selection`
//! and `remove_content_selection`.
//!
//! Every rule is the core's, asked the way `pdf-ffi` asks it:
//!
//! - **Members** are named by the ids the page's last content read handed
//!   out and resolved here against the pending read — each item exactly as
//!   the reader sees it, which is what a native shell sends back.
//! - **The batches** are `pdf_edit::move_selection` and
//!   `pdf_edit::remove_selection`, recorded through
//!   [`DocumentModel::record_content`]: a selection dragged twice before a
//!   save folds into one step, and a refusal takes a checkpoint.
//! - **A pending paste** (T-209): a selection that is exactly what one
//!   pending paste added moves the paste itself
//!   (`pdf_save::move_pending_paste`), so pasting and then nudging it into
//!   place is still one step; part of one is refused
//!   (`pdf_save::part_of_paste`) until a save writes it. Removing all of one
//!   goes through a checkpoint, refused where the session may not take one
//!   (`pdf_save::paste_not_deletable`).

use pdf_document::{ContentItemId, PageId};
use pdf_edit::Selection;
use pdf_save::{ImportedSources, PastedMembers};

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// The members of a selection on one page, by the ids the page's last
/// content read handed out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ContentMembers {
    pub runs: Vec<u64>,
    pub images: Vec<u64>,
    pub paths: Vec<u64>,
}

impl ContentMembers {
    /// Every member's id, runs first.
    fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.runs
            .iter()
            .chain(&self.images)
            .chain(&self.paths)
            .copied()
    }
}

/// One edit of a selection on page `page` (0-based). Geometry is in PDF
/// points (y up).
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionEdit {
    /// Shifts every member by `dx, dy`, each keeping its size.
    Move {
        page: u32,
        members: ContentMembers,
        dx: f64,
        dy: f64,
    },
    /// Takes every member off the page.
    Remove { page: u32, members: ContentMembers },
}

impl DocumentModel {
    /// Records `edit` as one undo step, or folds a move into the queued step
    /// of the same selection. A zero move records nothing.
    ///
    /// Refused, in this order: `EditNotPermitted` (or
    /// `UnsupportedOperation` for an encryption the rewrite could not keep)
    /// when the document's content cannot be edited; `TextExtractionDenied`
    /// when its content cannot be read; `UnsupportedOperation` for a
    /// distance that is not finite; `PageIndexOutOfBounds`;
    /// `UnsupportedOperation` for an empty selection, an id the page does not
    /// hold now, part of a pending paste, or a pending paste removed where no
    /// checkpoint may be taken; then whatever the core's write refuses.
    pub fn apply_selection_edit(&mut self, edit: SelectionEdit) -> Result<(), EngineError> {
        pdf_save::ensure_content_editing(self.document.security.as_ref())?;
        self.ensure_text_extraction_allowed()?;
        match edit {
            SelectionEdit::Move {
                page,
                members,
                dx,
                dy,
            } => self.move_selection(page, &members, dx, dy),
            SelectionEdit::Remove { page, members } => self.remove_selection(page, &members),
        }
    }

    fn move_selection(
        &mut self,
        page: u32,
        members: &ContentMembers,
        dx: f64,
        dy: f64,
    ) -> Result<(), EngineError> {
        if !(dx.is_finite() && dy.is_finite()) {
            return Err(unsupported("Selection offsets must be finite."));
        }
        let selection = self.selection_of(page, members)?;
        if dx == 0.0 && dy == 0.0 {
            return Ok(());
        }
        if let PastedMembers::Whole(index) = self.pasted_members(&selection)? {
            return self.track_content_step(true, true, |model| {
                pdf_save::move_pending_paste(&mut model.document, index, dx, dy)
                    .map_err(EngineError::from)
            });
        }
        let command = pdf_edit::move_selection(&selection, dx, dy)
            .ok_or_else(|| unsupported("The selection is empty."))?;
        self.record_content(command)
    }

    fn remove_selection(&mut self, page: u32, members: &ContentMembers) -> Result<(), EngineError> {
        let selection = self.selection_of(page, members)?;
        if let PastedMembers::Whole(_) = self.pasted_members(&selection)? {
            if !pdf_save::checkpoint::may_checkpoint(self) {
                return Err(pdf_save::paste_not_deletable().into());
            }
        }
        let command = pdf_edit::remove_selection(&selection)
            .ok_or_else(|| unsupported("The selection is empty."))?;
        self.record_content(command)
    }

    /// `pdf_save::pasted_members` over `selection`'s ids.
    fn pasted_members(&self, selection: &Selection) -> Result<PastedMembers, EngineError> {
        let ids = pdf_save::selection_ids(selection);
        pdf_save::pasted_members(&self.document, &ids).map_err(Into::into)
    }

    /// `members` of page `page_index`, each as the reader sees it now.
    /// Refused with `PageIndexOutOfBounds`, or `UnsupportedOperation` for no
    /// members or an id the page does not hold now.
    pub(super) fn selection_of(
        &self,
        page_index: u32,
        members: &ContentMembers,
    ) -> Result<Selection, EngineError> {
        let page = self.selection_page_id(page_index)?;
        if members.ids().next().is_none() {
            return Err(unsupported("The selection is empty."));
        }
        let sources = self.imported.pairs();
        let content = pdf_save::read_pending_page_content_of(
            &self.document,
            page,
            &self.base,
            ImportedSources::new(&sources),
        )
        .map_err(|err| EngineError::new(ErrorKind::InvalidDocument, err.to_string()))?;
        Ok(Selection {
            runs: pick(&content.text_runs, &members.runs, |run| run.id)?,
            images: pick(&content.images, &members.images, |image| image.id)?,
            paths: pick(&content.paths, &members.paths, |path| path.id)?,
        })
    }

    fn selection_page_id(&self, page_index: u32) -> Result<PageId, EngineError> {
        self.document
            .pages
            .get(page_index as usize)
            .map(|page| page.id)
            .ok_or_else(|| EngineError::page_out_of_bounds(page_index))
    }
}

/// The items of `items` named by `ids`, in that order; a stale id refuses.
fn pick<T: Clone>(
    items: &[T],
    ids: &[u64],
    id_of: impl Fn(&T) -> ContentItemId,
) -> Result<Vec<T>, EngineError> {
    ids.iter()
        .map(|id| {
            items
                .iter()
                .find(|item| id_of(item).0 == *id)
                .cloned()
                .ok_or_else(|| {
                    unsupported("the page has no item with that id now: read its content again")
                })
        })
        .collect()
}

fn unsupported(message: &str) -> EngineError {
    EngineError::new(ErrorKind::UnsupportedOperation, message)
}

#[cfg(test)]
mod tests;
