//! Public API: opens documents, renders pages, and queries text runs, all
//! serialized through the single pdfium actor (T-015, T-016, T-018).

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use pdfium_render::prelude::{
    PdfPageRenderRotation, PdfRenderConfig, Pdfium, PdfiumError, PdfiumInternalError,
};

use crate::actor::{Actor, JobHandle};
use crate::bitmap::{Bitmap, BitmapHandle};
use crate::error::RenderError;
use crate::inversion::invert_rgba_in_place;
use crate::library::resolve_library_path;
use crate::options::{Priority, Rect, RenderOptions};
use crate::state::{DocHandle, PdfiumState};
use crate::text::{collect_text_runs, find_matches, TextMatch, TextRun};

pub use crate::state::DocHandle as DocumentHandle;

/// The pdfium actor: `Actor<PdfiumState>`, one dedicated OS thread owning
/// every pdfium call for the process (see `actor.rs` and `design.md`).
pub type PdfiumActor = Actor<PdfiumState>;

/// A cancellable handle to an in-flight or completed page render.
pub type RenderHandle = JobHandle<BitmapHandle>;

fn global_actor() -> Result<&'static PdfiumActor, RenderError> {
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

fn map_pdfium_error(err: PdfiumError) -> RenderError {
    if let PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) = &err {
        RenderError::InvalidPassword
    } else {
        RenderError::OpenDocument(err.to_string())
    }
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
                let document = state
                    .pdfium
                    .load_pdf_from_file(&path, password.as_deref())
                    .map_err(map_pdfium_error)?;
                Ok(state.insert_document(document))
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
                let document = state
                    .pdfium
                    .load_pdf_from_byte_vec(bytes, password.as_deref())
                    .map_err(map_pdfium_error)?;
                Ok(state.insert_document(document))
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

    /// Queries every page's size in PDF points in a single actor round-trip.
    ///
    /// Shells laying out a whole document need all page sizes up front; one
    /// batched job avoids the N serialized submit/wait cycles that querying
    /// [`page_size`](Self::page_size) per page would push through the actor.
    pub fn page_sizes(
        &self,
        doc: DocumentHandle,
        priority: Priority,
    ) -> JobHandle<Vec<(f32, f32)>> {
        match global_actor() {
            Ok(actor) => actor.submit(priority, move |state: &mut PdfiumState| {
                page_sizes_job(state, doc)
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

/// Hard ceiling on a single rasterized page, enforced for every shell that
/// reaches the renderer. A degenerate MediaBox (e.g. 1pt wide × 50,000pt tall)
/// scaled to a fit-to-width DPI would otherwise ask pdfium to allocate an
/// unbounded bitmap on the shared actor thread, freezing the pipeline or
/// exhausting memory. `MAX_RASTER_PIXELS` at 4 bytes/pixel also caps the
/// allocation at 128 MiB.
const MAX_RASTER_DIMENSION: i32 = 16_384;
const MAX_RASTER_PIXELS: u64 = 32 * 1024 * 1024;

fn ensure_raster_within_limits(width: i32, height: i32) -> Result<(), RenderError> {
    let pixels = (width.max(0) as u64).saturating_mul(height.max(0) as u64);
    if width > MAX_RASTER_DIMENSION || height > MAX_RASTER_DIMENSION || pixels > MAX_RASTER_PIXELS {
        return Err(RenderError::RenderFailed(format!(
            "requested raster {width}x{height} exceeds the maximum safe size"
        )));
    }
    Ok(())
}

fn render_page_job(
    state: &mut PdfiumState,
    doc: DocHandle,
    page_index: u32,
    dpi: u32,
    region: Option<Rect>,
    options: RenderOptions,
) -> Result<BitmapHandle, RenderError> {
    let document = state
        .documents
        .get(&doc.0)
        .ok_or(RenderError::DocumentNotFound)?;
    let page = document
        .pages()
        .get(page_index as i32)
        .map_err(|_| RenderError::PageIndexOutOfBounds(page_index))?;

    let scale = dpi as f32 / 72.0;

    let config = match region {
        None => {
            let target_width = (page.width().value * scale).round().max(1.0) as i32;
            let target_height = (page.height().value * scale).round().max(1.0) as i32;
            ensure_raster_within_limits(target_width, target_height)?;
            PdfRenderConfig::new()
                .set_target_width(target_width)
                .set_target_height(target_height)
                .rotate(PdfPageRenderRotation::None, false)
        }
        Some(rect) => {
            let full_width = (page.width().value * scale).round().max(1.0) as i32;
            let full_height = (page.height().value * scale).round().max(1.0) as i32;
            ensure_raster_within_limits(full_width, full_height)?;
            let clip_left = (rect.left * scale).round() as i32;
            let clip_top = (rect.top * scale).round() as i32;
            let clip_right = (rect.right * scale).round() as i32;
            let clip_bottom = (rect.bottom * scale).round() as i32;
            PdfRenderConfig::new()
                .set_target_width(full_width)
                .set_target_height(full_height)
                .clip(clip_left, clip_top, clip_right, clip_bottom)
        }
    };

    let bitmap = page
        .render_with_config(&config)
        .map_err(|e| RenderError::RenderFailed(e.to_string()))?;

    let width = bitmap.width() as u32;
    let height = bitmap.height() as u32;
    let mut pixels = bitmap.as_rgba_bytes();

    if options.invert_content_colors {
        invert_rgba_in_place(&mut pixels);
    }

    let stride = width * 4;
    Ok(state.bitmaps.insert(Bitmap {
        width,
        height,
        stride,
        pixels,
    }))
}

fn text_runs_job(
    state: &mut PdfiumState,
    doc: DocHandle,
    page_index: u32,
) -> Result<Vec<TextRun>, RenderError> {
    let document = state
        .documents
        .get(&doc.0)
        .ok_or(RenderError::DocumentNotFound)?;
    let page = document
        .pages()
        .get(page_index as i32)
        .map_err(|_| RenderError::PageIndexOutOfBounds(page_index))?;
    let text = page
        .text()
        .map_err(|e| RenderError::RenderFailed(e.to_string()))?;
    Ok(collect_text_runs(&text))
}

fn page_size_job(
    state: &mut PdfiumState,
    doc: DocHandle,
    page_index: u32,
) -> Result<(f32, f32), RenderError> {
    let document = state
        .documents
        .get(&doc.0)
        .ok_or(RenderError::DocumentNotFound)?;
    let page = document
        .pages()
        .get(page_index as i32)
        .map_err(|_| RenderError::PageIndexOutOfBounds(page_index))?;
    Ok((page.width().value, page.height().value))
}

fn page_count_job(state: &mut PdfiumState, doc: DocHandle) -> Result<u32, RenderError> {
    let document = state
        .documents
        .get(&doc.0)
        .ok_or(RenderError::DocumentNotFound)?;
    Ok(document.pages().len() as u32)
}

fn search_job(
    state: &mut PdfiumState,
    doc: DocHandle,
    query: &str,
) -> Result<Vec<TextMatch>, RenderError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let document = state
        .documents
        .get(&doc.0)
        .ok_or(RenderError::DocumentNotFound)?;
    let pages = document.pages();
    let count = pages.len() as u32;
    let mut matches = Vec::new();
    for page_index in 0..count {
        let page = pages
            .get(page_index as i32)
            .map_err(|_| RenderError::PageIndexOutOfBounds(page_index))?;
        let text = page
            .text()
            .map_err(|e| RenderError::RenderFailed(e.to_string()))?;
        matches.extend(find_matches(&collect_text_runs(&text), query, page_index));
    }
    Ok(matches)
}

fn page_sizes_job(state: &mut PdfiumState, doc: DocHandle) -> Result<Vec<(f32, f32)>, RenderError> {
    let document = state
        .documents
        .get(&doc.0)
        .ok_or(RenderError::DocumentNotFound)?;
    let pages = document.pages();
    let count = pages.len() as u32;
    let mut sizes = Vec::with_capacity(count as usize);
    for page_index in 0..count {
        let page = pages
            .get(page_index as i32)
            .map_err(|_| RenderError::PageIndexOutOfBounds(page_index))?;
        sizes.push((page.width().value, page.height().value));
    }
    Ok(sizes)
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

    #[test]
    fn raster_limit_accepts_a_normal_page() {
        assert!(ensure_raster_within_limits(1_240, 1_754).is_ok());
    }

    #[test]
    fn raster_limit_rejects_a_degenerate_page() {
        // A 1pt × 50,000pt MediaBox at a fit-to-width DPI.
        assert!(matches!(
            ensure_raster_within_limits(600, 30_000_000),
            Err(RenderError::RenderFailed(_))
        ));
    }
}
