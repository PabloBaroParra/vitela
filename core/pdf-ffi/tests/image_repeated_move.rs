//! Moving an image again before saving amends its queued move.
//!
//! What a shell holds after a move is the image as `read_page_content` reads it
//! now — already at its new place. Sent as a second `MoveImage`, that snapshot
//! matched nothing in the bytes the save replays against, so the move was
//! refused and the image stayed put. A repeated move has to fold into the
//! queued one, keeping its original snapshot, like a text run's.

use gen_fixtures::content_edit as gen_fixtures;
use pdf_ffi::{
    apply_edit, open_from_bytes, redo, save_to_bytes, undo, FfiContentImageItem, FfiEditCommand,
    FfiRect, FfiSaveIntent, FfiSignatureAcknowledgement,
};
use std::sync::Arc;

fn open_image_page() -> Arc<pdf_ffi::DocumentHandle> {
    let mut document = gen_fixtures::build_image_page_document();
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("fixture serialization");
    open_from_bytes(bytes, None).expect("fixture should open")
}

fn only_image(handle: &pdf_ffi::DocumentHandle) -> FfiContentImageItem {
    let mut images = handle.read_page_content(0).expect("read").images;
    assert_eq!(images.len(), 1, "{images:?}");
    images.remove(0)
}

fn move_image(handle: &pdf_ffi::DocumentHandle, item: FfiContentImageItem, x: f64, y: f64) {
    let to = FfiRect { x, y, ..item.bbox };
    apply_edit(handle, FfiEditCommand::MoveImage { item, to }).expect("move image");
}

#[test]
fn repeated_image_moves_save_the_final_position_and_share_one_undo_step() {
    let handle = open_image_page();
    let original = only_image(&handle);

    move_image(&handle, original.clone(), 80.0, 90.0);
    move_image(&handle, only_image(&handle), 120.0, 140.0);

    let moved = only_image(&handle);
    assert_eq!((moved.bbox.x, moved.bbox.y), (120.0, 140.0));
    assert_eq!(
        (moved.bbox.width, moved.bbox.height),
        (original.bbox.width, original.bbox.height)
    );
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save the amended move");
    let written = only_image(&open_from_bytes(saved, None).expect("reopen"));
    assert!((written.bbox.x - 120.0).abs() < 0.01, "{:?}", written.bbox);
    assert!((written.bbox.y - 140.0).abs() < 0.01, "{:?}", written.bbox);
    assert!(undo(&handle));
    assert_eq!(only_image(&handle), original);
    assert!(!undo(&handle), "both moves must stay one undo step");
    assert!(redo(&handle));
    assert_eq!(only_image(&handle), moved);
}

#[test]
fn an_invalid_repeated_image_move_keeps_the_previous_destination_and_history() {
    let handle = open_image_page();
    move_image(&handle, only_image(&handle), 80.0, 90.0);
    let current = only_image(&handle);

    let to = FfiRect {
        x: f64::NAN,
        ..current.bbox
    };
    assert!(apply_edit(
        &handle,
        FfiEditCommand::MoveImage {
            item: current.clone(),
            to
        }
    )
    .is_err());

    assert_eq!(only_image(&handle), current);
    assert!(undo(&handle));
    assert!(!undo(&handle));
}

fn resize_image(handle: &pdf_ffi::DocumentHandle, item: FfiContentImageItem, to: FfiRect) {
    apply_edit(handle, FfiEditCommand::ResizeImage { item, to }).expect("resize image");
}

/// The Windows canvas records every drag as a resize. Dragging again before a
/// save — a move, then a corner — has to fold the same way two moves do.
#[test]
fn a_resize_after_a_resize_saves_the_final_box_in_one_undo_step() {
    let handle = open_image_page();
    let original = only_image(&handle);

    let first = FfiRect {
        x: 80.0,
        y: 90.0,
        ..original.bbox
    };
    resize_image(&handle, original.clone(), first);
    let last = FfiRect {
        x: 100.0,
        y: 110.0,
        width: 60.0,
        height: 40.0,
    };
    resize_image(&handle, only_image(&handle), last);

    let written = only_image(
        &open_from_bytes(
            save_to_bytes(
                &handle,
                FfiSaveIntent::Default,
                FfiSignatureAcknowledgement::Unacknowledged,
            )
            .expect("save the folded resize"),
            None,
        )
        .expect("reopen"),
    );
    for (got, want) in [
        (written.bbox.x, last.x),
        (written.bbox.y, last.y),
        (written.bbox.width, last.width),
        (written.bbox.height, last.height),
    ] {
        assert!((got - want).abs() < 0.01, "{:?}", written.bbox);
    }
    assert!(undo(&handle));
    assert_eq!(only_image(&handle), original);
    assert!(!undo(&handle), "both drags are one undo step");
}

#[test]
fn a_moved_image_can_be_deleted_before_a_save() {
    let handle = open_image_page();
    move_image(&handle, only_image(&handle), 80.0, 90.0);

    apply_edit(
        &handle,
        FfiEditCommand::RemoveImage {
            item: only_image(&handle),
            source: None,
        },
    )
    .expect("remove folds into the queued move");

    assert!(handle.read_page_content(0).expect("read").images.is_empty());
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");
    let reopened = open_from_bytes(saved, None).expect("reopen");
    assert!(reopened
        .read_page_content(0)
        .expect("read")
        .images
        .is_empty());
}
