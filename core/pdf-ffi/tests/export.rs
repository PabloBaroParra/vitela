//! The export-to-images surface: which pages, what each file is called,
//! whether a page is too large to raster, and the encoded page itself.
//!
//! The grammar and the naming rules are pinned in `pdf-save`'s own tests; what
//! these add is that the boundary hands them across unchanged — a shell that
//! got a different sentence, or a different file name, than the GTK shell
//! would be the bug the shared rules exist to prevent.

use pdf_ffi::{
    export_page_image, first_page_too_large_to_export, open_from_bytes, page_image_file_name,
    parse_page_selection, FfiError, FfiExportFormat,
};

fn letter_fixture() -> std::sync::Arc<pdf_ffi::DocumentHandle> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("encrypted")
        .join("rc4_128_user_and_owner.pdf");
    let bytes = std::fs::read(path).expect("fixture must be readable");
    open_from_bytes(bytes, Some("user-rc4-pass".to_string())).expect("fixture should open")
}

#[test]
fn a_page_selection_comes_back_zero_based_sorted_and_deduplicated() {
    assert_eq!(
        parse_page_selection("5,1-3,2".to_string(), 10).unwrap(),
        vec![0, 1, 2, 4]
    );
}

/// The reader sees the core's own sentence, not a second copy of it.
#[test]
fn a_refused_selection_carries_the_cores_sentence() {
    match parse_page_selection("7-3".to_string(), 10) {
        Err(FfiError::InvalidPageSelection { detail }) => assert_eq!(
            detail,
            pdf_save::PageSelectionError::DescendingRange { from: 7, to: 3 }.to_string()
        ),
        other => panic!("expected InvalidPageSelection, got {other:?}"),
    }
}

#[test]
fn a_file_name_drops_the_pdf_extension_and_pads_the_page_number() {
    assert_eq!(
        page_image_file_name("report.pdf".to_string(), 2, 12, FfiExportFormat::Jpeg),
        "report-03.jpg"
    );
    assert_eq!(
        page_image_file_name("report.pdf".to_string(), 0, 1, FfiExportFormat::Png),
        "report-1.png"
    );
}

#[test]
fn a_document_name_that_would_escape_the_folder_is_flattened() {
    assert_eq!(
        page_image_file_name("..\\..\\evil.pdf".to_string(), 0, 1, FfiExportFormat::Png),
        "evil-1.png"
    );
}

/// US Letter at 150 DPI is 1275x1650.
#[test]
fn an_exported_png_decodes_at_the_requested_resolution() {
    let handle = letter_fixture();

    let bytes = export_page_image(&handle, 0, 150, FfiExportFormat::Png).expect("export");

    assert_eq!(&bytes[0..4], &[0x89, b'P', b'N', b'G']);
    let decoded = image::load_from_memory(&bytes).expect("must decode");
    assert_eq!((decoded.width(), decoded.height()), (1275, 1650));
}

#[test]
fn an_exported_jpeg_is_a_jpeg() {
    let handle = letter_fixture();

    let bytes = export_page_image(&handle, 0, 72, FfiExportFormat::Jpeg).expect("export");

    assert_eq!(&bytes[0..2], &[0xFF, 0xD8]);
}

#[test]
fn a_page_past_the_end_is_refused_by_position() {
    let handle = letter_fixture();
    let past_the_end = handle.page_count();

    assert!(matches!(
        export_page_image(&handle, past_the_end, 72, FfiExportFormat::Png),
        Err(FfiError::PageIndexOutOfBounds { .. })
    ));
}

/// Letter fits at 400 DPI and not at 600 — the reason a shell's ceiling is 400.
#[test]
fn an_oversized_page_is_named_before_anything_is_rendered() {
    let handle = letter_fixture();

    assert_eq!(first_page_too_large_to_export(&handle, vec![0], 400), None);
    assert_eq!(
        first_page_too_large_to_export(&handle, vec![0], 600),
        Some(0)
    );
}

#[test]
fn a_page_the_document_does_not_have_is_not_called_oversized() {
    let handle = letter_fixture();

    assert_eq!(first_page_too_large_to_export(&handle, vec![99], 600), None);
}
