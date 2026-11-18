//! The `Engine`'s import surface ("Add PDFs") and the Documents view's
//! blocks (B26 T-227).
//!
//! The rules live in [`crate::model`]'s import module, over the core; this is
//! the JavaScript boundary. As with every page edit, an import changes the
//! model at once (`pageCount`, `pageIds`, `documentBlocks`) but not the
//! pixels: call `refreshPreview` after it before rendering again.

use wasm_bindgen::prelude::*;

use super::{js_error, Engine};
use crate::dto::{DocumentBlock, ImportedPages, PreparedImport};
use crate::error::EngineError;

#[wasm_bindgen]
impl Engine {
    /// Opens `bytes` as an import source (with `password` for an encrypted
    /// one) and keeps it until `importPrepared` takes it or
    /// `discardPrepared` drops it. Changes no document.
    ///
    /// Refused with `PasswordRequired` or `WrongPassword` when the source is
    /// encrypted and `password` does not open it (a missing one may come
    /// back as either: ask, and call again), and `UnsupportedOperation` when
    /// it forbids copying its pages, has none, or carries something an
    /// import cannot graft (a signature, an XFA form, optional content).
    #[wasm_bindgen(js_name = prepareImport)]
    pub fn prepare_import(
        &mut self,
        bytes: Vec<u8>,
        password: Option<String>,
    ) -> Result<PreparedImport, JsValue> {
        self.prepared
            .prepare(&bytes, password.as_deref())
            .map(Into::into)
            .map_err(js_error)
    }

    /// Drops a prepared source (an abandoned pick). `false` when `id` holds
    /// nothing.
    #[wasm_bindgen(js_name = discardPrepared)]
    pub fn discard_prepared(&mut self, id: u32) -> bool {
        self.prepared.discard(id)
    }

    /// Adds every page of the prepared sources `ids`, in that order, at
    /// position `index` (the page count appends), as ONE undoable edit, and
    /// hands the sources over to the document.
    ///
    /// All or nothing. Refused with `EditNotPermitted` when the document
    /// withholds a permission an import needs, `UnsupportedOperation` when
    /// its encryption cannot survive the rewrite or `ids` is empty, repeats
    /// one, or names one not prepared, and `PageIndexOutOfBounds` when
    /// `index` is past the end.
    #[wasm_bindgen(js_name = importPrepared)]
    pub fn import_prepared(
        &mut self,
        doc: u32,
        ids: Vec<u32>,
        index: u32,
    ) -> Result<ImportedPages, JsValue> {
        // Borrowed field by field: the document and the prepared sources
        // are used together.
        let open = self
            .documents
            .get_mut(&doc)
            .ok_or_else(|| js_error(EngineError::document_not_found(doc)))?;
        open.model
            .import_prepared(&mut self.prepared, &ids, index)
            .map(Into::into)
            .map_err(js_error)
    }

    /// Why the document would refuse any import right now, in the reader's
    /// words, or `undefined` — asked before the file picker.
    #[wasm_bindgen(js_name = importRefusal)]
    pub fn import_refusal(&self, doc: u32) -> Result<Option<String>, JsValue> {
        self.document(doc)
            .map(|open| open.model.import_refusal())
            .map_err(js_error)
    }

    /// Every block of the document, in page order: one per contiguous run
    /// of pages from the same PDF. Changes with every edit, like
    /// `pageDimensions`.
    #[wasm_bindgen(js_name = documentBlocks)]
    pub fn document_blocks(&self, doc: u32) -> Result<Vec<DocumentBlock>, JsValue> {
        self.document(doc)
            .map(|open| open.model.blocks().into_iter().map(Into::into).collect())
            .map_err(js_error)
    }
}
