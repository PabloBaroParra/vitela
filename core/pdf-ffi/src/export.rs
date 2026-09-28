//! Exporting pages as images: which pages, what each file is called, whether
//! a page is too large to raster, and the encoded page itself.
//!
//! Every rule here already exists in the core — the page grammar and the file
//! naming in `pdf_save::export`, the raster ceiling in `pdf-render`. This
//! module only carries them across. A shell that parsed `"1-3,7"` or named
//! `report-03.png` on its own would be a second answer to a question the GTK
//! shell already asks the core, and the two would drift.
//!
//! The permission gate is `/P` bit 5, the one text extraction asks: PDF 1.7
//! table 22 grants it for copying or otherwise extracting "text and
//! graphics", and a page image is exactly that. Nothing is modified, so the
//! modify-contents bit is not the question.

use crate::document::DocumentHandle;
use crate::error::FfiError;

/// Mirrors `pdf_save::ExportFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiExportFormat {
    Png,
    /// No alpha channel: the page is flattened onto an opaque background.
    Jpeg,
}

impl From<FfiExportFormat> for pdf_save::ExportFormat {
    fn from(format: FfiExportFormat) -> Self {
        match format {
            FfiExportFormat::Png => pdf_save::ExportFormat::Png,
            FfiExportFormat::Jpeg => pdf_save::ExportFormat::Jpeg,
        }
    }
}

/// Reads a one-based page selection — `"1-3,7"` — into ascending,
/// deduplicated **zero-based** positions.
///
/// # Errors
///
/// [`FfiError::InvalidPageSelection`] carrying the core's own sentence, which
/// is written for the person who typed the range and is meant to be shown as
/// it is.
#[uniffi::export]
pub fn parse_page_selection(input: String, total_pages: u32) -> Result<Vec<u32>, FfiError> {
    pdf_save::parse_page_selection(&input, total_pages).map_err(|error| {
        FfiError::InvalidPageSelection {
            detail: error.to_string(),
        }
    })
}

/// The file name the page at `page_index` is written under, named after the
/// open document: `"report.pdf"` page 3 of 12 as a JPEG is `"report-03.jpg"`.
///
/// Always a single path component, whatever `document_name` holds, so joining
/// it onto a folder the user chose cannot land anywhere else.
#[uniffi::export]
pub fn page_image_file_name(
    document_name: String,
    page_index: u32,
    total_pages: u32,
    format: FfiExportFormat,
) -> String {
    pdf_save::page_image_file_name(
        pdf_save::document_file_stem(&document_name),
        page_index,
        total_pages,
        format.into(),
    )
}

/// The first of `pages` (zero-based positions) whose whole-page raster at
/// `dpi` exceeds the ceiling every render enforces, or `None` when all fit.
///
/// Asked before an export starts, so a page that would be refused is named up
/// front instead of after every page before it has been written. A position
/// the document does not have is not "too large" — rendering it fails on its
/// own terms.
#[uniffi::export]
pub fn first_page_too_large_to_export(
    handle: &DocumentHandle,
    pages: Vec<u32>,
    dpi: u32,
) -> Option<u32> {
    let dimensions = handle.page_dimensions();
    pages.into_iter().find(|&page| {
        dimensions.get(page as usize).is_some_and(|size| {
            !pdf_render::full_page_raster_fits(size.width_pt as f32, size.height_pt as f32, dpi)
        })
    })
}

/// Renders the page at position `page_index` at `dpi` and encodes it as
/// `format`, returning the bytes of the file to write.
///
/// Draws what [`crate::render_page`] draws — the last opened, saved or
/// refreshed state of the document.
///
/// # Errors
///
/// [`FfiError::UnsupportedOperation`] when the document withholds extraction,
/// [`FfiError::PageIndexOutOfBounds`] for a position it does not have, and
/// [`FfiError::RenderFailed`] for a page over the raster ceiling — ask
/// [`first_page_too_large_to_export`] first to hear about that one earlier.
#[uniffi::export]
pub fn export_page_image(
    handle: &DocumentHandle,
    page_index: u32,
    dpi: u32,
    format: FfiExportFormat,
) -> Result<Vec<u8>, FfiError> {
    let render_doc = {
        let state = handle.lock();
        if !state.text_extraction_allowed() {
            return Err(FfiError::UnsupportedOperation {
                detail: "this document does not permit extracting its pages as images".to_string(),
            });
        }
        state.render_doc().ok_or(FfiError::DocumentNotFound)?
    };

    let renderer = pdf_render::PdfiumRenderer::new();
    Ok(pdf_save::export_page_as_image(
        &renderer,
        render_doc,
        page_index,
        dpi,
        format.into(),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both arms, so a third format cannot be added on one side only.
    #[test]
    fn each_format_reaches_the_core_as_itself() {
        assert_eq!(
            pdf_save::ExportFormat::from(FfiExportFormat::Png),
            pdf_save::ExportFormat::Png
        );
        assert_eq!(
            pdf_save::ExportFormat::from(FfiExportFormat::Jpeg),
            pdf_save::ExportFormat::Jpeg
        );
    }

    #[test]
    fn an_empty_selection_is_refused_rather_than_read_as_every_page() {
        assert!(matches!(
            parse_page_selection("  ".to_string(), 4),
            Err(FfiError::InvalidPageSelection { .. })
        ));
    }
}
