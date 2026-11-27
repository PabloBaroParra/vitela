//! The `Engine`'s vector-path editing surface (B26 T-263): the three edits
//! `applyEdit` records on the paths a page paints — move, resize and remove
//! a rule, a border or a shaded box a page's content read handed out.
//! `pdf-ffi`'s `FfiEditCommand::{MovePath, ResizePath, RemovePath}`.
//!
//! The rules live in [`crate::model`]'s path module, over the core; this is
//! the JavaScript boundary. Path ids cross as numbers (below 2^53); geometry
//! is in PDF points (y up).

use pdf_document::Rect;
use wasm_bindgen::prelude::*;

use super::content::item_id;
use super::edit::Edit;
use super::{js_error, EditCommand};
use crate::model::PathEdit;

#[wasm_bindgen]
impl EditCommand {
    /// Shifts path `path_id` on page `page` by `dx, dy` PDF points (y up),
    /// its shape kept. A second move of the same path amends its step; a
    /// zero move records nothing.
    #[wasm_bindgen(js_name = movePath)]
    pub fn move_path(page: u32, path_id: f64, dx: f64, dy: f64) -> Result<EditCommand, JsValue> {
        let path = item_id(path_id).map_err(js_error)?;
        Ok(EditCommand(Edit::Path(PathEdit::Move {
            page,
            path,
            dx,
            dy,
        })))
    }

    /// Reshapes path `path_id` on page `page` so its box becomes `x, y,
    /// width, height` (PDF points, bottom-left origin), its line width kept.
    /// A side the path is flat along stays flat; one it has may not go to
    /// nothing. The box it has records nothing.
    #[wasm_bindgen(js_name = resizePath)]
    pub fn resize_path(
        page: u32,
        path_id: f64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<EditCommand, JsValue> {
        let path = item_id(path_id).map_err(js_error)?;
        Ok(EditCommand(Edit::Path(PathEdit::Resize {
            page,
            path,
            to: Rect {
                x,
                y,
                width,
                height,
            },
        })))
    }

    /// Takes path `path_id` off page `page`.
    #[wasm_bindgen(js_name = removePath)]
    pub fn remove_path(page: u32, path_id: f64) -> Result<EditCommand, JsValue> {
        let path = item_id(path_id).map_err(js_error)?;
        Ok(EditCommand(Edit::Path(PathEdit::Remove { page, path })))
    }
}
