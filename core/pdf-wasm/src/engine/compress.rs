//! The `Engine`'s compress surface (B26 T-250): the presets, the two
//! questions a dialog asks before it offers to run, and the compressed save
//! — `pdf-ffi`'s `compress_presets`, `compression_refusal`,
//! `compressed_save_will_invalidate_signatures` and
//! `save_compressed_to_bytes`, with the web's one intent (keep the
//! document's encryption).
//!
//! The rules live in [`crate::model`]'s compress module, over the core; this
//! is the JavaScript boundary. Compressing records no edit: the history and
//! the preview are untouched.

use wasm_bindgen::prelude::*;

use super::{js_error, Engine};
use crate::dto::{CompressRefusalEntry, CompressedSave};
use crate::model::{compress_presets as presets, preset_from_name};

/// Every preset a compress dialog may offer (`lossless`, `balanced`,
/// `small`), strongest-preserving first: the core's own list.
#[wasm_bindgen(js_name = compressPresets)]
pub fn compress_presets() -> Vec<String> {
    presets().into_iter().map(str::to_string).collect()
}

#[wasm_bindgen]
impl Engine {
    /// Why the document cannot be compressed, or `undefined` when it can —
    /// today only an encrypted one (`encryptedDocumentNotRewritable`).
    #[wasm_bindgen(js_name = compressionRefusal)]
    pub fn compression_refusal(&self, doc: u32) -> Result<Option<CompressRefusalEntry>, JsValue> {
        self.document(doc)
            .map(|open| open.model.compression_refusal().map(Into::into))
            .map_err(js_error)
    }

    /// Whether compressing breaks a signature the file carries — `true` for
    /// a signed file even with nothing edited, unlike the ordinary save.
    /// When `true`, `saveCompressedToBytes` refuses with
    /// `SignaturesWouldBeInvalidated` unless `invalidateSignatures` says the
    /// user agreed.
    #[wasm_bindgen(js_name = compressionInvalidatesSignatures)]
    pub fn compression_invalidates_signatures(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .and_then(|open| open.model.compressed_save_will_invalidate_signatures())
            .map_err(js_error)
    }

    /// The document with every edit applied, compressed as far as `preset`
    /// allows. Never larger than `saveToBytes`; exactly it (`reduced`
    /// false) when nothing smaller came out or the compression was refused,
    /// with the reason in `refusals`. Refused with `UnsupportedOperation`
    /// for an unknown preset, `SignaturesWouldBeInvalidated` (see
    /// `compressionInvalidatesSignatures`) and `SaveFailed`.
    #[wasm_bindgen(js_name = saveCompressedToBytes)]
    pub fn save_compressed_to_bytes(
        &self,
        doc: u32,
        preset: String,
        invalidate_signatures: bool,
    ) -> Result<CompressedSave, JsValue> {
        let preset = preset_from_name(&preset).map_err(js_error)?;
        self.document(doc)
            .and_then(|open| open.model.save_compressed(preset, invalidate_signatures))
            .map(Into::into)
            .map_err(js_error)
    }
}
