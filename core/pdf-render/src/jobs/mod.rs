//! The pdfium work itself, as plain synchronous functions over
//! `&mut PdfiumState`.
//!
//! Every function here runs to completion on the calling thread and knows
//! nothing about how it was scheduled. Native builds call them from inside
//! the pdfium actor's jobs (`renderer.rs`); wasm32 builds, which have no
//! threads, call them directly (`sync_renderer.rs`), where the Web Worker
//! hosting the module is the actor. Keeping the work out of the closures is
//! what lets both front ends share one implementation.

mod raster;
mod text;

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use pdfium_render::prelude::{
    PdfDocument, PdfPage, PdfPageRenderRotation, PdfiumError, PdfiumInternalError,
};

use crate::error::RenderError;
use crate::selection::{PageGeometry, PageRotation};
use crate::state::{DocHandle, PdfiumState};

pub use raster::full_page_raster_fits;
#[cfg(target_arch = "wasm32")]
pub(crate) use raster::{render_page_bitmap, render_page_tiles_bitmaps};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use raster::{render_page_job, render_page_tiles_job};
pub(crate) use text::{search_job, text_runs_job};

pub(crate) fn map_pdfium_error(err: PdfiumError) -> RenderError {
    if let PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) = &err {
        RenderError::InvalidPassword
    } else {
        RenderError::OpenDocument(err.to_string())
    }
}

/// Opens a document from a filesystem path. Native only: the web has no
/// paths, and pdfium-render has no `load_pdf_from_file` on wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn open_from_path_job(
    state: &mut PdfiumState,
    path: &Path,
    password: Option<&str>,
) -> Result<DocHandle, RenderError> {
    let document = state
        .pdfium
        .load_pdf_from_file(path, password)
        .map_err(map_pdfium_error)?;
    Ok(state.insert_document(document))
}

/// Opens a document from an in-memory buffer. Ownership of `bytes` goes to
/// pdfium (`load_pdf_from_byte_vec`), which keeps it alive for the
/// document's lifetime.
pub(crate) fn open_from_bytes_job(
    state: &mut PdfiumState,
    bytes: Vec<u8>,
    password: Option<&str>,
) -> Result<DocHandle, RenderError> {
    let document = state
        .pdfium
        .load_pdf_from_byte_vec(bytes, password)
        .map_err(map_pdfium_error)?;
    Ok(state.insert_document(document))
}

/// Looks up an open document, or reports the handle as unknown.
pub(crate) fn document(
    state: &PdfiumState,
    doc: DocHandle,
) -> Result<&PdfDocument<'static>, RenderError> {
    state
        .documents
        .get(&doc.0)
        .ok_or(RenderError::DocumentNotFound)
}

/// Loads one page (`FPDF_LoadPage`, which parses its content stream).
pub(crate) fn page<'a>(
    document: &'a PdfDocument<'static>,
    page_index: u32,
) -> Result<PdfPage<'a>, RenderError> {
    document
        .pages()
        .get(page_index as i32)
        .map_err(|_| RenderError::PageIndexOutOfBounds(page_index))
}

pub(crate) fn page_size_job(
    state: &mut PdfiumState,
    doc: DocHandle,
    page_index: u32,
) -> Result<(f32, f32), RenderError> {
    let document = document(state, doc)?;
    let page = page(document, page_index)?;
    Ok((page.width().value, page.height().value))
}

pub(crate) fn page_count_job(state: &mut PdfiumState, doc: DocHandle) -> Result<u32, RenderError> {
    Ok(document(state, doc)?.pages().len() as u32)
}

pub(crate) fn page_geometry_job(
    state: &mut PdfiumState,
    doc: DocHandle,
) -> Result<Vec<PageGeometry>, RenderError> {
    let document = document(state, doc)?;
    let count = document.pages().len() as u32;
    let mut geometry = Vec::with_capacity(count as usize);
    for page_index in 0..count {
        let page = page(document, page_index)?;
        geometry.push(PageGeometry {
            // Already turned: pdfium swaps a page's width and height for a
            // quarter turn, which is why `rotation` below is extra
            // information and not a second way of saying the same thing.
            width_pt: page.width().value,
            height_pt: page.height().value,
            // A page whose `/Rotate` pdfium cannot read is drawn upright, so
            // treating the failure as "no turn" keeps the overlay transform
            // agreeing with the raster rather than guessing against it.
            rotation: page.rotation().map_or(PageRotation::None, page_rotation),
        });
    }
    Ok(geometry)
}

fn page_rotation(rotation: PdfPageRenderRotation) -> PageRotation {
    match rotation {
        PdfPageRenderRotation::None => PageRotation::None,
        PdfPageRenderRotation::Degrees90 => PageRotation::Clockwise90,
        PdfPageRenderRotation::Degrees180 => PageRotation::Clockwise180,
        PdfPageRenderRotation::Degrees270 => PageRotation::Clockwise270,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::options::Priority;
    use crate::renderer::global_actor;

    fn fixture_bytes(pages: u32) -> Vec<u8> {
        let mut doc = gen_fixtures::build_multi_page_document(pages, "pdf-render-jobs");
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("serialize fixture");
        bytes
    }

    #[test]
    fn open_from_bytes_job_registers_a_countable_document() {
        let bytes = fixture_bytes(3);
        let count = global_actor()
            .expect("bind pdfium")
            .submit(Priority::Visible, move |state| {
                let doc = open_from_bytes_job(state, bytes, None)?;
                let count = page_count_job(state, doc);
                state.close_document(doc);
                count
            })
            .wait();
        assert_eq!(count.unwrap(), 3);
    }

    #[test]
    fn open_from_bytes_job_rejects_bytes_that_are_not_a_pdf() {
        let result = global_actor()
            .expect("bind pdfium")
            .submit(Priority::Visible, |state| {
                open_from_bytes_job(state, b"not a pdf".to_vec(), None)
            })
            .wait();
        assert!(matches!(result, Err(RenderError::OpenDocument(_))));
    }
}
