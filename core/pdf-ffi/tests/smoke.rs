//! FFI smoke tests (T-043): handle lifecycle and error mapping, exercised
//! through this crate's real public functions in-process (the same pattern
//! the Batch 1 spike used — Rust-level tests validate the underlying logic;
//! actually running generated Swift/C#/Kotlin bindings happens in each
//! platform's own CI workflow, e.g. `macos.yml`, T-042).

use pdf_ffi::{
    apply_edit, create_blank_document, insert_image_stamp, open_from_bytes,
    open_with_passwords_from_bytes, redo, render_page, save_to_bytes, undo, FfiColor,
    FfiEditCommand, FfiError, FfiOrientation, FfiPageSize, FfiRect, FfiRenderOptions,
    FfiSaveIntent,
};

fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("encrypted")
        .join(name);
    std::fs::read(path).expect("fixture must be readable")
}

fn sample_png() -> Vec<u8> {
    use image::{ImageFormat, RgbaImage};
    use std::io::Cursor;

    let image = RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 200]));
    let mut buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut buf, ImageFormat::Png)
        .expect("encode sample png");
    buf.into_inner()
}

// ---------------------------------------------------------------------
// Handle lifecycle: open -> render -> read -> drop
// ---------------------------------------------------------------------

#[test]
fn page_dimensions_match_the_page_count_and_carry_point_sizes() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let handle = open_from_bytes(bytes, Some("user-rc4-pass".to_string()))
        .expect("should open with the correct user password");

    let dimensions = handle.page_dimensions();

    assert_eq!(dimensions.len() as u32, handle.page_count());
    assert!(dimensions
        .iter()
        .all(|page| page.width_pt > 0.0 && page.height_pt > 0.0));
}

#[test]
fn text_runs_expose_one_pdf_space_rectangle_per_character() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let handle = open_from_bytes(bytes, Some("user-rc4-pass".to_string())).unwrap();

    let runs = handle.text_runs(0).expect("text extraction should succeed");

    assert!(runs
        .iter()
        .all(|run| run.text.chars().count() == run.character_bounds.len()));
    assert!(runs
        .iter()
        .flat_map(|run| &run.character_bounds)
        .any(|bounds| bounds.height_pt > 0.0));
}

#[test]
fn search_returns_page_index_and_matching_pdf_space_geometry() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let handle = open_from_bytes(bytes, Some("user-rc4-pass".to_string())).unwrap();

    let results = handle
        .search("Fixture".to_string())
        .expect("search should succeed");

    let result = results.first().expect("known fixture text should match");
    assert_eq!(result.page_index, 0);
    assert_eq!(result.character_bounds.len(), 7);
}

#[test]
fn search_uses_render_side_page_indexes_until_structural_edits_are_saved() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let handle = open_from_bytes(bytes, Some("user-rc4-pass".to_string())).unwrap();
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: FfiPageSize::A4,
            orientation: FfiOrientation::Portrait,
        },
    )
    .expect("structural edit should succeed");

    let results = handle
        .search("Fixture".to_string())
        .expect("search should succeed");

    assert_eq!(
        results
            .first()
            .expect("known fixture text should match")
            .page_index,
        0
    );
}

#[test]
fn search_does_not_match_across_a_line_break() {
    // pdfium's text extraction emits a line separator between visually distinct
    // lines, so concatenating per-run text can't fabricate a match that spans a
    // line boundary: "alphabravo" is absent even though "alpha" and "bravo" are
    // consecutive lines. Guards the shell search that scans concatenated run
    // text (DocumentHandle::search).
    let mut doc = gen_fixtures::build_multi_line_page_document(&["alpha", "bravo", "charlie"]);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .expect("serialize multi-line fixture");
    let handle = open_from_bytes(bytes, None).unwrap();

    assert!(
        handle
            .search("alphabravo".to_string())
            .expect("search should succeed")
            .is_empty(),
        "a query spanning a line break must not match"
    );

    let single_line = handle
        .search("bravo".to_string())
        .expect("search should succeed");
    assert_eq!(
        single_line.len(),
        1,
        "a real single-line query should match once"
    );
    assert_eq!(single_line[0].page_index, 0);
}

