//! The export-to-images surface: which pages, what each file is called,
//! whether a page is too large to raster, and the encoded page itself.
//!
//! The grammar and the naming rules are pinned in `pdf-save`'s own tests; what
//! these add is that the boundary hands them across unchanged — a shell that
//! got a different sentence, or a different file name, than the GTK shell
//! would be the bug the shared rules exist to prevent.

use pdf_ffi::output_snapshot;
use pdf_ffi::{
    apply_edit, export_page_image, first_page_too_large_to_export, open_from_bytes,
    page_image_file_name, parse_page_selection, refresh_preview, FfiColor, FfiEditCommand,
    FfiError, FfiExportFormat, FfiRect,
};

fn letter_fixture() -> std::sync::Arc<pdf_ffi::DocumentHandle> {
    open_from_bytes(letter_bytes(), Some("user-rc4-pass".to_string())).expect("fixture should open")
}

/// The same encrypted file, opened with the password that lets the reader
/// annotate it — which the user password alone does not.
fn annotatable_letter_fixture() -> std::sync::Arc<pdf_ffi::DocumentHandle> {
    open_from_bytes(letter_bytes(), Some("owner-rc4-pass".to_string()))
        .expect("fixture should open")
}

fn letter_bytes() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("encrypted")
        .join("rc4_128_user_and_owner.pdf");
    std::fs::read(path).expect("fixture must be readable")
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

/// The bug the output snapshot exists for, reproduced through the call the
/// Windows shell makes: a highlight added in the session is drawn by the
/// shell's overlay only, so exporting the live handle leaves it out.
#[test]
fn exporting_the_live_handle_leaves_a_session_annotation_out() {
    let handle = annotatable_letter_fixture();
    let before = export_page_image(&handle, 0, 72, FfiExportFormat::Png).expect("export");

    add_highlight(&handle);
    refresh_preview(&handle).expect("refreshing the preview should succeed");
    let after = export_page_image(&handle, 0, 72, FfiExportFormat::Png).expect("export");

    assert_eq!(
        pixels(&after),
        pixels(&before),
        "the live preview never carries the session's annotations"
    );
}

/// The fix: an output snapshot is the save, reopened — so it carries the
/// annotation the live handle leaves out. The fixture is encrypted, so this
/// also pins that the snapshot reopens under the session's own password
/// without the shell having to hand it back.
#[test]
fn an_output_snapshot_exports_the_session_annotation() {
    let handle = annotatable_letter_fixture();
    let before = export_page_image(&handle, 0, 72, FfiExportFormat::Png).expect("export");
    add_highlight(&handle);

    let snapshot = output_snapshot(&handle).expect("the snapshot reopens");
    let exported = export_page_image(&snapshot, 0, 72, FfiExportFormat::Png).expect("export");

    assert_ne!(
        pixels(&exported),
        pixels(&before),
        "the snapshot carries the annotation"
    );
    assert_eq!(snapshot.page_count(), handle.page_count());
    assert!(
        snapshot.annotations().is_empty(),
        "the annotation is part of the snapshot's pages, not a second editable copy"
    );
}

/// The snapshot is opened with the session's own credential, so it grants
/// what the session grants and no more: a reader who could not annotate
/// still gets a snapshot that refuses an annotation.
#[test]
fn an_output_snapshot_keeps_the_sessions_permissions() {
    let handle = letter_fixture();

    let snapshot = output_snapshot(&handle).expect("the snapshot reopens");

    assert!(matches!(
        apply_edit(
            &snapshot,
            FfiEditCommand::AddHighlight {
                page: 0,
                rect: FfiRect {
                    x: 20.0,
                    y: 20.0,
                    width: 200.0,
                    height: 40.0,
                },
                color: FfiColor { r: 255, g: 0, b: 0 },
            },
        ),
        Err(FfiError::UnsupportedOperation { .. })
    ));
    assert!(export_page_image(&snapshot, 0, 72, FfiExportFormat::Png).is_ok());
}

fn add_highlight(handle: &pdf_ffi::DocumentHandle) {
    apply_edit(
        handle,
        FfiEditCommand::AddHighlight {
            page: 0,
            rect: FfiRect {
                x: 20.0,
                y: 20.0,
                width: 200.0,
                height: 40.0,
            },
            color: FfiColor { r: 255, g: 0, b: 0 },
        },
    )
    .expect("highlighting should succeed");
}

fn pixels(png: &[u8]) -> Vec<u8> {
    image::load_from_memory(png)
        .expect("must decode")
        .into_rgba8()
        .into_raw()
}
