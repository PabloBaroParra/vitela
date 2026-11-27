//! The `Engine`'s text-block editing surface (B26 T-261): the two edits
//! `applyEdit` records on text as a block — retype a whole block (one line,
//! or a paragraph) a page's paragraph read handed out, and insert a new line
//! of text. `pdf-ffi`'s `retype_text_block` and
//! `FfiEditCommand::InsertTextRun`.
//!
//! The rules live in [`crate::model`]'s text-block module, over the core;
//! this is the JavaScript boundary. Run ids cross as numbers (below 2^53);
//! geometry is in PDF points (y up).

use wasm_bindgen::prelude::*;

use super::content::item_id;
use super::edit::Edit;
use super::{js_error, EditCommand};
use crate::model::TextBlockEdit;

#[wasm_bindgen]
impl EditCommand {
    /// Shows `text` in place of the block made of runs `run_ids` on page
    /// `page` — its runs as `readParagraphBlocks` lists them. A paragraph
    /// wraps every typed line to its width; a one-line block puts all of it
    /// in its first run; text with nothing to show removes the block. A
    /// second retype of the same block amends its step; the text it already
    /// shows records nothing.
    #[wasm_bindgen(js_name = retypeTextBlock)]
    pub fn retype_text_block(
        page: u32,
        run_ids: Vec<f64>,
        text: String,
    ) -> Result<EditCommand, JsValue> {
        let runs = run_ids
            .into_iter()
            .map(item_id)
            .collect::<Result<Vec<_>, _>>()
            .map_err(js_error)?;
        Ok(EditCommand(Edit::TextBlock(TextBlockEdit::Retype {
            page,
            runs,
            text,
        })))
    }

    /// Paints `text`, one line, on page `page` in Helvetica at `size`
    /// points (1–72), its top-left corner at `x, y` (PDF points).
    #[wasm_bindgen(js_name = insertText)]
    pub fn insert_text(page: u32, x: f64, y: f64, text: String, size: f64) -> EditCommand {
        EditCommand(Edit::TextBlock(TextBlockEdit::Insert {
            page,
            at: (x, y),
            text,
            size,
        }))
    }
}
