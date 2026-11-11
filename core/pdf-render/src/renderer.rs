//! Native public API: opens documents, renders pages, and queries text runs,
//! all serialized through the single pdfium actor (T-015, T-016, T-018).
//!
//! Each method only schedules work; the work itself lives in `jobs`, shared
//! with the threadless wasm32 front end (`sync_renderer.rs`). This module
//! needs threads and a filesystem path to the pdfium library, so it is not
//! built for wasm32.

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use pdfium_render::prelude::Pdfium;

use crate::actor::{Actor, JobHandle};
use crate::bitmap::BitmapHandle;
use crate::error::RenderError;
use crate::jobs::{
    open_from_bytes_job, open_from_path_job, page_count_job, page_geometry_job, page_size_job,
    render_page_job, render_page_tiles_job, search_job, text_runs_job,
};
use crate::library::resolve_library_path;
use crate::options::{Priority, Rect, RenderOptions, Tile};
use crate::selection::PageGeometry;
use crate::state::PdfiumState;
use crate::text::{TextMatch, TextRun};

pub use crate::state::DocHandle as DocumentHandle;

/// The pdfium actor: `Actor<PdfiumState>`, one dedicated OS thread owning
/// every pdfium call for the process (see `actor.rs` and `design.md`).
pub type PdfiumActor = Actor<PdfiumState>;

/// A cancellable handle to an in-flight or completed page render.
pub type RenderHandle = JobHandle<BitmapHandle>;

pub(crate) fn global_actor() -> Result<&'static PdfiumActor, RenderError> {
    // Only a *successful* bind is cached: a failed `bind_to_library` never
    // constructs the singleton `Pdfium` (pdfium.rs's `BINDINGS` is set solely
    // inside `Pdfium::new`), so retrying after a transient failure — library
    // installed later, env var fixed, AV briefly locking the file — is safe
    // and keeps the process usable without a restart.
    static ACTOR: OnceLock<PdfiumActor> = OnceLock::new();
    static BIND: Mutex<()> = Mutex::new(());

    if let Some(actor) = ACTOR.get() {
        return Ok(actor);
    }
    let _guard = BIND.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(actor) = ACTOR.get() {
        return Ok(actor);
    }
    let actor = bind_actor(resolve_library_path())?;
    Ok(ACTOR.get_or_init(|| actor))
}

fn bind_actor(path: std::path::PathBuf) -> Result<PdfiumActor, RenderError> {
    let bindings = Pdfium::bind_to_library(&path).map_err(|error| RenderError::LibraryLoad {
        path,
        message: error.to_string(),
    })?;
    // Leaked deliberately: exactly one `Pdfium` instance may exist for
    // the lifetime of the process (`Pdfium::new` asserts this — see
    // pdfium.rs's `BINDINGS` global), matching this crate's single-actor
    // design. The actor, and this leaked reference, live until process
    // exit.
    let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
    Ok(Actor::spawn(PdfiumState::new(pdfium)))
}

/// Entry point for opening documents, rendering pages, and querying text
/// runs — all work is dispatched to the single global pdfium actor.
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfiumRenderer;

impl PdfiumRenderer {
    pub fn new() -> Self {
        PdfiumRenderer
    }

    /// Opens a document from a file path, optionally decrypting it with
    /// `password`. Blocks the calling thread until the actor processes the
    /// open job (opens are submitted at `Priority::Visible` — a document
    /// can't be usefully scrolled or rendered until it exists).
    pub fn open_document(
        &self,
        path: impl AsRef<Path>,
        password: Option<&str>,
    ) -> Result<DocumentHandle, RenderError> {
        let path = path.as_ref().to_path_buf();
        let password = password.map(|p| p.to_string());

        global_actor()?
            .submit(Priority::Visible, move |state: &mut PdfiumState| {
                open_from_path_job(state, &path, password.as_deref())
            })
            .wait()
    }

