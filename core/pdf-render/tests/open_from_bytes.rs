//! Real-pdfium integration test (T-068 DELTA, TDD): bytes-based document
//! open — the render-side half of the canonical cross-platform
//! `open_from_bytes` contract `pdf-ffi` (Batch 7) wires up. Android (SAF) and
//! iOS (security-scoped bookmarks) shells only ever have an in-memory byte
//! buffer, never a filesystem path pdfium can open directly.

use pdf_render::{PdfiumRenderer, Priority, RenderOptions};

fn small_fixture_bytes() -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_page_document(3, "pdf-render-bytes-open");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("serialize fixture to bytes");
    bytes
}

#[test]
fn opens_document_from_bytes_and_renders_page_one() {
    let bytes = small_fixture_bytes();
    let renderer = PdfiumRenderer::new();

    let doc = renderer
        .open_document_from_bytes(bytes, None)
        .expect("open fixture from bytes");
    let bitmap = renderer
        .render_page(
            doc,
            0,
            150,
            None,
            RenderOptions::default(),
            Priority::Visible,
        )
        .wait()
        .expect("render page 1");

    // US Letter (612x792pt) at 150 DPI: 612/72*150 = 1275, 792/72*150 = 1650.
    assert_eq!(bitmap.width().unwrap(), 1275);
    assert_eq!(bitmap.height().unwrap(), 1650);
}

#[test]
fn opens_encrypted_document_from_bytes_with_correct_password() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("fixtures")
            .join("encrypted")
            .join("rc4_128_user_and_owner.pdf"),
    )
    .expect("fixture must be readable");
    let renderer = PdfiumRenderer::new();

    let doc = renderer
        .open_document_from_bytes(bytes, Some("user-rc4-pass"))
        .expect("open encrypted fixture from bytes with correct password");
    let bitmap = renderer
        .render_page(
            doc,
            0,
            72,
            None,
            RenderOptions::default(),
            Priority::Visible,
        )
        .wait()
        .expect("render page 1");
    assert!(bitmap.width().unwrap() > 0);
}

#[test]
fn opening_from_bytes_with_wrong_password_is_a_clean_error() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("fixtures")
            .join("encrypted")
            .join("rc4_128_user_and_owner.pdf"),
    )
    .expect("fixture must be readable");
    let renderer = PdfiumRenderer::new();

    let result = renderer.open_document_from_bytes(bytes, Some("wrong-password"));
    assert!(matches!(
        result,
        Err(pdf_render::RenderError::InvalidPassword)
    ));
}
