//! Exporting pages as images (B26 T-252): which pages, what each file is
//! called, whether a page is too large to raster, the snapshot the pages are
//! drawn from, and the encoder.
//!
//! Every rule is the core's, asked the way `pdf-ffi`'s `export` module asks
//! it: the page grammar (`pdf_save::parse_page_selection`), the file names
//! (`pdf_save::page_image_file_name`), the raster ceiling
//! (`pdf_render::full_page_raster_fits`) and the encoder
//! (`pdf_save::encode_rgba_image` — the half of `export_page_as_image` that
//! builds on wasm32; the render half is the engine's synchronous renderer).
//!
//! The gate is the text-extraction permission, as in `pdf-ffi`: a page image
//! is "text and graphics" leaving the document.
//!
//! The pages are drawn from the **output snapshot** — the bytes a save would
//! write now, signatures acknowledged, as `pdf-ffi`'s `output_snapshot` — not
//! from the on-screen preview: the preview leaves the session's annotations
//! out for the overlay to draw, so an unsaved highlight would be missing from
//! every image. The Android and Windows shells export from it for the same
//! reason. Exporting is not an edit: nothing is recorded.
//!
//! Printing (B26 T-257) draws from the same snapshot, rendered the same way,
//! but without the extraction gate (see [`DocumentModel::print_snapshot_bytes`]).

use pdf_save::ExportFormat;

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// The formats an export may write, by the names they cross the boundary
/// under — the core's `ExportFormat`, in its order.
pub const EXPORT_FORMATS: [&str; 2] = ["png", "jpeg"];

/// The name a format crosses the boundary under.
pub fn format_name(format: ExportFormat) -> &'static str {
    // Total, no wildcard: a third format has to be named here.
    match format {
        ExportFormat::Png => "png",
        ExportFormat::Jpeg => "jpeg",
    }
}

/// The format `name` names, or `UnsupportedOperation` for a name no format
/// has.
pub fn format_from_name(name: &str) -> Result<ExportFormat, EngineError> {
    [ExportFormat::Png, ExportFormat::Jpeg]
        .into_iter()
        .find(|format| format_name(*format) == name)
        .ok_or_else(|| {
            EngineError::new(
                ErrorKind::UnsupportedOperation,
                format!("'{name}' is not an image export format"),
            )
        })
}

/// Reads a one-based page selection (`"1-3,7"`) into ascending, deduplicated
/// **zero-based** positions. A selection the core cannot read is
/// `InvalidPageSelection`, with the core's sentence for the person who typed
/// it.
pub fn parse_page_selection(input: &str, total_pages: u32) -> Result<Vec<u32>, EngineError> {
    pdf_save::parse_page_selection(input, total_pages)
        .map_err(|err| EngineError::new(ErrorKind::InvalidPageSelection, err.to_string()))
}

/// The file the page at `page_index` is written under, named after the open
/// document: `"report.pdf"` page 3 of 12 as a JPEG is `"report-03.jpg"`.
/// Always one path component.
pub fn page_image_file_name(
    document_name: &str,
    page_index: u32,
    total_pages: u32,
    format: ExportFormat,
) -> String {
    pdf_save::page_image_file_name(
        pdf_save::document_file_stem(document_name),
        page_index,
        total_pages,
        format,
    )
}

/// Encodes a rendered page (`width * height * 4` RGBA bytes) as `format`.
pub fn encode_page_image(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    format: ExportFormat,
) -> Result<Vec<u8>, EngineError> {
    pdf_save::encode_rgba_image(width, height, rgba, format)
        .map_err(|err| EngineError::new(ErrorKind::RenderFailed, err.to_string()))
}

impl DocumentModel {
    /// Refuses an export the document's permissions withhold, with
    /// `pdf-ffi`'s sentence.
    pub fn ensure_image_export_allowed(&self) -> Result<(), EngineError> {
        if self.text_extraction_allowed() {
            Ok(())
        } else {
            Err(EngineError::new(
                ErrorKind::UnsupportedOperation,
                "this document does not permit extracting its pages as images",
            ))
        }
    }

    /// The first of `pages` (zero-based, the document's current order) whose
    /// whole-page raster at `dpi` exceeds the ceiling every render enforces,
    /// or `None` when all fit. A position the document does not have is not
    /// "too large": rendering it fails on its own terms.
    pub fn first_page_too_large_to_export(&self, pages: &[u32], dpi: u32) -> Option<u32> {
        let sizes = self.page_sizes();
        pages.iter().copied().find(|&page| {
            sizes.get(page as usize).is_some_and(|size| {
                !pdf_render::full_page_raster_fits(size.width_pt as f32, size.height_pt as f32, dpi)
            })
        })
    }

    /// The bytes an export draws its pages from: what a save would write
    /// now, every pending edit and annotation in it. A signature is
    /// acknowledged, as `pdf-ffi`'s `output_snapshot` does: the snapshot is
    /// never handed out as a PDF, only rasterized.
    pub fn output_snapshot_bytes(&self) -> Result<Vec<u8>, EngineError> {
        self.ensure_image_export_allowed()?;
        self.print_snapshot_bytes()
    }

