//! The `Engine`'s image editing surface (B26 T-260): the five edits
//! `applyEdit` records on the images of a page — move, resize, remove,
//! replace an image a page's content read handed out, and insert a new one —
//! and the question asked before a shell opens its file picker for a
//! replacement. `pdf-ffi`'s `FfiEditCommand::{MoveImage, ResizeImage,
//! RemoveImage, ReplaceImageSource, InsertImage}`, `check_image_replacement`
//! and `stamp_placement`.
//!
//! The rules live in [`crate::model`]'s image module, over the core; this is
//! the JavaScript boundary. Image ids cross as numbers (below 2^53);
//! geometry is in PDF points (y up).

use pdf_document::Rect;
use wasm_bindgen::prelude::*;

use super::content::item_id;
use super::edit::Edit;
use super::{js_error, EditCommand, Engine};
use crate::model::ImageEdit;

#[wasm_bindgen]
impl EditCommand {
    /// Shifts image `image_id` on page `page` by `dx, dy` PDF points (y
    /// up), its size kept. A second move of the same image amends its step;
    /// a zero move records nothing.
    #[wasm_bindgen(js_name = moveImage)]
    pub fn move_image(page: u32, image_id: f64, dx: f64, dy: f64) -> Result<EditCommand, JsValue> {
        let image = item_id(image_id).map_err(js_error)?;
        Ok(EditCommand(Edit::Image(ImageEdit::Move {
            page,
            image,
            dx,
            dy,
        })))
    }

    /// Paints image `image_id` on page `page` into the box `x, y, width,
    /// height` (PDF points, bottom-left origin). The box it has records
    /// nothing; one with no area is refused.
    #[wasm_bindgen(js_name = resizeImage)]
    pub fn resize_image(
        page: u32,
        image_id: f64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<EditCommand, JsValue> {
        let image = item_id(image_id).map_err(js_error)?;
        Ok(EditCommand(Edit::Image(ImageEdit::Resize {
            page,
            image,
            to: Rect {
                x,
                y,
                width,
                height,
            },
        })))
    }

    /// Takes image `image_id` off page `page`.
    #[wasm_bindgen(js_name = removeImage)]
    pub fn remove_image(page: u32, image_id: f64) -> Result<EditCommand, JsValue> {
        let image = item_id(image_id).map_err(js_error)?;
        Ok(EditCommand(Edit::Image(ImageEdit::Remove { page, image })))
    }

    /// Shows `bytes` (an encoded PNG or JPEG) in place of image `image_id`'s
    /// picture on page `page`, in its place and size. Ask
    /// `checkImageReplacement` before a file is chosen.
    #[wasm_bindgen(js_name = replaceImage)]
    pub fn replace_image(page: u32, image_id: f64, bytes: Vec<u8>) -> Result<EditCommand, JsValue> {
        let image = item_id(image_id).map_err(js_error)?;
        Ok(EditCommand(Edit::Image(ImageEdit::Replace {
            page,
            image,
            bytes,
        })))
    }

    /// Paints `bytes` (an encoded PNG or JPEG) on page `page` as a new
    /// image with its top-left corner at `x, y` (PDF points), at the size
    /// the core's placement gives it: its own proportions, the longest side
    /// two inches.
    #[wasm_bindgen(js_name = insertImage)]
    pub fn insert_image(page: u32, x: f64, y: f64, bytes: Vec<u8>) -> EditCommand {
        EditCommand(Edit::Image(ImageEdit::Insert {
            page,
            at: (x, y),
            bytes,
        }))
    }
}

#[wasm_bindgen]
impl Engine {
    /// Whether image `image_id` on page `page_index` could be replaced now,
    /// asked before the file picker opens: `EditNotPermitted` for a
    /// document that forbids content changes, `UnsupportedOperation` for an
    /// image with an unsaved edit in a session that may not checkpoint, one
    /// the page does not paint now, or a picture undo could not put back.
    #[wasm_bindgen(js_name = checkImageReplacement)]
    pub fn check_image_replacement(
        &self,
        doc: u32,
        page_index: u32,
        image_id: f64,
    ) -> Result<(), JsValue> {
        let image_id = item_id(image_id).map_err(js_error)?;
        self.document(doc)
            .and_then(|open| open.model.check_image_replacement(page_index, image_id))
            .map_err(js_error)
    }
}