#[test]
fn opens_encrypted_fixture_from_bytes_and_renders_page_one() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let handle = open_from_bytes(bytes, Some("user-rc4-pass".to_string()))
        .expect("should open with the correct user password");

    assert_eq!(handle.page_count(), 1);

    let bitmap =
        render_page(&handle, 0, 72, FfiRenderOptions::default()).expect("page 1 should render");
    assert!(bitmap.width().unwrap() > 0);
    assert!(bitmap.height().unwrap() > 0);
    let pixels = bitmap.get_pixels().expect("pixels should be readable");
    assert_eq!(
        pixels.len() as u32,
        bitmap.stride().unwrap() * bitmap.height().unwrap()
    );
}

#[test]
fn independent_bitmap_handles_do_not_interfere_when_one_is_dropped() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let handle = open_from_bytes(bytes, Some("user-rc4-pass".to_string())).unwrap();

    let first = render_page(&handle, 0, 72, FfiRenderOptions::default()).unwrap();
    let second = render_page(&handle, 0, 72, FfiRenderOptions::default()).unwrap();

    drop(first);

    // The second, independent handle must remain fully readable after the
    // first is dropped (spec "Bitmap Handle Lifecycle": drop releases only
    // its own registry entry, no memory leak, no cross-handle interference).
    assert!(!second.get_pixels().unwrap().is_empty());
}

#[test]
fn render_page_out_of_bounds_returns_typed_error() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let handle = open_from_bytes(bytes, Some("user-rc4-pass".to_string())).unwrap();

    let result = render_page(&handle, 99, 72, FfiRenderOptions::default());
    assert!(matches!(
        result,
        Err(FfiError::PageIndexOutOfBounds { index: 99 })
    ));
}

// ---------------------------------------------------------------------
// Error mapping: password handling on open
// ---------------------------------------------------------------------

#[test]
fn open_from_bytes_wrong_password_is_a_typed_error() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let result = open_from_bytes(bytes, Some("definitely-wrong".to_string()));
    assert!(matches!(result, Err(FfiError::WrongPassword)));
}

#[test]
fn open_from_bytes_missing_password_is_a_typed_error() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let result = open_from_bytes(bytes, None);
    assert!(matches!(result, Err(FfiError::PasswordRequired)));
}

// ---------------------------------------------------------------------
// Encrypted save flows (spec "Encrypted Document Save Behavior"): a
// single-password open supports incremental saves only; an encrypted FULL
// rewrite (forced by any structural page edit) requires the dual-password
// open so both PDF password roles can be re-applied.
// ---------------------------------------------------------------------

#[test]
fn single_password_open_cannot_full_rewrite_an_encrypted_document() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let handle = open_from_bytes(bytes, Some("user-rc4-pass".to_string())).unwrap();

    // A structural page edit forces the full-rewrite writer, which cannot
    // reconstruct the unknown owner password — must be a typed error, never
    // a silent security-policy change.
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: FfiPageSize::A4,
            orientation: FfiOrientation::Portrait,
        },
    )
    .unwrap();

    let result = save_to_bytes(&handle, FfiSaveIntent::Default);
    assert!(matches!(result, Err(FfiError::InvalidSaveRequest { .. })));
}

#[test]
fn dual_password_open_survives_structural_edit_and_stays_encrypted() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let handle = open_with_passwords_from_bytes(
        bytes,
        "user-rc4-pass".to_string(),
        "owner-rc4-pass".to_string(),
    )
    .expect("dual-password open should succeed");
    assert_eq!(handle.page_count(), 1);

    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: FfiPageSize::A4,
            orientation: FfiOrientation::Portrait,
        },
    )
    .unwrap();

    let saved = save_to_bytes(&handle, FfiSaveIntent::Default)
        .expect("encrypted full rewrite must succeed with both passwords");

    // Still encrypted after the rewrite, and both the structural edit and
    // the original user credential survive the round-trip.
    let reloaded = lopdf::Document::load_mem(&saved).expect("output must reload");
    assert!(reloaded.is_encrypted());
    let decrypted = lopdf::Document::load_mem_with_options(
        &saved,
        lopdf::LoadOptions::with_password("user-rc4-pass"),
    )
    .expect("re-encrypted output must open with the original user password");
    assert_eq!(decrypted.get_pages().len(), 2);
}

