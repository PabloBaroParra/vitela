//! `Engine`: the one object the web worker talks to.
//!
//! It owns `pdf-render`'s `SyncRenderer` and every open document. Documents
//! are addressed by a numeric id, like the native handles, rather than being
//! JS objects of their own: every read needs the renderer too, and keeping
//! both behind one `&mut self` means no `RefCell` can be re-entered from JS.
//!
//! Each open document is two views of the same bytes, as in `pdf-ffi`: the
//! lopdf model (credentials, permissions, page sizes) and the pdfium handle
//! (pixels, text). The model is opened first, so a bad password is refused
//! by the core's own rule before pdfium is asked.

use std::collections::HashMap;

use pdf_manip::OpenCredentials;
use pdf_render::{DocumentHandle, RenderOptions, SyncRenderer};
use wasm_bindgen::prelude::*;

use crate::convert::tight_rgba;
use crate::dto::{PageCharacters, PageDimensions, RenderedPage, SearchMatch, TextRun};
use crate::error::EngineError;
use crate::model::DocumentModel;

struct OpenDocument {
    model: DocumentModel,
    render: DocumentHandle,
}

/// The web shell's core engine. Construct it after pdfium is bound (see the
/// crate docs).
#[wasm_bindgen]
pub struct Engine {
    renderer: SyncRenderer,
    documents: HashMap<u32, OpenDocument>,
    next_id: u32,
}

#[wasm_bindgen]
impl Engine {
    /// Binds pdfium (once per module) and starts with no open documents.
    /// Fails with `EngineUnavailable` if `initialize_pdfium_render` has not
    /// run.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Engine, JsValue> {
        console_error_panic_hook::set_once();
        let renderer = SyncRenderer::new().map_err(|err| js_error(err.into()))?;
        Ok(Engine {
            renderer,
            documents: HashMap::new(),
            next_id: 0,
        })
    }

    /// Opens `bytes`, with no password or with one (user or owner — the
    /// document decides which it matches). Returns the document's id.
    pub fn open(&mut self, bytes: Vec<u8>, password: Option<String>) -> Result<u32, JsValue> {
        self.open_with(bytes, password.into()).map_err(js_error)
    }

    /// Opens an encrypted document after verifying both its passwords, each
    /// against its own role. An unencrypted document opens normally.
    #[wasm_bindgen(js_name = openWithPasswords)]
    pub fn open_with_passwords(
        &mut self,
        bytes: Vec<u8>,
        user_password: String,
        owner_password: String,
    ) -> Result<u32, JsValue> {
        let credentials = OpenCredentials::UserAndOwner {
            user: user_password,
            owner: owner_password,
        };
        self.open_with(bytes, credentials).map_err(js_error)
    }

    /// Closes a document. `false` if the id was unknown or already closed.
    pub fn close(&mut self, doc: u32) -> bool {
        match self.documents.remove(&doc) {
            Some(open) => self.renderer.close_document(open.render),
            None => false,
        }
    }

    #[wasm_bindgen(js_name = pageCount)]
    pub fn page_count(&self, doc: u32) -> Result<u32, JsValue> {
        self.document(doc)
            .map(|open| open.model.page_count())
            .map_err(js_error)
    }

    /// Every page's layout size and turn, in page order.
    #[wasm_bindgen(js_name = pageDimensions)]
    pub fn page_dimensions(&self, doc: u32) -> Result<Vec<PageDimensions>, JsValue> {
        let open = self.document(doc).map_err(js_error)?;
        Ok(open
            .model
            .page_sizes()
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Whether the document lets its text be read — what the UI checks to
    /// offer search and selection at all.
    #[wasm_bindgen(js_name = textExtractionAllowed)]
    pub fn text_extraction_allowed(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.text_extraction_allowed())
            .map_err(js_error)
    }

    /// Renders a whole page at `dpi` (72 = one pixel per point), optionally
    /// with its content colours inverted for dark mode.
    #[wasm_bindgen(js_name = renderPage)]
    pub fn render_page(
        &mut self,
        doc: u32,
        page_index: u32,
        dpi: u32,
        invert_colors: bool,
    ) -> Result<RenderedPage, JsValue> {
        let render = self.page_handle(doc, page_index).map_err(js_error)?;
        let options = RenderOptions::new().with_invert_content_colors(invert_colors);
        let bitmap = self
            .renderer
            .render_page(render, page_index, dpi, None, options)
            .map_err(|err| js_error(err.into()))?;
        let (width, height) = (bitmap.width, bitmap.height);
        Ok(RenderedPage::new(width, height, tight_rgba(bitmap)))
    }

    /// One page's text runs. Refused with `TextExtractionDenied` when the
    /// document withholds text extraction.
    #[wasm_bindgen(js_name = textRuns)]
    pub fn text_runs(&mut self, doc: u32, page_index: u32) -> Result<Vec<TextRun>, JsValue> {
        let runs = self.read_runs(doc, page_index).map_err(js_error)?;
        Ok(runs.into_iter().map(Into::into).collect())
    }

    /// One page's characters for caret hit-testing and selection. Same gate
    /// as `textRuns`.
    #[wasm_bindgen(js_name = pageCharacters)]
    pub fn page_characters(
        &mut self,
        doc: u32,
        page_index: u32,
    ) -> Result<PageCharacters, JsValue> {
        let runs = self.read_runs(doc, page_index).map_err(js_error)?;
        Ok(PageCharacters::new(pdf_render::PageCharacters::from_runs(
            &runs,
        )))
    }

    /// Exact, case-sensitive matches of `query` across the document, in page
    /// order. An empty query matches nothing. Same gate as `textRuns`.
    pub fn search(&mut self, doc: u32, query: String) -> Result<Vec<SearchMatch>, JsValue> {
        let render = self.text_handle(doc).map_err(js_error)?;
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let found = self
            .renderer
            .search(render, &query)
            .map_err(|err| js_error(err.into()))?;
        Ok(found.into_iter().map(Into::into).collect())
    }
}

