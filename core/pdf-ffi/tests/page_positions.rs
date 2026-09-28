//! Every `page` crossing this boundary is a **position** — the index a shell
//! renders, scrolls to and draws on — never a `PageId`.
//!
//! The two coincide on a freshly opened file (`populate_document` hands out
//! `PageId(0..n)`), which is how the boundary got away with conflating them.
//! These tests reorder pages first, so that a `page` read as an id lands on
//! the wrong page and fails.

use pdf_ffi::{
    apply_edit, create_document_with_blank_page, insert_image_stamp, open_from_bytes,
    save_to_bytes, undo, DocumentHandle, FfiColor, FfiEditCommand, FfiError, FfiOrientation,
    FfiPageSize, FfiRect, FfiSaveIntent, FfiSignatureAcknowledgement, FfiTextStyle,
};
use std::sync::Arc;

const A4: (f64, f64) = (595.0, 842.0);
const LETTER: (f64, f64) = (612.0, 792.0);
const A4_LANDSCAPE: (f64, f64) = (842.0, 595.0);

/// Three pages whose sizes tell them apart — A4, Letter, A4 landscape —
/// saved and reopened so that all three are base pages of the opened file,
/// the case a real document is.
fn three_distinct_pages() -> Arc<DocumentHandle> {
    let handle = create_document_with_blank_page(FfiPageSize::A4, FfiOrientation::Portrait)
        .expect("blank document");
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 1,
            size: FfiPageSize::Letter,
            orientation: FfiOrientation::Portrait,
        },
    )
    .expect("insert letter");
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 2,
            size: FfiPageSize::A4,
            orientation: FfiOrientation::Landscape,
        },
    )
    .expect("insert landscape");
    let bytes = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");
    open_from_bytes(bytes, None).expect("reopen")
}

fn sizes(handle: &DocumentHandle) -> Vec<(f64, f64)> {
    handle
        .page_dimensions()
        .into_iter()
        .map(|page| (page.width_pt.round(), page.height_pt.round()))
        .collect()
}

fn move_page(handle: &DocumentHandle, from: u32, to: u32) {
    apply_edit(handle, FfiEditCommand::MovePages { from, count: 1, to }).expect("move");
}

fn rect() -> FfiRect {
    FfiRect {
        x: 72.0,
        y: 72.0,
        width: 100.0,
        height: 20.0,
    }
}

fn red() -> FfiColor {
    FfiColor { r: 255, g: 0, b: 0 }
}

#[test]
fn the_fixture_opens_with_three_distinguishable_pages() {
    let handle = three_distinct_pages();

    assert_eq!(sizes(&handle), vec![A4, LETTER, A4_LANDSCAPE]);
}

#[test]
fn moving_a_page_reorders_the_dimensions_a_shell_lays_out() {
    let handle = three_distinct_pages();

    move_page(&handle, 0, 2);

    assert_eq!(sizes(&handle), vec![LETTER, A4_LANDSCAPE, A4]);
}

#[test]
fn moving_a_block_keeps_its_internal_order() {
    let handle = three_distinct_pages();

    apply_edit(
        &handle,
        FfiEditCommand::MovePages {
            from: 1,
            count: 2,
            to: 0,
        },
    )
    .expect("move block");

    assert_eq!(sizes(&handle), vec![LETTER, A4_LANDSCAPE, A4]);
}

#[test]
fn a_move_is_one_undoable_step() {
    let handle = three_distinct_pages();
    move_page(&handle, 0, 2);

    assert!(undo(&handle));

    assert_eq!(sizes(&handle), vec![A4, LETTER, A4_LANDSCAPE]);
}

#[test]
fn a_move_past_the_last_page_is_refused_by_position() {
    let handle = three_distinct_pages();

    let error = apply_edit(
        &handle,
        FfiEditCommand::MovePages {
            from: 2,
            count: 2,
            to: 0,
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, FfiError::PageIndexOutOfBounds { index: 2 }),
        "{error:?}"
    );
    assert_eq!(sizes(&handle), vec![A4, LETTER, A4_LANDSCAPE]);
}

#[test]
fn a_move_to_a_position_the_block_cannot_start_at_is_refused() {
    let handle = three_distinct_pages();

    let error = apply_edit(
        &handle,
        FfiEditCommand::MovePages {
            from: 0,
            count: 2,
            to: 2,
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, FfiError::PageIndexOutOfBounds { index: 2 }),
        "{error:?}"
    );
}

