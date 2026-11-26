//! The `Engine`'s page-content surface (B26 T-258): the gate a shell asks
//! before it offers content editing, what a page paints (runs, images,
//! paths) as the reader sees it, its text blocks and paragraphs, a run's
//! style, the page's fonts, and the base generation every content read is
//! valid against — `pdf-ffi`'s `content_editing_allowed`,
//! `read_page_content`, `read_page_paths`, `read_page_text_blocks`,
//! `read_page_paragraph_blocks`, `page_font_families` and
//! `content_base_generation`.
//!
//! The rules live in [`crate::model`]'s content module, over the core;
//! this is the JavaScript boundary. Read-only: the edits come later (T-259+).

use wasm_bindgen::prelude::*;

use super::{js_error, Engine};
use crate::dto::{PageContentEntry, TextBlockEntry, TextRunStyleEntry};
use crate::error::{EngineError, ErrorKind};

#[wasm_bindgen]
impl Engine {
    /// Whether the document lets its page content be rewritten: the modify
    /// permission, and whether the full rewrite a content edit forces could
    /// keep its encryption (`pdf_save::content_editing_allowed`).
    #[wasm_bindgen(js_name = contentEditingAllowed)]
    pub fn content_editing_allowed(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.content_editing_allowed())
            .map_err(js_error)
    }

    /// Why an editor of this document's content cannot be offered, as the
    /// core's lower-case sentence, or `undefined` when it can: the content
    /// edit's own gate first, then the text permission every content read
    /// is behind.
    #[wasm_bindgen(js_name = contentEditingRefusal)]
    pub fn content_editing_refusal(&self, doc: u32) -> Result<Option<String>, JsValue> {
        self.document(doc)
            .map(|open| open.model.content_editing_refusal())
            .map_err(js_error)
    }

    /// What page `page_index` paints — its text runs, images and vector
    /// paths — every pending content edit laid on top. Parsed on every call:
    /// a shell caches by page id, `paintRevision` and
    /// `contentBaseGeneration`. Refused with `TextExtractionDenied`.
    #[wasm_bindgen(js_name = readPageContent)]
    pub fn read_page_content(
        &self,
        doc: u32,
        page_index: u32,
    ) -> Result<PageContentEntry, JsValue> {
        self.document(doc)
            .and_then(|open| open.model.read_page_content(page_index))
            .map(Into::into)
            .map_err(js_error)
    }

    /// The text blocks on page `page_index`, one line each. Same gate as
    /// `readPageContent`.
    #[wasm_bindgen(js_name = readTextBlocks)]
    pub fn read_text_blocks(
        &self,
        doc: u32,
        page_index: u32,
    ) -> Result<Vec<TextBlockEntry>, JsValue> {
        self.document(doc)
            .and_then(|open| open.model.read_text_blocks(page_index))
            .map(|blocks| blocks.into_iter().map(Into::into).collect())
            .map_err(js_error)
    }

    /// The text blocks on page `page_index`, stacked lines joined into
    /// paragraphs. Same gate as `readPageContent`.
    #[wasm_bindgen(js_name = readParagraphBlocks)]
    pub fn read_paragraph_blocks(
        &self,
        doc: u32,
        page_index: u32,
    ) -> Result<Vec<TextBlockEntry>, JsValue> {
        self.document(doc)
            .and_then(|open| open.model.read_paragraph_blocks(page_index))
            .map(|blocks| blocks.into_iter().map(Into::into).collect())
            .map_err(js_error)
    }

    /// How run `run_id` on page `page_index` looks, or `undefined` when the
    /// file cannot say (a run only a pending insertion added).
    /// `UnsupportedOperation` for an id the page does not hold now. Same
    /// gate as `readPageContent`.
    #[wasm_bindgen(js_name = textRunStyle)]
    pub fn text_run_style(
        &self,
        doc: u32,
        page_index: u32,
        run_id: f64,
    ) -> Result<Option<TextRunStyleEntry>, JsValue> {
        let run_id = item_id(run_id).map_err(js_error)?;
        self.document(doc)
            .and_then(|open| open.model.text_run_style(page_index, run_id))
            .map(|style| style.map(Into::into))
            .map_err(js_error)
    }

    /// The `/BaseFont` of each font page `page_index` declares, as
    /// `[resource, baseFont, resource, baseFont, …]` in resource order.
    /// Same gate as `readPageContent`.
    #[wasm_bindgen(js_name = pageFontFamilies)]
    pub fn page_font_families(&self, doc: u32, page_index: u32) -> Result<Vec<String>, JsValue> {
        self.document(doc)
            .and_then(|open| open.model.page_font_families(page_index))
            .map(|families| {
                families
                    .into_iter()
                    .flat_map(|(resource, base_font)| [resource, base_font])
                    .collect()
            })
            .map_err(js_error)
    }

    /// Changes whenever the core writes the pending content edits into a new
    /// in-memory base, or undo or redo steps across one: every content item
    /// read before is stale. A number: it counts checkpoints.
    #[wasm_bindgen(js_name = contentBaseGeneration)]
    pub fn content_base_generation(&self, doc: u32) -> Result<f64, JsValue> {
        self.document(doc)
            .map(|open| open.model.content_base_generation() as f64)
            .map_err(js_error)
    }
}

/// A content item id from JavaScript: a whole number the core could have
/// handed out.
fn item_id(id: f64) -> Result<u64, EngineError> {
    if id.is_finite() && id >= 0.0 && id.fract() == 0.0 && id < 9_007_199_254_740_992.0 {
        Ok(id as u64)
    } else {
        Err(EngineError::new(
            ErrorKind::UnsupportedOperation,
            "a content item id is a whole number",
        ))
    }
}
