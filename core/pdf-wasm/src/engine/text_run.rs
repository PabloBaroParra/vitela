//! The `Engine`'s text-run editing surface (B26 T-259): the four edits
//! `applyEdit` records on a run a page's content read handed out — retype,
//! move, remove, restyle (bold / italic / underline / strikethrough, one run
//! or several as one step) — and the two reads an editor over a run needs:
//! its look now and its font's embedded program. `pdf-ffi`'s
//! `FfiEditCommand::{ReplaceTextRunContent, MoveTextRun, RemoveTextRun,
//! RestyleTextRun}`, `restyle_text_runs` and `text_run_look`.
//!
//! The rules live in [`crate::model`]'s text-run module, over the core; this
//! is the JavaScript boundary. Run ids cross as numbers (below 2^53).

use pdf_document::TextRunLook;
use wasm_bindgen::prelude::*;

use super::content::item_id;
use super::edit::Edit;
use super::{js_error, EditCommand, Engine};
use crate::dto::{FontProgramEntry, TextRunLookEntry};
use crate::model::TextRunEdit;

#[wasm_bindgen]
impl EditCommand {
    /// Shows `text` in place of run `run_id`'s text on page `page`, in its
    /// own font, size and place (a composite-font run in the inserted
    /// standard font, as the native shells do). A second retype of the same
    /// run amends its step; the text it already shows records nothing.
    #[wasm_bindgen(js_name = retypeTextRun)]
    pub fn retype_text_run(page: u32, run_id: f64, text: String) -> Result<EditCommand, JsValue> {
        let run = item_id(run_id).map_err(js_error)?;
        Ok(EditCommand(Edit::TextRun(TextRunEdit::Retype {
            page,
            run,
            text,
        })))
    }

    /// Shifts run `run_id` on page `page` by `dx, dy` PDF points (y up). A
    /// second move of the same run amends its step; a zero move records
    /// nothing.
    #[wasm_bindgen(js_name = moveTextRun)]
    pub fn move_text_run(page: u32, run_id: f64, dx: f64, dy: f64) -> Result<EditCommand, JsValue> {
        let run = item_id(run_id).map_err(js_error)?;
        Ok(EditCommand(Edit::TextRun(TextRunEdit::Move {
            page,
            run,
            dx,
            dy,
        })))
    }

    /// Takes run `run_id` off page `page`.
    #[wasm_bindgen(js_name = removeTextRun)]
    pub fn remove_text_run(page: u32, run_id: f64) -> Result<EditCommand, JsValue> {
        let run = item_id(run_id).map_err(js_error)?;
        Ok(EditCommand(Edit::TextRun(TextRunEdit::Remove {
            page,
            run,
        })))
    }

    /// Gives every run of `run_ids` on page `page` the look `bold`,
    /// `italic`, `underline`, `strike`, as one undo step; a run that already
    /// looks that way is left alone, and none to change records nothing.
    #[wasm_bindgen(js_name = restyleTextRuns)]
    pub fn restyle_text_runs(
        page: u32,
        run_ids: Vec<f64>,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
    ) -> Result<EditCommand, JsValue> {
        let runs = run_ids
            .into_iter()
            .map(item_id)
            .collect::<Result<Vec<_>, _>>()
            .map_err(js_error)?;
        Ok(EditCommand(Edit::TextRun(TextRunEdit::Restyle {
            page,
            runs,
            look: TextRunLook {
                bold,
                italic,
                underline,
                strike,
            },
        })))
    }
}

#[wasm_bindgen]
impl Engine {
    /// How run `run_id` on page `page_index` looks now — a queued restyle
    /// included — and which styles can come off it, or `undefined` when the
    /// file cannot say. `UnsupportedOperation` for an id the page does not
    /// hold now. Same gate as `readPageContent`.
    #[wasm_bindgen(js_name = textRunLook)]
    pub fn text_run_look(
        &self,
        doc: u32,
        page_index: u32,
        run_id: f64,
    ) -> Result<Option<TextRunLookEntry>, JsValue> {
        let run_id = item_id(run_id).map_err(js_error)?;
        self.document(doc)
            .and_then(|open| open.model.text_run_look(page_index, run_id))
            .map(|look| look.map(Into::into))
            .map_err(js_error)
    }

    /// The font program run `run_id`'s font embeds, or `undefined` for one
    /// that embeds none (a standard-14 font) or a run only a pending edit
    /// holds. Same gate as `readPageContent`.
    #[wasm_bindgen(js_name = textRunFontProgram)]
    pub fn text_run_font_program(
        &self,
        doc: u32,
        page_index: u32,
        run_id: f64,
    ) -> Result<Option<FontProgramEntry>, JsValue> {
        let run_id = item_id(run_id).map_err(js_error)?;
        self.document(doc)
            .and_then(|open| open.model.text_run_font_program(page_index, run_id))
            .map(|program| program.map(Into::into))
            .map_err(js_error)
    }
}
