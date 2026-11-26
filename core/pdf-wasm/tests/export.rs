//! The `Engine`'s image-export surface against the real pdfium, in a
//! headless browser (B26 T-252). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{image_export_formats, page_image_file_name, parse_page_selection, EditCommand};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_SIGNATURE: &[u8] = &[0xFF, 0xD8, 0xFF];

/// The IHDR width and height of a PNG.
fn png_size(bytes: &[u8]) -> (u32, u32) {
    let word = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    (word(16), word(20))
}

#[wasm_bindgen_test]
fn the_formats_selection_and_names_are_the_cores() {
    assert_eq!(image_export_formats(), ["png", "jpeg"]);
    assert_eq!(parse_page_selection("2-3,1".into(), 4).unwrap(), [0, 1, 2]);
    assert_eq!(
        error_name(parse_page_selection("5".into(), 4).unwrap_err()),
        "InvalidPageSelection"
    );
    assert_eq!(
        page_image_file_name("report.pdf".into(), 2, 12, "jpeg".into()).unwrap(),
        "report-03.jpg"
    );
    assert_eq!(
        error_name(page_image_file_name("report.pdf".into(), 0, 1, "gif".into()).unwrap_err()),
        "UnsupportedOperation"
    );
}

#[wasm_bindgen_test]
async fn a_page_exports_as_png_at_the_dpi_and_as_jpeg() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["one", "two"]), None)
        .unwrap();
    assert_eq!(
        engine
            .first_page_too_large_to_export(doc, vec![0, 1], 400)
            .unwrap(),
        None
    );
    assert_eq!(
        engine
            .first_page_too_large_to_export(doc, vec![0, 1], 600)
            .unwrap(),
        Some(0)
    );

    let opened = engine.open_image_export(doc).unwrap();
    assert_eq!(opened.page_count(), 2);
    let export = opened.id();
    let png = engine
        .export_page_image(export, 1, 72, "png".into())
        .unwrap();
    assert_eq!(&png[..8], PNG_SIGNATURE);
    assert_eq!(png_size(&png), (612, 792));
    let png = engine
        .export_page_image(export, 0, 144, "png".into())
        .unwrap();
    assert_eq!(png_size(&png), (1224, 1584));
    let jpeg = engine
        .export_page_image(export, 0, 72, "jpeg".into())
        .unwrap();
    assert_eq!(&jpeg[..3], JPEG_SIGNATURE);

    assert_eq!(
        error_name(
            engine
                .export_page_image(export, 2, 72, "png".into())
                .unwrap_err()
        ),
        "PageIndexOutOfBounds"
    );
    assert!(engine.close_image_export(export));
    assert!(!engine.close_image_export(export));
    assert_eq!(
        error_name(
            engine
                .export_page_image(export, 0, 72, "png".into())
                .unwrap_err()
        ),
        "DocumentNotFound"
    );
}

#[wasm_bindgen_test]
async fn the_export_draws_the_pending_edits_and_leaves_the_history_alone() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["one", "two", "three"]), None)
        .unwrap();
    // Page 2 turned a quarter: its image is landscape.
    engine
        .apply_edit(doc, &EditCommand::rotate_pages(1, 1, 90))
        .unwrap();
    // A page removed before the export opened is not in its snapshot.
    engine
        .apply_edit(doc, &EditCommand::remove_pages(2, 1))
        .unwrap();

    let opened = engine.open_image_export(doc).unwrap();
    assert_eq!(opened.page_count(), 2);
    let export = opened.id();
    let png = engine
        .export_page_image(export, 1, 72, "png".into())
        .unwrap();
    assert_eq!(png_size(&png), (792, 612));
    engine.close_image_export(export);

    assert!(engine.can_undo(doc).unwrap());
    assert!(!engine.can_redo(doc).unwrap());
}

#[wasm_bindgen_test]
async fn a_document_that_withholds_extraction_is_refused() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::restricted_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    assert_eq!(
        error_name(engine.open_image_export(doc).unwrap_err()),
        "UnsupportedOperation"
    );
}
