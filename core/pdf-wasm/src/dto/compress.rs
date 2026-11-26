//! The values `Engine`'s compress surface hands to JavaScript (B26 T-250).

use wasm_bindgen::prelude::*;

use crate::model::{CompressRefusal, CompressedCopy};

/// Something a compression could not do: `kind` to branch on
/// (`encryptedDocumentNotRewritable`, `signaturesWouldBeInvalidated` or
/// `other`), `message` in the core's words.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct CompressRefusalEntry {
    kind: &'static str,
    message: String,
}

impl From<CompressRefusal> for CompressRefusalEntry {
    fn from(refusal: CompressRefusal) -> Self {
        CompressRefusalEntry {
            kind: refusal.kind,
            message: refusal.message,
        }
    }
}

#[wasm_bindgen]
impl CompressRefusalEntry {
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.to_string()
    }

    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

/// A compressed save: the bytes and the core's report on them. Sizes are
/// numbers (not `BigInt`): a wasm32 module cannot hold a document past
/// 4 GiB, so every size is exact in an `f64`.
#[wasm_bindgen]
pub struct CompressedSave {
    bytes: Vec<u8>,
    before_bytes: u64,
    after_bytes: u64,
    saved_bytes: u64,
    reduced: bool,
    refusals: Vec<CompressRefusal>,
}

impl From<CompressedCopy> for CompressedSave {
    fn from(copy: CompressedCopy) -> Self {
        CompressedSave {
            bytes: copy.bytes,
            before_bytes: copy.before_bytes,
            after_bytes: copy.after_bytes,
            saved_bytes: copy.saved_bytes,
            reduced: copy.reduced,
            refusals: copy.refusals,
        }
    }
}

#[wasm_bindgen]
impl CompressedSave {
    /// Size of this save's own output, before compression — not the opened
    /// file.
    #[wasm_bindgen(getter, js_name = beforeBytes)]
    pub fn before_bytes(&self) -> f64 {
        self.before_bytes as f64
    }

    /// Size of the bytes `takeBytes` hands over.
    #[wasm_bindgen(getter, js_name = afterBytes)]
    pub fn after_bytes(&self) -> f64 {
        self.after_bytes as f64
    }

    /// `beforeBytes - afterBytes`, or zero.
    #[wasm_bindgen(getter, js_name = savedBytes)]
    pub fn saved_bytes(&self) -> f64 {
        self.saved_bytes as f64
    }

    /// Whether the bytes are strictly smaller than the ordinary save. When
    /// `false` they ARE the ordinary save: there is no smaller copy to offer.
    #[wasm_bindgen(getter)]
    pub fn reduced(&self) -> bool {
        self.reduced
    }

    /// What could not be done, in the order the core found it.
    #[wasm_bindgen(getter)]
    pub fn refusals(&self) -> Vec<CompressRefusalEntry> {
        self.refusals.iter().cloned().map(Into::into).collect()
    }

    /// Moves the PDF out (one copy into a JS `Uint8Array`); a second call
    /// returns an empty array.
    #[wasm_bindgen(js_name = takeBytes)]
    pub fn take_bytes(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}