#[test]
fn rotating_after_a_move_turns_the_page_at_that_position() {
    let handle = three_distinct_pages();
    move_page(&handle, 0, 2);

    apply_edit(
        &handle,
        FfiEditCommand::RotatePage {
            page: 0,
            delta_degrees: 90,
        },
    )
    .expect("rotate");

    // The Letter page now sits first and is the one on its side.
    assert_eq!(sizes(&handle), vec![(792.0, 612.0), A4_LANDSCAPE, A4]);
}

#[test]
fn a_rotation_past_the_last_page_is_refused() {
    let handle = three_distinct_pages();

    let error = apply_edit(
        &handle,
        FfiEditCommand::RotatePage {
            page: 3,
            delta_degrees: 90,
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, FfiError::PageIndexOutOfBounds { index: 3 }),
        "{error:?}"
    );
}

#[test]
fn an_inserted_blank_page_shows_up_in_the_dimensions() {
    let handle = three_distinct_pages();

    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: FfiPageSize::Letter,
            orientation: FfiOrientation::Landscape,
        },
    )
    .expect("insert");

    assert_eq!(
        sizes(&handle),
        vec![(792.0, 612.0), A4, LETTER, A4_LANDSCAPE]
    );
}

#[test]
fn an_annotation_drawn_after_a_move_stays_on_the_page_it_was_drawn_on() {
    let handle = three_distinct_pages();
    move_page(&handle, 0, 2);

    // Drawn on position 0 — the Letter page, since the move.
    apply_edit(
        &handle,
        FfiEditCommand::AddHighlight {
            page: 0,
            rect: rect(),
            color: red(),
        },
    )
    .expect("highlight");
    assert_eq!(handle.annotations()[0].page, 0);

    // Send the Letter page to the end: the highlight goes with it.
    move_page(&handle, 0, 2);
    assert_eq!(sizes(&handle)[2], LETTER);
    assert_eq!(handle.annotations()[0].page, 2);
}

#[test]
fn an_image_stamp_after_a_move_stays_on_the_page_it_was_placed_on() {
    let handle = three_distinct_pages();
    move_page(&handle, 0, 2);

    insert_image_stamp(&handle, 0, sample_png(), rect()).expect("stamp");
    move_page(&handle, 0, 2);

    assert_eq!(handle.annotations()[0].page, 2);
}

#[test]
fn a_form_field_placed_after_a_move_stays_on_the_page_it_was_placed_on() {
    let handle = three_distinct_pages();
    move_page(&handle, 0, 2);

    apply_edit(
        &handle,
        FfiEditCommand::AddTextField {
            page: 0,
            rect: rect(),
            style: FfiTextStyle {
                font: pdf_ffi::FfiFontFamily::Helvetica,
                size_pt: 12.0,
                color: red(),
            },
            multiline: false,
            max_len: None,
        },
    )
    .expect("field");
    assert_eq!(handle.list_form_fields()[0].page, 0);

    move_page(&handle, 0, 2);
    assert_eq!(handle.list_form_fields()[0].page, 2);
}

#[test]
fn a_move_survives_save_and_reopen() {
    let handle = three_distinct_pages();
    move_page(&handle, 0, 2);

    let bytes = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");
    let reopened = open_from_bytes(bytes, None).expect("reopen");

    assert_eq!(sizes(&reopened), vec![LETTER, A4_LANDSCAPE, A4]);
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

#[test]
fn page_content_is_read_and_edited_by_position_after_an_insertion() {
    let mut fixture = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
    let mut bytes = Vec::new();
    fixture.save_to(&mut bytes).expect("serialize fixture");
    let handle = open_from_bytes(bytes, None).expect("open fixture");

    // A blank page in front pushes the text page to position 1 — and the
    // blank page is minted id 1, so reading `1` as an id finds nothing.
    apply_edit(
        &handle,
        FfiEditCommand::InsertBlankPage {
            index: 0,
            size: FfiPageSize::A4,
            orientation: FfiOrientation::Portrait,
        },
    )
    .expect("insert");

    let content = handle.read_page_content(1).expect("read by position");
    let run = content
        .text_runs
        .first()
        .expect("the text page's run")
        .clone();
    assert_eq!(run.text, "Hello world");
    assert_eq!(run.page, 1);

    apply_edit(
        &handle,
        FfiEditCommand::ReplaceTextRunContent {
            item: run,
            after: "Hello there".to_string(),
        },
    )
    .expect("retype the run it was handed");
}