#[test]
fn open_with_passwords_from_bytes_rejects_a_wrong_owner_password() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let result = open_with_passwords_from_bytes(
        bytes,
        "user-rc4-pass".to_string(),
        "definitely-wrong".to_string(),
    );
    assert!(matches!(result, Err(FfiError::WrongPassword)));
}

// ---------------------------------------------------------------------
// create_blank_document -> apply_edit -> insert_image_stamp -> save_to_bytes
// ---------------------------------------------------------------------

#[test]
fn creates_blank_document_inserts_a_page_and_annotation_then_saves_a_reloadable_pdf() {
    let handle = create_blank_document(FfiPageSize::A4, FfiOrientation::Portrait)
        .expect("blank document creation should succeed");
    assert_eq!(handle.page_count(), 0);

    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: FfiPageSize::A4,
            orientation: FfiOrientation::Portrait,
        },
    )
    .expect("insert should succeed");
    assert_eq!(handle.page_count(), 1);

    apply_edit(
        &handle,
        FfiEditCommand::AddHighlight {
            page: 0,
            rect: FfiRect {
                x: 10.0,
                y: 10.0,
                width: 50.0,
                height: 20.0,
            },
            color: FfiColor { r: 255, g: 0, b: 0 },
        },
    )
    .expect("adding a highlight should succeed");

    insert_image_stamp(
        &handle,
        0,
        sample_png(),
        FfiRect {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
        },
    )
    .expect("inserting an image stamp should succeed");

    let bytes = save_to_bytes(&handle, FfiSaveIntent::Default).expect("save should succeed");
    let reloaded = lopdf::Document::load_mem(&bytes).expect("output must reload");
    assert_eq!(reloaded.get_pages().len(), 1);

    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let annots = reloaded
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Annots")
        .and_then(|o| o.as_array())
        .unwrap();
    assert_eq!(annots.len(), 2, "highlight + image stamp");
}

#[test]
fn insert_image_stamp_rejects_garbage_bytes_with_a_typed_error() {
    let handle = create_blank_document(FfiPageSize::A4, FfiOrientation::Portrait).unwrap();
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: FfiPageSize::A4,
            orientation: FfiOrientation::Portrait,
        },
    )
    .unwrap();

    let result = insert_image_stamp(
        &handle,
        0,
        b"not an image".to_vec(),
        FfiRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
    );
    assert!(matches!(result, Err(FfiError::InvalidImage { .. })));
}

// ---------------------------------------------------------------------
// Undo/redo (spec "Undo/Redo via EditLog")
// ---------------------------------------------------------------------

#[test]
fn undo_and_redo_round_trip_through_the_ffi() {
    let handle = create_blank_document(FfiPageSize::A4, FfiOrientation::Portrait).unwrap();
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: FfiPageSize::A4,
            orientation: FfiOrientation::Portrait,
        },
    )
    .unwrap();
    assert_eq!(handle.page_count(), 1);

    assert!(undo(&handle));
    assert_eq!(handle.page_count(), 0);

    assert!(redo(&handle));
    assert_eq!(handle.page_count(), 1);

    // Nothing left to undo/redo beyond the recorded history.
    assert!(undo(&handle));
    assert!(!undo(&handle));
    assert!(redo(&handle));
    assert!(!redo(&handle));
}

// ---------------------------------------------------------------------
// apply_edit error mapping: unknown annotation id
// ---------------------------------------------------------------------

#[test]
fn remove_annotation_with_unknown_id_returns_typed_error() {
    let handle = create_blank_document(FfiPageSize::A4, FfiOrientation::Portrait).unwrap();
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: FfiPageSize::A4,
            orientation: FfiOrientation::Portrait,
        },
    )
    .unwrap();

    let result = apply_edit(
        &handle,
        FfiEditCommand::RemoveAnnotation { annotation_id: 999 },
    );
    assert!(matches!(
        result,
        Err(FfiError::AnnotationNotFound { annotation_id: 999 })
    ));
}

#[test]
fn remove_page_with_out_of_bounds_index_returns_typed_error() {
    let handle = create_blank_document(FfiPageSize::A4, FfiOrientation::Portrait).unwrap();
    let result = apply_edit(&handle, FfiEditCommand::RemovePage { index: 5 });
    assert!(matches!(
        result,
        Err(FfiError::PageIndexOutOfBounds { index: 5 })
    ));
}