    /// Opens a document from an in-memory byte buffer, optionally decrypting
    /// it with `password` — the **canonical** cross-platform entrypoint
    /// (spec delta "FileAccessPort"): Android (Storage Access Framework) and
    /// iOS (security-scoped bookmarks) only ever hand a shell a byte stream,
    /// never a filesystem path pdfium can open directly. `open_document`'s
    /// path-based contract remains a convenience solely for the GTK4 shell,
    /// which bypasses the FFI boundary and can read its own files with
    /// `std::fs` directly. Ownership of `bytes` is handed to pdfium
    /// (`Pdfium::load_pdf_from_byte_vec`), which keeps it alive for the
    /// document's lifetime — no separate buffer bookkeeping needed here.
    pub fn open_document_from_bytes(
        &self,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> Result<DocumentHandle, RenderError> {
        let password = password.map(|p| p.to_string());

        global_actor()?
            .submit(Priority::Visible, move |state: &mut PdfiumState| {
                open_from_bytes_job(state, bytes, password.as_deref())
            })
            .wait()
    }

    /// Closes a previously opened document, freeing pdfium-side resources.
    /// Returns `true` if the handle was known and closed, `false` if it was
    /// already closed or never valid.
    pub fn close_document(&self, handle: DocumentHandle) -> Result<bool, RenderError> {
        global_actor()?
            .submit(Priority::Visible, move |state: &mut PdfiumState| {
                Ok(state.close_document(handle))
            })
            .wait()
    }

    /// Returns the number of pages in an open document without rasterizing.
    pub fn page_count(&self, doc: DocumentHandle, priority: Priority) -> JobHandle<u32> {
        match global_actor() {
            Ok(actor) => actor.submit(priority, move |state: &mut PdfiumState| {
                page_count_job(state, doc)
            }),
            Err(error) => JobHandle::failed(error),
        }
    }

    /// Queries every page's drawn geometry in a single actor round-trip.
    ///
    /// Shells laying out a whole document need all page sizes up front; one
    /// batched job avoids the N serialized submit/wait cycles that querying
    /// [`page_size`](Self::page_size) per page would push through the actor.
    /// The turn rides along with the size because the two only mean anything
    /// together — see [`PageGeometry`].
    pub fn page_geometry(
        &self,
        doc: DocumentHandle,
        priority: Priority,
    ) -> JobHandle<Vec<PageGeometry>> {
        match global_actor() {
            Ok(actor) => actor.submit(priority, move |state: &mut PdfiumState| {
                page_geometry_job(state, doc)
            }),
            Err(error) => JobHandle::failed(error),
        }
    }

    /// Requests a page render at the given DPI. Returns immediately with a
    /// cancellable [`RenderHandle`]; call `.wait()` to block for the result.
    ///
    /// `region`, if given, is a sub-rectangle of the page (in PDF points) to
    /// render instead of the full page — used for partial/tile rendering.
    /// `options.invert_content_colors` requests dark-mode inversion (T-017):
    /// applied as a post-render linear RGBA inversion (see `inversion.rs`
    /// module docs for why the pdfium-native color-scheme path isn't used).
    pub fn render_page(
        &self,
        doc: DocumentHandle,
        page_index: u32,
        dpi: u32,
        region: Option<Rect>,
        options: RenderOptions,
        priority: Priority,
    ) -> RenderHandle {
        match global_actor() {
            Ok(actor) => actor.submit(priority, move |state: &mut PdfiumState| {
                render_page_job(state, doc, page_index, dpi, region, options)
            }),
            Err(error) => JobHandle::failed(error),
        }
    }

    /// Renders several bounded, output-pixel-aligned tiles of one page in a
    /// single actor job, in the order given. Each `tile` uses the same
    /// top-left pixel space as a normal render at `dpi`, including pdfium's
    /// CropBox and page rotation handling, and no full-page bitmap is
    /// allocated.
    ///
    /// This exists because loading a page is not free: pdfium parses the
    /// page's content stream on `FPDF_LoadPage`, and on a text-heavy page at
    /// deep zoom that parse costs more than rasterizing one tile. Requesting
    /// tiles one at a time pays it once per tile *and* forces a full
    /// submit/wait round-trip between them; batching pays it once for the
    /// whole viewport.
    ///
    /// Every tile must lie inside the page at `dpi` — a batch fails as a unit
    /// rather than returning a partially filled viewport.
    pub fn render_page_tiles(
        &self,
        doc: DocumentHandle,
        page_index: u32,
        dpi: u32,
        tiles: Vec<Tile>,
        options: RenderOptions,
        priority: Priority,
    ) -> JobHandle<Vec<BitmapHandle>> {
        match global_actor() {
            Ok(actor) => actor.submit(priority, move |state: &mut PdfiumState| {
                render_page_tiles_job(state, doc, page_index, dpi, &tiles, options)
            }),
            Err(error) => JobHandle::failed(error),
        }
    }

    /// Queries a page's size in PDF points without rasterizing it — the
    /// cheap preflight shells need to compute a fit-to-width DPI before the
    /// one real `render_page` call.
    pub fn page_size(
        &self,
        doc: DocumentHandle,
        page_index: u32,
        priority: Priority,
    ) -> JobHandle<(f32, f32)> {
        match global_actor() {
            Ok(actor) => actor.submit(priority, move |state: &mut PdfiumState| {
                page_size_job(state, doc, page_index)
            }),
            Err(error) => JobHandle::failed(error),
        }
    }

    /// Queries per-run text position/font data for a page (T-018, spec's
    /// "Text-Run Data Exposure" future-phase enabler). MVP performs no text
    /// edits — this exists so a later text-editing phase doesn't require a
    /// render-layer rewrite.
    pub fn text_runs(
        &self,
        doc: DocumentHandle,
        page_index: u32,
        priority: Priority,
    ) -> JobHandle<Vec<TextRun>> {
        match global_actor() {
            Ok(actor) => actor.submit(priority, move |state: &mut PdfiumState| {
                text_runs_job(state, doc, page_index)
            }),
            Err(error) => JobHandle::failed(error),
        }
    }

    /// Finds exact, case-sensitive matches of `query` across the whole
    /// document, in render-side page order, in a single actor round-trip.
    ///
    /// This enforces no document permissions: like [`text_runs`](Self::text_runs)
    /// it is the raw capability, and policy stays at the shell boundary (see
    /// `pdf-ffi`'s `search`, which gates on the document's extraction
    /// permission before delegating here).
    pub fn search(
        &self,
        doc: DocumentHandle,
        query: String,
        priority: Priority,
    ) -> JobHandle<Vec<TextMatch>> {
        match global_actor() {
            Ok(actor) => actor.submit(priority, move |state: &mut PdfiumState| {
                search_job(state, doc, &query)
            }),
            Err(error) => JobHandle::failed(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_pdfium() {
        assert!(matches!(
            bind_actor("x".into()),
            Err(RenderError::LibraryLoad { .. })
        ));
    }
}
