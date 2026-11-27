//! The `Engine`'s document-properties surface (B26 T-255): the `/Info` text
//! keys as the next save would write them, the gate a dialog asks before it
//! offers to edit them, and the change `applyEdit` records — `pdf-ffi`'s
//! `read_document_info` and `FfiEditCommand::SetDocumentInfo`, plus the
//! modify-contents gate the native shells ask first.
//!
//! The rules live in [`crate::model`]'s metadata module, over the core; this
//! is the JavaScript boundary. A change is an ordinary edit: one undo step in
//! the shared history, written by the next save.

use wasm_bindgen::prelude::*;

use super::edit::Edit;
use super::{js_error, EditCommand, Engine};
use crate::dto::DocumentPropertiesEntry;
use crate::model::DocumentProperties;

#[wasm_bindgen]
impl EditCommand {
    /// Sets the six text keys of `/Info` as one undo step. An absent or empty
    /// value removes its key; the creation and modification dates stay as
    /// they are. The values the document already has record nothing.
    /// `EditNotPermitted` on apply when the document withholds modification
    /// (see `metadataRefusal`).
    #[wasm_bindgen(js_name = setDocumentProperties)]
    pub fn set_document_properties(
        title: Option<String>,
        author: Option<String>,
        subject: Option<String>,
        keywords: Option<String>,
        creator: Option<String>,
        producer: Option<String>,
    ) -> EditCommand {
        EditCommand(Edit::Metadata(DocumentProperties {
            title,
            author,
            subject,
            keywords,
            creator,
            producer,
        }))
    }
}

#[wasm_bindgen]
impl Engine {
    /// The six text keys of `/Info` as the next save would write them (the
    /// last pending change's, else the file's). Read-only: answers however
    /// restricted the document is.
    #[wasm_bindgen(js_name = documentProperties)]
    pub fn document_properties(&self, doc: u32) -> Result<DocumentPropertiesEntry, JsValue> {
        self.document(doc)
            .map(|open| open.model.document_properties().into())
            .map_err(js_error)
    }

    /// Why the document's properties cannot be changed, as the core's
    /// clause, or `undefined` when they can — a document whose permissions
    /// withhold modification.
    #[wasm_bindgen(js_name = metadataRefusal)]
    pub fn metadata_refusal(&self, doc: u32) -> Result<Option<String>, JsValue> {
        self.document(doc)
            .map(|open| open.model.metadata_refusal().map(str::to_string))
            .map_err(js_error)
    }
}
