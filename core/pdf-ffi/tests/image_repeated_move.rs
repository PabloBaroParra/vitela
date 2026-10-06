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
