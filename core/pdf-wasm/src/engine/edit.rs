//! The `Engine`'s edit surface: page edits, undo/redo, save, and the preview
//! refresh (B26 T-224).
//!
//! The session rules live in [`crate::model`]'s edit module, over the core;
//! this is the JavaScript boundary plus the one step that needs pdfium:
//! reopening the preview snapshot as the document's render side.
//!
//! As in `pdf-ffi`, an edit changes the model at once (`pageCount`,
//! `pageDimensions`) but not the pixels or the text: those come from the
//! pdfium document, which still holds the last snapshot. Call
//! `refreshPreview` after every edit, undo or redo before rendering or
//! reading text again.

use wasm_bindgen::prelude::*;

use super::{js_error, Engine};
use crate::model::PageEdit;

/// One page edit for `Engine.applyEdit`. Positions are 0-based.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct EditCommand(PageEdit);

#[wasm_bindgen]
impl EditCommand {
    /// Moves the `count` pages starting at `from` so the first of them ends
    /// up at `to`.
    #[wasm_bindgen(js_name = movePages)]
    pub fn move_pages(from: u32, count: u32, to: u32) -> EditCommand {
        EditCommand(PageEdit::Move { from, count, to })
    }

    /// Turns each of the `count` pages starting at `from` clockwise by
    /// `delta_degrees`, as one undo step.
    #[wasm_bindgen(js_name = rotatePages)]
    pub fn rotate_pages(from: u32, count: u32, delta_degrees: i32) -> EditCommand {
        EditCommand(PageEdit::Rotate {
            from,
            count,
            delta_degrees,
        })
    }

    /// Removes the `count` pages starting at `index`, as one undo step.
    #[wasm_bindgen(js_name = removePages)]
    pub fn remove_pages(index: u32, count: u32) -> EditCommand {
        EditCommand(PageEdit::Remove { index, count })
    }
}

#[wasm_bindgen]
impl Engine {
    /// Records one page edit as one undo step. Refused with
    /// `EditNotPermitted` when the document withholds the assembly
    /// permission (asked first), `PageIndexOutOfBounds` naming a position
    /// that does not exist, or `UnsupportedOperation` when the edit cannot
    /// be recorded (a move onto itself, an empty run, or a rewrite the
    /// document's encryption does not allow).
    #[wasm_bindgen(js_name = applyEdit)]
    pub fn apply_edit(&mut self, doc: u32, edit: &EditCommand) -> Result<(), JsValue> {
        self.document_mut(doc)
            .and_then(|open| open.model.apply_edit(edit.0))
            .map_err(js_error)
    }

    /// Undoes the last edit; `false` when there was nothing to undo.
    pub fn undo(&mut self, doc: u32) -> Result<bool, JsValue> {
        self.document_mut(doc)
            .map(|open| open.model.undo())
            .map_err(js_error)
    }

    /// Redoes the last undone edit; `false` when there was nothing to redo.
    pub fn redo(&mut self, doc: u32) -> Result<bool, JsValue> {
        self.document_mut(doc)
            .map(|open| open.model.redo())
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = canUndo)]
    pub fn can_undo(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.can_undo())
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = canRedo)]
    pub fn can_redo(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.can_redo())
            .map_err(js_error)
    }

    /// The document with every edit applied, as a complete PDF computed from
    /// the bytes it was opened from, keeping its encryption. A save that
    /// would break a signature is refused with `SignaturesWouldBeInvalidated`
    /// unless `invalidateSignatures` says the user was told.
    #[wasm_bindgen(js_name = saveToBytes)]
    pub fn save_to_bytes(&self, doc: u32, invalidate_signatures: bool) -> Result<Vec<u8>, JsValue> {
        self.document(doc)
            .and_then(|open| open.model.save_to_bytes(invalidate_signatures))
            .map_err(js_error)
    }

    /// Reopens the document's render side from a snapshot of its unsaved
    /// edits, so pages, thumbnails and text show them. The edit history and
    /// the next save are untouched.
    #[wasm_bindgen(js_name = refreshPreview)]
    pub fn refresh_preview(&mut self, doc: u32) -> Result<(), JsValue> {
        self.reopen_render(doc).map_err(js_error)
    }
}
