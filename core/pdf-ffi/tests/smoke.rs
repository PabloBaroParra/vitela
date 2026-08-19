//! FFI smoke tests (T-043): handle lifecycle and error mapping, exercised
//! through this crate's real public functions in-process (the same pattern
//! the Batch 1 spike used — Rust-level tests validate the underlying logic;
//! actually running generated Swift/C#/Kotlin bindings happens in each
//! platform's own CI workflow, e.g. `macos.yml`, T-042).

use pdf_ffi::{
    apply_edit, create_blank_document, create_document_with_blank_page, insert_image_stamp,
    open_from_bytes, open_with_passwords_from_bytes, redo, render_page, save_to_bytes,
    stamp_placement, undo, FfiColor, FfiEditCommand, FfiError, FfiFontKind, FfiOrientation,
    FfiPageSize, FfiRect, FfiRenderOptions, FfiSaveIntent, FfiSignatureAcknowledgement,
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
fn page_characters_support_caret_hit_testing_and_selection_text() {
    let handle = open_single_line_fixture("Hello world");

    let characters = handle
        .page_characters(0)
        .expect("text extraction should succeed");

    assert!(!characters.is_empty());
    assert_eq!(characters.len(), "Hello world".chars().count() as u32);
    // Far left of the 700pt baseline lands before the first character; far
    // right lands after the last — same line-then-column resolution
    // `PageCharacters::caret_at` is tested against directly in `pdf-render`.
    assert_eq!(characters.caret_at(-1_000.0, 700.0), Some(0));
    assert_eq!(characters.caret_at(1_000.0, 700.0), Some(characters.len()));

    let anchor = characters.caret_at(-1_000.0, 700.0).unwrap();
    let focus = characters.caret_at(1_000.0, 700.0).unwrap();
    assert_eq!(characters.text_in(anchor, focus), "Hello world");
    // A backwards drag (focus before anchor) must report the same text.
    assert_eq!(characters.text_in(focus, anchor), "Hello world");

    let rects = characters.rects_in(anchor, focus);
    assert_eq!(rects.len(), 1, "a single-line selection is one bar");
    assert!(rects[0].width_pt > 0.0 && rects[0].height_pt > 0.0);
}

#[test]
fn page_characters_is_denied_when_the_document_forbids_text_extraction() {
    let bytes = restricted_single_line_pdf("Hello world", "user-no-copy", "owner-no-copy");
    let handle = open_from_bytes(bytes, Some("user-no-copy".to_string()))
        .expect("should open with the correct user password");

    let result = handle.page_characters(0);

    assert!(
        matches!(result, Err(FfiError::UnsupportedOperation { .. })),
        "a /P without the copy bit must deny page_characters, same as text_runs/search"
    );
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
fn open_from_bytes_with_valid_password_returns_a_render_capable_document() {
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

    let result = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    );
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

    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
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

    let bytes = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save should succeed");
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

#[test]
fn stamp_placement_keeps_the_image_proportions_and_hangs_below_the_anchor() {
    use image::{ImageFormat, RgbImage};
    use std::io::Cursor;

    let wide = RgbImage::from_pixel(400, 100, image::Rgb([10, 20, 30]));
    let mut buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(wide)
        .write_to(&mut buf, ImageFormat::Png)
        .expect("encode wide png");

    let rect = stamp_placement(buf.into_inner(), 200.0, 500.0).expect("placement should succeed");

    // 4:1 in, 4:1 out — never the fixed box the shells used to pass in.
    assert_eq!((rect.width, rect.height), (144.0, 36.0));
    assert_eq!((rect.x, rect.y), (200.0, 500.0 - 36.0));
}

#[test]
fn stamp_placement_rejects_garbage_bytes_with_a_typed_error() {
    let result = stamp_placement(b"not an image".to_vec(), 0.0, 0.0);

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

// ---------------------------------------------------------------------
// Page-content editing (Batch 21, T-158): read_page_content + the nine
// content Command variants through apply_edit.
// ---------------------------------------------------------------------

fn open_single_line_fixture(line: &str) -> std::sync::Arc<pdf_ffi::DocumentHandle> {
    let mut doc = gen_fixtures::build_multi_line_page_document(&[line]);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .expect("serialize single-line fixture");
    open_from_bytes(bytes, None).expect("fixture should open")
}

/// Builds a single-line AES-128 PDF whose `/P` grants printing but **not**
/// copying — same shape `pdf-manip`'s `text_extraction_permission.rs` uses to
/// exercise the gate, built locally here since that support module is
/// private to its own crate.
fn restricted_single_line_pdf(line: &str, user_password: &str, owner_password: &str) -> Vec<u8> {
    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
    use lopdf::xref::XrefType;
    use std::collections::BTreeMap;
    use std::sync::Arc as StdArc;

    let mut doc = gen_fixtures::build_multi_line_page_document(&[line]);
    // Classic xref table: lopdf cannot re-hydrate objects out of an encrypted
    // cross-reference stream at load time.
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = lopdf::Object::string_literal("read-page-content-permission-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);

    let crypt_filter: StdArc<dyn CryptFilter> = StdArc::new(Aes128CryptFilter);
    let version = lopdf::EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password,
        user_password,
        permissions: lopdf::Permissions::PRINTABLE,
    };
    let state = lopdf::EncryptionState::try_from(version).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

#[test]
fn read_page_content_is_denied_when_the_document_forbids_text_extraction() {
    let bytes = restricted_single_line_pdf("Hello world", "user-no-copy", "owner-no-copy");
    let handle = open_from_bytes(bytes, Some("user-no-copy".to_string()))
        .expect("should open with the correct user password");

    let result = handle.read_page_content(0);

    assert!(
        matches!(result, Err(FfiError::UnsupportedOperation { .. })),
        "a /P without the copy bit must deny read_page_content, same as text_runs/search"
    );
}

#[test]
fn read_page_content_finds_the_standard14_run_written_by_the_fixture() {
    let handle = open_single_line_fixture("Hello world");

    let content = handle
        .read_page_content(0)
        .expect("page content should parse");

    assert!(content.images.is_empty());
    let run = content
        .text_runs
        .first()
        .expect("fixture wrote one text run");
    assert_eq!(run.text, "Hello world");
    assert_eq!(run.font_kind, FfiFontKind::Standard14);
}

#[test]
fn editing_a_standard14_run_then_saving_and_reopening_shows_the_new_text() {
    let handle = open_single_line_fixture("Hello world");
    let content = handle.read_page_content(0).unwrap();
    let run = content.text_runs.first().unwrap().clone();

    apply_edit(
        &handle,
        FfiEditCommand::ReplaceTextRunContent {
            item: run,
            after: "Goodbye world".to_string(),
        },
    )
    .expect("replacing standard-14 text should succeed");

    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save should succeed");

    let reopened = open_from_bytes(saved, None).expect("saved bytes should reopen");
    let reread = reopened
        .read_page_content(0)
        .expect("page content should parse after save");

    assert_eq!(reread.text_runs.first().unwrap().text, "Goodbye world");
}

#[test]
fn replace_text_run_content_with_stale_item_fails_at_save() {
    // Per Batch 21 decision 5, `apply_edit` on a content command only
    // records the log entry — `pdf-edit` resolves the targeted item against
    // the real content stream during `save_to_bytes`'s replay, so a stale
    // item is accepted here and rejected there.
    let handle = open_single_line_fixture("Hello world");
    let content = handle.read_page_content(0).unwrap();
    let mut stale = content.text_runs.first().unwrap().clone();
    // A different revision than what's actually on the page: the parser
    // never assigned this id/text/position combination.
    stale.text = "not what is on the page".to_string();

    apply_edit(
        &handle,
        FfiEditCommand::ReplaceTextRunContent {
            item: stale,
            after: "irrelevant".to_string(),
        },
    )
    .expect("apply_edit only records the command, it does not resolve the item yet");

    let result = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    );

    assert!(matches!(result, Err(FfiError::Internal { .. })));
}

// ---------------------------------------------------------------------
// create_document_with_blank_page (T-063)
// ---------------------------------------------------------------------

#[test]
fn creates_a_document_that_already_has_one_renderable_page() {
    let handle = create_document_with_blank_page(FfiPageSize::A4, FfiOrientation::Portrait)
        .expect("one-page document creation should succeed");

    assert_eq!(handle.page_count(), 1);

    let dimensions = handle.page_dimensions();
    assert_eq!(dimensions.len(), 1);
    assert!((dimensions[0].width_pt - 595.0).abs() < 0.5);
    assert!((dimensions[0].height_pt - 842.0).abs() < 0.5);

    // A zero-page document comes back with no render handle at all (pdfium
    // cannot open one); the whole point of this entrypoint is that a reader
    // can actually draw what it hands back.
    render_page(&handle, 0, 72, FfiRenderOptions::default()).expect("the first page should render");
}

#[test]
fn the_created_document_starts_with_no_pending_edits() {
    let handle = create_document_with_blank_page(FfiPageSize::A4, FfiOrientation::Portrait)
        .expect("one-page document creation should succeed");

    // Routing the first page through `apply_edit` would leave an undoable
    // command queued, so a shell's unsaved-work guard would fire on a
    // document nobody has touched yet. The page has to be there before the
    // handle exists.
    assert!(!undo(&handle));
}
