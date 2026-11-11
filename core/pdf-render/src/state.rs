//! pdfium state: the live `Pdfium` binding plus open documents and (native
//! only) the bitmap registry. Only ever touched from one thread: the pdfium
//! actor's worker on native (see `actor.rs` and `design.md` "Threading —
//! pdfium single-actor model"), the module's only thread on wasm32.

use std::collections::HashMap;

use pdfium_render::prelude::Pdfium;

#[cfg(not(target_arch = "wasm32"))]
use crate::bitmap::BitmapRegistry;

/// Opaque handle to a document opened via the pdfium actor. Indexes into
/// [`PdfiumState::documents`]; only meaningful to the actor that issued it.
///
/// Unlike [`crate::bitmap::BitmapHandle`], this does not release-on-drop —
/// document lifecycle management (explicit close, or automatic release tied
/// to an FFI-facing handle) is deferred to `pdf-ffi` (B7), which owns the
/// public `DocumentHandle` interface object per `design.md`'s FFI design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocHandle(pub(crate) u64);

/// State owned exclusively by the pdfium actor's worker thread.
pub struct PdfiumState {
    pub(crate) pdfium: &'static Pdfium,
    pub(crate) documents: HashMap<u64, pdfium_render::prelude::PdfDocument<'static>>,
    next_doc_id: u64,
    /// Release-on-drop registry behind the native actor's `BitmapHandle`s.
    /// wasm32 hands rendered bitmaps back by value instead.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) bitmaps: BitmapRegistry,
}

impl PdfiumState {
    pub fn new(pdfium: &'static Pdfium) -> Self {
        PdfiumState {
            pdfium,
            documents: HashMap::new(),
            next_doc_id: 0,
            #[cfg(not(target_arch = "wasm32"))]
            bitmaps: BitmapRegistry::new(),
        }
    }

    pub(crate) fn insert_document(
        &mut self,
        document: pdfium_render::prelude::PdfDocument<'static>,
    ) -> DocHandle {
        let id = self.next_doc_id;
        self.next_doc_id += 1;
        self.documents.insert(id, document);
        DocHandle(id)
    }

    /// Explicitly closes and drops a document, freeing pdfium-side resources.
    pub(crate) fn close_document(&mut self, handle: DocHandle) -> bool {
        self.documents.remove(&handle.0).is_some()
    }
}
