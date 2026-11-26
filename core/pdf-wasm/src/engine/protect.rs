//! The `Engine`'s protect surface (B26 T-251): the two questions a dialog
//! asks before it offers to run, and the protected copy — `pdf-ffi`'s
//! `protection_will_invalidate_signatures` and `protect_to_bytes`, plus the
//! modify-contents gate the Windows and Android shells ask first.
//!
//! The rules live in [`crate::model`]'s protect module, over the core; this
//! is the JavaScript boundary. Protecting records no edit: the history and
//! the preview are untouched, and the passwords are never kept.

use wasm_bindgen::prelude::*;

use super::{js_error, Engine};

#[wasm_bindgen]
impl Engine {
    /// Why the document cannot be protected, as the core's clause, or
    /// `undefined` when it can — today only a document whose permissions
    /// withhold modification.
    #[wasm_bindgen(js_name = protectionRefusal)]
    pub fn protection_refusal(&self, doc: u32) -> Result<Option<String>, JsValue> {
        self.document(doc)
            .map(|open| open.model.protection_refusal().map(str::to_string))
            .map_err(js_error)
    }

    /// Whether the protected copy breaks a signature the file carries. When
    /// `true`, `protectToBytes` refuses with `SignaturesWouldBeInvalidated`
    /// unless `invalidateSignatures` says the user agreed.
    #[wasm_bindgen(js_name = protectionInvalidatesSignatures)]
    pub fn protection_invalidates_signatures(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .and_then(|open| open.model.protection_will_invalidate_signatures())
            .map_err(js_error)
    }

    /// The document with every edit applied, under new AES-128 protection:
    /// `openPassword` to open it, `permissionsPassword` to lift its
    /// permissions (every permission is granted). Refused with
    /// `InvalidPasswords` (one missing, or both the same: the core's
    /// sentence), `EditNotPermitted` (see `protectionRefusal`),
    /// `SignaturesWouldBeInvalidated` and `SaveFailed`.
    #[wasm_bindgen(js_name = protectToBytes)]
    pub fn protect_to_bytes(
        &self,
        doc: u32,
        open_password: String,
        permissions_password: String,
        invalidate_signatures: bool,
    ) -> Result<Vec<u8>, JsValue> {
        self.document(doc)
            .and_then(|open| {
                open.model.protected_bytes(
                    &open_password,
                    &permissions_password,
                    invalidate_signatures,
                )
            })
            .map_err(js_error)
    }
}
