//! The `Engine`'s extract and split surface (B26 T-254): `pdf-ffi`'s
//! `extract_pages_to_pdf`, `extract_source_is_signed` and `plan_split`, plus
//! the up-front gate the Windows and Android shells ask before they offer
//! either (`pdf_save::extraction_refusal`).
//!
//! The rules live in [`crate::model`]'s extract module, over the core; this
//! is the JavaScript boundary. Extracting records no edit: the history and
//! the preview are untouched. A split is one plan and one extraction per
//! part, as on the native shells.

use wasm_bindgen::prelude::*;

use super::{js_error, Engine};
use crate::dto::SplitPart;
use crate::model::plan_split as plan;

/// The parts a split at `cuts` (one-based "split after page" positions,
/// `"3,7"`) makes of a document of `totalPages` named `documentName`, with
/// the file each is written under (`"report-part1.pdf"`). Refused with
/// `InvalidPageSelection`, whose message is the core's sentence for the
/// person who typed it.
#[wasm_bindgen(js_name = planSplit)]
pub fn plan_split(
    cuts: String,
    total_pages: u32,
    document_name: String,
) -> Result<Vec<SplitPart>, JsValue> {
    plan(&cuts, total_pages, &document_name)
        .map(|parts| parts.into_iter().map(SplitPart::from).collect())
        .map_err(js_error)
}

#[wasm_bindgen]
impl Engine {
    /// Why no page of the document can be extracted or split off, as the
    /// core's sentence, or `undefined` when they can: its permissions
    /// withhold extraction, or its encryption cannot survive the rewrite.
    #[wasm_bindgen(js_name = extractionRefusal)]
    pub fn extraction_refusal(&self, doc: u32) -> Result<Option<String>, JsValue> {
        self.document(doc)
            .map(|open| open.model.extraction_refusal())
            .map_err(js_error)
    }

    /// Whether the file as opened is signed: an extracted copy's signature
    /// no longer verifies, which a summary says.
    #[wasm_bindgen(js_name = extractSourceIsSigned)]
    pub fn extract_source_is_signed(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.extract_source_is_signed())
            .map_err(js_error)
    }

    /// A new PDF of `pages` (zero-based, ascending, unique; the document's
    /// current order), every pending edit applied, protected as the source
    /// is. Refused with `InvalidPageSelection`, `PageIndexOutOfBounds`,
    /// `UnsupportedOperation` (see `extractionRefusal`) and `SaveFailed`.
    #[wasm_bindgen(js_name = extractPagesToBytes)]
    pub fn extract_pages_to_bytes(&self, doc: u32, pages: Vec<u32>) -> Result<Vec<u8>, JsValue> {
        self.document(doc)
            .and_then(|open| open.model.extracted_pages(&pages))
            .map_err(js_error)
    }
}
