//! wasm32 public API: the same work as the native `PdfiumRenderer`, run
//! synchronously on the calling thread.
//!
//! `wasm32-unknown-unknown` cannot host the actor: spawning a thread fails,
//! `Condvar::wait` panics and an empty `mpsc::Receiver::recv` never returns
//! (see `spikes/web-wasm/README.md`). On the web the Web Worker that loads
//! this module *is* the actor — it already owns pdfium on one thread — so
//! scheduling (priorities, cancellation) belongs to the worker's message
//! loop, between `postMessage`s, and every call here simply runs.
//!
//! [`SyncRenderer`] is an owned value rather than a `thread_local` registry
//! so the caller (a `#[wasm_bindgen]` struct in `pdf-wasm`) holds the open
//! documents directly: no `RefCell` borrow can be re-entered from JS and no
//! hidden global outlives the handle. Only the `Pdfium` binding itself is
//! process-wide, because pdfium-render allows exactly one per process.

use std::cell::OnceCell;

use pdfium_render::prelude::Pdfium;

use crate::bitmap::Bitmap;
use crate::error::RenderError;
use crate::jobs::{
    open_from_bytes_job, page_count_job, page_geometry_job, page_size_job, render_page_bitmap,
    render_page_tiles_bitmaps, search_job, text_runs_job,
};
use crate::options::{Rect, RenderOptions, Tile};
use crate::selection::PageGeometry;
use crate::state::{DocHandle, PdfiumState};
use crate::text::{TextMatch, TextRun};

thread_local! {
    // A wasm32-unknown-unknown module runs on one thread, so a thread_local
    // is the whole process. Leaked like the native binding: pdfium-render
    // promotes the first bindings into a global and refuses a second bind.
    static PDFIUM: OnceCell<&'static Pdfium> = const { OnceCell::new() };
}

/// Binds the Emscripten pdfium module on first use and returns the one
/// process-wide `Pdfium`. A failed bind is not cached, so a caller that
/// forgot `initialize_pdfium_render` can initialize and retry.
fn pdfium() -> Result<&'static Pdfium, RenderError> {
    PDFIUM.with(|cell| {
        if let Some(pdfium) = cell.get() {
            return Ok(*pdfium);
        }
        let bindings = Pdfium::bind_to_system_library()
            .map_err(|error| RenderError::LibraryUnavailable(error.to_string()))?;
        let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
        Ok(*cell.get_or_init(|| pdfium))
    })
}

/// Synchronous pdfium front end for wasm32. Requires JavaScript to have
/// called pdfium-render's `initialize_pdfium_render(pdfiumModule,
/// rustModule, debug)` before [`SyncRenderer::new`].
pub struct SyncRenderer {
    state: PdfiumState,
}

impl SyncRenderer {
    /// Binds pdfium (once per process) and starts with no open documents.
    pub fn new() -> Result<Self, RenderError> {
        Ok(SyncRenderer {
            state: PdfiumState::new(pdfium()?),
        })
    }

    /// Opens a document from bytes, optionally decrypting it with
    /// `password`. pdfium copies the bytes into its own heap, so a large PDF
    /// is held twice while open.
    pub fn open_document_from_bytes(
        &mut self,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> Result<DocHandle, RenderError> {
        open_from_bytes_job(&mut self.state, bytes, password)
    }

    /// Closes a document. `false` if the handle was unknown or already
    /// closed.
    pub fn close_document(&mut self, doc: DocHandle) -> bool {
        self.state.close_document(doc)
    }

    pub fn page_count(&mut self, doc: DocHandle) -> Result<u32, RenderError> {
        page_count_job(&mut self.state, doc)
    }

    /// One page's size in PDF points, already turned by its `/Rotate`.
    pub fn page_size(
        &mut self,
        doc: DocHandle,
        page_index: u32,
    ) -> Result<(f32, f32), RenderError> {
        page_size_job(&mut self.state, doc, page_index)
    }

    /// Every page's drawn size and turn, in one pass.
    pub fn page_geometry(&mut self, doc: DocHandle) -> Result<Vec<PageGeometry>, RenderError> {
        page_geometry_job(&mut self.state, doc)
    }

    /// Renders a page (or a region of it, in PDF points) at `dpi` into an
    /// owned RGBA bitmap (`stride == width * 4`).
    pub fn render_page(
        &mut self,
        doc: DocHandle,
        page_index: u32,
        dpi: u32,
        region: Option<Rect>,
        options: RenderOptions,
    ) -> Result<Bitmap, RenderError> {
        render_page_bitmap(&self.state, doc, page_index, dpi, region, options)
    }

    /// Renders output-pixel tiles of one page, loading the page once. Fails
    /// as a unit if any tile lies outside the page at `dpi`.
    pub fn render_page_tiles(
        &mut self,
        doc: DocHandle,
        page_index: u32,
        dpi: u32,
        tiles: &[Tile],
        options: RenderOptions,
    ) -> Result<Vec<Bitmap>, RenderError> {
        render_page_tiles_bitmaps(&self.state, doc, page_index, dpi, tiles, options)
    }

    pub fn text_runs(
        &mut self,
        doc: DocHandle,
        page_index: u32,
    ) -> Result<Vec<TextRun>, RenderError> {
        text_runs_job(&mut self.state, doc, page_index)
    }

    /// Exact, case-sensitive matches of `query` across the document. Enforces
    /// no permissions, like the native `search`: policy stays at the shell
    /// boundary.
    pub fn search(&mut self, doc: DocHandle, query: &str) -> Result<Vec<TextMatch>, RenderError> {
        search_job(&mut self.state, doc, query)
    }
}