impl Engine {
    fn open_with(
        &mut self,
        bytes: Vec<u8>,
        credentials: OpenCredentials,
    ) -> Result<u32, EngineError> {
        let model = DocumentModel::open(&bytes, &credentials)?;
        let render = self
            .renderer
            .open_document_from_bytes(bytes, credentials.render_password())?;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.documents.insert(id, OpenDocument { model, render });
        Ok(id)
    }

    fn document(&self, doc: u32) -> Result<&OpenDocument, EngineError> {
        self.documents
            .get(&doc)
            .ok_or_else(|| EngineError::document_not_found(doc))
    }

    fn page_handle(&self, doc: u32, page_index: u32) -> Result<DocumentHandle, EngineError> {
        let open = self.document(doc)?;
        open.model.ensure_page(page_index)?;
        Ok(open.render)
    }

    /// The render handle behind the text gate: the permission is checked
    /// first, so a restricted document is refused whatever is asked of it.
    fn text_handle(&self, doc: u32) -> Result<DocumentHandle, EngineError> {
        let open = self.document(doc)?;
        open.model.ensure_text_extraction_allowed()?;
        Ok(open.render)
    }

    fn read_runs(
        &mut self,
        doc: u32,
        page_index: u32,
    ) -> Result<Vec<pdf_render::TextRun>, EngineError> {
        let render = self.text_handle(doc)?;
        self.document(doc)?.model.ensure_page(page_index)?;
        Ok(self.renderer.text_runs(render, page_index)?)
    }
}

/// A JavaScript `Error` whose `name` is the failure's kind.
fn js_error(err: EngineError) -> JsValue {
    let error = js_sys::Error::new(&err.message);
    error.set_name(err.kind.name());
    error.into()
}
