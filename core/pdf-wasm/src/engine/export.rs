//! The `Engine`'s image-export surface (B26 T-252): `pdf-ffi`'s
//! `parse_page_selection`, `page_image_file_name`,
//! `first_page_too_large_to_export`, `output_snapshot` and
//! `export_page_image`, with one difference forced by the target:
//! `pdf_save::export_page_as_image` is native only (it drives the threaded
//! renderer), so a page is rendered by this engine's synchronous renderer and
//! encoded by `pdf_save::encode_rgba_image`, the half of it that builds here.
//!
//! An export draws from its own snapshot of what a save would write now
//! (`openImageExport`), opened once and closed when the export ends
//! (`closeImageExport`), so every page of one export shows the same state
//! and the document can be edited meanwhile. The rules are in
//! [`crate::model`]'s export module.

use pdf_render::{DocumentHandle, RenderOptions};
use wasm_bindgen::prelude::*;

use super::{js_error, Engine};
use crate::convert::tight_rgba;
use crate::dto::OpenedImageExport;
use crate::error::EngineError;
use crate::model::{
    encode_page_image, format_from_name, page_image_file_name as file_name,
    parse_page_selection as parse, EXPORT_FORMATS,
};

/// One export's snapshot: the pdfium handle it renders from and how many
/// pages it has.
pub(super) struct ImageExport {
    render: DocumentHandle,
    page_count: u32,
}

/// The formats an export may write (`png`, `jpeg`): the core's own list.
#[wasm_bindgen(js_name = imageExportFormats)]
pub fn image_export_formats() -> Vec<String> {
    EXPORT_FORMATS.into_iter().map(str::to_string).collect()
}

/// Reads a one-based page selection (`"1-3,7"`) into ascending, deduplicated
/// zero-based positions. Refused with `InvalidPageSelection`, whose message
/// is the core's sentence for the person who typed it.
#[wasm_bindgen(js_name = parsePageSelection)]
pub fn parse_page_selection(input: String, total_pages: u32) -> Result<Vec<u32>, JsValue> {
    parse(&input, total_pages).map_err(js_error)
}

/// The file the page at `pageIndex` is exported under, named after the open
/// document: `"report.pdf"` page 3 of 12 as `jpeg` is `"report-03.jpg"`.
/// Always one path component. Refused with `UnsupportedOperation` for an
/// unknown format.
#[wasm_bindgen(js_name = pageImageFileName)]
pub fn page_image_file_name(
    document_name: String,
    page_index: u32,
    total_pages: u32,
    format: String,
) -> Result<String, JsValue> {
    let format = format_from_name(&format).map_err(js_error)?;
    Ok(file_name(&document_name, page_index, total_pages, format))
}

#[wasm_bindgen]
impl Engine {
    /// The first of `pages` (zero-based) whose whole-page raster at `dpi` is
    /// over the ceiling every render enforces, or `undefined` when all fit —
    /// asked before an export starts, so the page is named up front.
    #[wasm_bindgen(js_name = firstPageTooLargeToExport)]
    pub fn first_page_too_large_to_export(
        &self,
        doc: u32,
        pages: Vec<u32>,
        dpi: u32,
    ) -> Result<Option<u32>, JsValue> {
        self.document(doc)
            .map(|open| open.model.first_page_too_large_to_export(&pages, dpi))
            .map_err(js_error)
    }

    /// Starts an export: opens a snapshot of what a save would write now
    /// (every pending edit and annotation in it; a signature is not asked
    /// about, nothing is written) and returns its id with the snapshot's
    /// page count — what to plan the export against. Records no edit.
    /// Refused with `UnsupportedOperation` when the document withholds
    /// extraction. Close it with `closeImageExport`.
    #[wasm_bindgen(js_name = openImageExport)]
    pub fn open_image_export(&mut self, doc: u32) -> Result<OpenedImageExport, JsValue> {
        self.open_export(doc).map_err(js_error)
    }

    /// Starts a print (B26 T-257): the same snapshot as `openImageExport`,
    /// pages drawn with `exportPageImage` and closed with
    /// `closeImageExport`, but asked no permission — no native shell asks
    /// one before printing, and the core has no print-bit rule.
    #[wasm_bindgen(js_name = openPrintSnapshot)]
    pub fn open_print_snapshot(&mut self, doc: u32) -> Result<OpenedImageExport, JsValue> {
        let bytes = self
            .document(doc)
            .and_then(|open| open.model.print_snapshot_bytes())
            .map_err(js_error)?;
        self.open_snapshot(doc, bytes).map_err(js_error)
    }

    /// One page of an export, rendered at `dpi` and encoded as `format`:
    /// the bytes of the file. Refused with `DocumentNotFound` for an export
    /// that is not open, `PageIndexOutOfBounds`, `UnsupportedOperation` for
    /// an unknown format, and `RenderFailed` for a page over the raster
    /// ceiling (ask `firstPageTooLargeToExport` first).
    #[wasm_bindgen(js_name = exportPageImage)]
    pub fn export_page_image(
        &mut self,
        export: u32,
        page_index: u32,
        dpi: u32,
        format: String,
    ) -> Result<Vec<u8>, JsValue> {
        self.export_page(export, page_index, dpi, &format)
            .map_err(js_error)
    }

    /// Ends an export, closing its snapshot. `false` if it was not open.
    #[wasm_bindgen(js_name = closeImageExport)]
    pub fn close_image_export(&mut self, export: u32) -> bool {
        match self.exports.remove(&export) {
            Some(open) => self.renderer.close_document(open.render),
            None => false,
        }
    }
}

impl Engine {
    fn open_export(&mut self, doc: u32) -> Result<OpenedImageExport, EngineError> {
        let bytes = self.document(doc)?.model.output_snapshot_bytes()?;
        self.open_snapshot(doc, bytes)
    }

    /// Opens `bytes`, a snapshot of `doc`, as an export to draw pages from.
    fn open_snapshot(
        &mut self,
        doc: u32,
        bytes: Vec<u8>,
    ) -> Result<OpenedImageExport, EngineError> {
        let open = self.document(doc)?;
        let page_count = open.model.page_count();
        let password = open.render_password.clone();
        let render = self
            .renderer
            .open_document_from_bytes(bytes, password.as_deref())?;
        let id = self.next_export;
        self.next_export = self.next_export.wrapping_add(1);
        self.exports.insert(id, ImageExport { render, page_count });
        Ok(OpenedImageExport::new(id, page_count))
    }

    fn export_page(
        &mut self,
        export: u32,
        page_index: u32,
        dpi: u32,
        format: &str,
    ) -> Result<Vec<u8>, EngineError> {
        let format = format_from_name(format)?;
        let open = self.exports.get(&export).ok_or_else(|| {
            EngineError::new(
                crate::error::ErrorKind::DocumentNotFound,
                format!("no open image export with id {export}"),
            )
        })?;
        if page_index >= open.page_count {
            return Err(EngineError::page_out_of_bounds(page_index));
        }
        let bitmap = self.renderer.render_page(
            open.render,
            page_index,
            dpi,
            None,
            RenderOptions::default(),
        )?;
        let (width, height) = (bitmap.width, bitmap.height);
        encode_page_image(width, height, tight_rgba(bitmap), format)
    }
}