    /// The same snapshot for printing (B26 T-257), with no permission asked:
    /// `pdf-ffi`'s `output_snapshot` asks none, and neither does any native
    /// shell before its print dialog — the core has no rule for the `/P`
    /// print bit, so the web does not invent one.
    pub fn print_snapshot_bytes(&self) -> Result<Vec<u8>, EngineError> {
        Ok(self.with_save_input(true, pdf_save::save_document)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{pdfs, AnnotationEdit, PageEdit};
    use pdf_annotate::NewAnnotation;
    use pdf_document::{Color, Rect};
    use pdf_manip::OpenCredentials;

    fn open(bytes: &[u8]) -> DocumentModel {
        DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
    }

    #[test]
    fn the_formats_round_trip_by_name_and_an_unknown_one_is_refused() {
        assert_eq!(EXPORT_FORMATS, ["png", "jpeg"]);
        for name in EXPORT_FORMATS {
            assert_eq!(format_name(format_from_name(name).unwrap()), name);
        }
        let err = format_from_name("gif").unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
        assert!(err.message.contains("gif"), "{}", err.message);
    }

    #[test]
    fn a_selection_is_read_by_the_cores_grammar() {
        assert_eq!(parse_page_selection("3, 1-2,2", 5).unwrap(), [0, 1, 2]);
        let err = parse_page_selection("  ", 5).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidPageSelection);
        let err = parse_page_selection("9", 5).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidPageSelection);
        assert!(!err.message.is_empty());
        assert_eq!(err.kind.name(), "InvalidPageSelection");
    }

    #[test]
    fn a_file_is_named_after_the_document_with_a_padded_page_number() {
        assert_eq!(
            page_image_file_name("report.pdf", 2, 12, ExportFormat::Jpeg),
            "report-03.jpg"
        );
        assert_eq!(
            page_image_file_name("scan.PDF", 0, 1, ExportFormat::Png),
            "scan-1.png"
        );
    }

    #[test]
    fn a_page_encodes_to_the_signature_of_its_format() {
        let rgba = vec![255; 3 * 2 * 4];
        let png = encode_page_image(3, 2, rgba.clone(), ExportFormat::Png).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let jpeg = encode_page_image(3, 2, rgba, ExportFormat::Jpeg).unwrap();
        assert_eq!(&jpeg[..3], [0xFF, 0xD8, 0xFF]);

        let err = encode_page_image(3, 2, vec![0; 5], ExportFormat::Png).unwrap_err();
        assert_eq!(err.kind, ErrorKind::RenderFailed);
    }

    #[test]
    fn letter_fits_at_400_dpi_and_not_at_600() {
        let model = open(&pdfs::text_document(&["one", "two"]));
        assert_eq!(model.first_page_too_large_to_export(&[0, 1], 400), None);
        assert_eq!(model.first_page_too_large_to_export(&[1, 0], 600), Some(1));
        assert_eq!(model.first_page_too_large_to_export(&[7], 600), None);
    }

    #[test]
    fn the_snapshot_carries_the_pending_page_edits_and_annotations_and_records_nothing() {
        let mut model = open(&pdfs::text_document(&["one", "two", "three"]));
        model
            .apply_edit(PageEdit::Remove { index: 0, count: 1 })
            .unwrap();
        model
            .apply_annotation_edit(AnnotationEdit::Add {
                page: 0,
                annotation: NewAnnotation::Highlight {
                    rect: Rect {
                        x: 72.0,
                        y: 690.0,
                        width: 120.0,
                        height: 30.0,
                    },
                    color: Color {
                        r: 255,
                        g: 255,
                        b: 0,
                    },
                },
            })
            .unwrap();

        let bytes = model.output_snapshot_bytes().expect("snapshot");
        let snapshot = lopdf::Document::load_mem(&bytes).expect("parses");
        let pages = snapshot.get_pages();
        assert_eq!(pages.len(), 2);
        let first = snapshot.get_dictionary(pages[&1]).unwrap();
        assert!(
            first.get(b"Annots").is_ok(),
            "the unsaved highlight is drawn into the snapshot"
        );
        // Not an edit: two steps to undo, none to redo.
        assert!(model.can_undo());
        assert!(!model.can_redo());
    }

    #[test]
    fn a_signed_document_snapshots_without_asking() {
        let model = open(pdfs::SIGNED);
        assert!(model.output_snapshot_bytes().is_ok());
    }

    #[test]
    fn a_document_that_withholds_extraction_is_refused() {
        let model = DocumentModel::open(
            &pdfs::restricted_document(),
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        let err = model.ensure_image_export_allowed().unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
        assert_eq!(
            err.message,
            "this document does not permit extracting its pages as images"
        );
        assert_eq!(
            model.output_snapshot_bytes().unwrap_err().kind,
            ErrorKind::UnsupportedOperation
        );
    }

    #[test]
    fn a_print_snapshot_carries_the_pending_annotations_and_records_nothing() {
        let mut model = open(&pdfs::text_document(&["one", "two"]));
        model
            .apply_annotation_edit(AnnotationEdit::Add {
                page: 1,
                annotation: NewAnnotation::Highlight {
                    rect: Rect {
                        x: 72.0,
                        y: 690.0,
                        width: 120.0,
                        height: 30.0,
                    },
                    color: Color {
                        r: 255,
                        g: 255,
                        b: 0,
                    },
                },
            })
            .unwrap();

        let bytes = model.print_snapshot_bytes().expect("snapshot");
        let snapshot = lopdf::Document::load_mem(&bytes).expect("parses");
        let pages = snapshot.get_pages();
        assert_eq!(pages.len(), 2);
        let second = snapshot.get_dictionary(pages[&2]).unwrap();
        assert!(second.get(b"Annots").is_ok());
        assert!(model.can_undo());
        assert!(!model.can_redo());
    }

    #[test]
    fn printing_asks_no_permission_as_no_native_shell_does() {
        let model = DocumentModel::open(
            &pdfs::restricted_document(),
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        assert!(model.ensure_image_export_allowed().is_err());
        assert!(model.print_snapshot_bytes().is_ok());
    }
}
