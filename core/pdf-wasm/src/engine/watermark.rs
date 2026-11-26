//! The `Engine`'s watermark surface (B26 T-256): the looks a dialog starts
//! from, the gate it asks before it offers to stamp, and the two stamps
//! `applyEdit` records — `pdf-ffi`'s `default_watermark`,
//! `default_image_watermark`, `FfiEditCommand::AddWatermark` and
//! `AddImageWatermark`.
//!
//! The rules live in [`crate::model`]'s watermark module, over the core;
//! this is the JavaScript boundary. A stamp is page content: one undo step,
//! and it makes `previewStale` true.

use wasm_bindgen::prelude::*;

use super::edit::Edit;
use super::{js_error, EditCommand, Engine};
use crate::dto::WatermarkLook;
use crate::model::{WatermarkRequest, WatermarkStamp};

/// The looks the core starts every shell's first stamp from: a large grey
/// diagonal over the content for text, an upright half-page picture behind
/// it.
#[wasm_bindgen(js_name = watermarkDefaults)]
pub fn watermark_defaults() -> WatermarkLook {
    crate::model::watermark_defaults().into()
}

#[wasm_bindgen]
impl EditCommand {
    /// Stamps `text` (Helvetica, the core's grey) on the 0-based `pages`, as
    /// one undo step. On apply: `EditNotPermitted` or `UnsupportedOperation`
    /// when the document refuses a content edit (asked first, see
    /// `watermarkRefusal`), `PageIndexOutOfBounds`, `EncodingGap` for a
    /// character Helvetica cannot show, `UnsupportedOperation` for blank
    /// text, a repeated page or a look outside the core's ranges.
    #[wasm_bindgen(js_name = addTextWatermark)]
    pub fn add_text_watermark(
        pages: Vec<u32>,
        text: String,
        font_size: f64,
        opacity: f64,
        rotation_degrees: f64,
        behind: bool,
    ) -> EditCommand {
        EditCommand(Edit::Watermark(WatermarkRequest {
            pages,
            stamp: WatermarkStamp::Text { text, font_size },
            opacity,
            rotation_degrees,
            behind,
        }))
    }

    /// Stamps the PNG or JPEG `image` on the 0-based `pages`, fitted inside
    /// each visible box shrunk by `scale`, as one undo step and one image
    /// object in the file. Refused as `addTextWatermark` is, with
    /// `InvalidImage` for bytes that are not a picture.
    #[wasm_bindgen(js_name = addPictureWatermark)]
    pub fn add_picture_watermark(
        pages: Vec<u32>,
        image: Vec<u8>,
        scale: f64,
        opacity: f64,
        rotation_degrees: f64,
        behind: bool,
    ) -> EditCommand {
        EditCommand(Edit::Watermark(WatermarkRequest {
            pages,
            stamp: WatermarkStamp::Picture {
                bytes: image,
                scale,
            },
            opacity,
            rotation_degrees,
            behind,
        }))
    }
}

#[wasm_bindgen]
impl Engine {
    /// Why no watermark can be stamped on the document, as the core's
    /// sentence, or `undefined` when one can: its permissions withhold
    /// modification, or its encryption cannot survive the rewrite a stamp
    /// forces.
    #[wasm_bindgen(js_name = watermarkRefusal)]
    pub fn watermark_refusal(&self, doc: u32) -> Result<Option<String>, JsValue> {
        self.document(doc)
            .map(|open| open.model.watermark_refusal())
            .map_err(js_error)
    }
}
