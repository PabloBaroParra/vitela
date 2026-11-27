//! The content clipboard (B26 T-262): copy a selection out of a page as a
//! clip, and paste a clip onto a page as one undo step. `pdf-ffi`'s
//! `copy_content_selection`, `cut_content_selection` and `paste_content`.
//!
//! The clip is the core's (`pdf_edit::clipboard`): a self-contained one-page
//! PDF and the box its items covered, plus whether the session it came from
//! held anything from an encrypted PDF. Nothing here keeps it: the shell
//! holds it, so it outlives the page — and the document — it was copied
//! from, and hands it back unchanged to paste.
//!
//! - **Copy** is `pdf_save::checkpoint::copy_selection`: members with an
//!   unsaved edit are copied as they read now, from the queue written in
//!   memory, never swapped in. It needs the text-extraction permission; a
//!   copy made for a cut needs `pdf_save::content_cut_allowed` (extraction
//!   and content editing) and says `ContentEditRefusal::Cut` otherwise,
//!   before anything is read. The cut's removal is an ordinary
//!   [`super::SelectionEdit::Remove`] that follows.
//! - **Paste** is `pdf_save::record_paste`: the clip's box with its top-left
//!   corner at the point, one pending entry until the save, its items read
//!   back with synthetic ids (`pending`) so they can be selected and dragged
//!   into place as the paste itself.

use pdf_document::ContentClip;
use pdf_save::ImportedSources;

use super::{ContentMembers, DocumentModel};
use crate::error::{EngineError, ErrorKind};

impl DocumentModel {
    /// Copies `members` of page `page_index` into a clip, leaving the page
    /// as it is; `for_cut` asks the cut's permission instead.
    ///
    /// Refused, in this order: `EditNotPermitted`
    /// (`ContentEditRefusal::Cut`) for a cut the document does not allow, or
    /// `TextExtractionDenied` for a copy; `PageIndexOutOfBounds`;
    /// `UnsupportedOperation` for an empty selection or an id the page does
    /// not hold now; then whatever the core's copy refuses.
    pub fn copy_content_selection(
        &self,
        page_index: u32,
        members: &ContentMembers,
        for_cut: bool,
    ) -> Result<ContentClip, EngineError> {
        if for_cut {
            if !pdf_save::content_cut_allowed(self.document.security.as_ref()) {
                return Err(pdf_save::ContentEditRefusal::Cut.into());
            }
        } else {
            self.ensure_text_extraction_allowed()?;
        }
        let selection = self.selection_of(page_index, members)?;
        let sources = self.imported.pairs();
        pdf_save::checkpoint::copy_selection(self, ImportedSources::new(&sources), &selection)
            .map_err(Into::into)
    }

    /// Writes `clip` onto page `page_index`, its box's top-left corner at
    /// `at` (PDF points, y up), as one undo step.
    ///
    /// Refused, in this order: `UnsupportedOperation` for a point that is
    /// not finite; `EditNotPermitted` (or `UnsupportedOperation` for an
    /// encryption the rewrite could not keep) when the document's content
    /// cannot be edited; `PageIndexOutOfBounds`; then whatever the core's
    /// validation of the paste refuses.
    pub fn paste_content(
        &mut self,
        page_index: u32,
        clip: ContentClip,
        at: (f64, f64),
    ) -> Result<(), EngineError> {
        let (left, top) = at;
        if !(left.is_finite() && top.is_finite()) {
            return Err(EngineError::new(
                ErrorKind::UnsupportedOperation,
                "Paste coordinates must be finite.",
            ));
        }
        pdf_save::ensure_content_editing(self.document.security.as_ref())?;
        let page = self
            .document
            .pages
            .get(page_index as usize)
            .map(|page| page.id)
            .ok_or_else(|| EngineError::page_out_of_bounds(page_index))?;
        // A paste is a page-content step the preview shows and repaints in
        // place (`shows_in_session_preview`, `Command::is_content_edit`).
        self.track_content_step(true, true, |model| {
            let sources = model.imported.pairs();
            pdf_save::record_paste(
                &mut model.document,
                &model.base,
                ImportedSources::new(&sources),
                page,
                clip,
                left,
                top,
            )
            .map_err(EngineError::from)
        })
    }
}

#[cfg(test)]
mod tests;
