//! Vector paths through the FFI (B25 T-206): read, move, resize, remove,
//! undo — and what a reopened save says about each.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_ffi::{
    apply_edit, open_from_bytes, refresh_preview, save_to_bytes, undo, FfiContentPathItem,
    FfiEditCommand, FfiPathPaint, FfiRect, FfiSaveIntent, FfiSignatureAcknowledgement,
};

/// A page shaped like the reference quote's client box: a stroked frame,
/// a hairline rule under it, and a clip that must not show up as an item.
fn table_page() -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let content = b"0.5 w 319 135 251 125 re S\n\
                    319 120 m 570 120 l S\n\
                    q 0 0 50 50 re W n Q"
        .to_vec();
    let content_id = document.add_object(Stream::new(dictionary! {}, content));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("fixture serialization");
    bytes
}

fn saved(handle: &pdf_ffi::DocumentHandle) -> Vec<u8> {
    save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save")
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> FfiRect {
    FfiRect {
        x,
        y,
        width,
        height,
    }
}

fn close(left: &FfiRect, right: &FfiRect) -> bool {
    (left.x - right.x).abs() < 1e-6
        && (left.y - right.y).abs() < 1e-6
        && (left.width - right.width).abs() < 1e-6
        && (left.height - right.height).abs() < 1e-6
}

fn frame(paths: &[FfiContentPathItem]) -> FfiContentPathItem {
    paths
        .iter()
        .find(|path| path.bbox.height > 0.0)
        .cloned()
        .expect("the frame")
}

#[test]
fn the_frame_and_the_rule_are_paths_and_the_clip_is_not() {
    let handle = open_from_bytes(table_page(), None).expect("open");

    let paths = handle.read_page_paths(0).expect("paths");

    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|path| path.paint == FfiPathPaint::Stroke));
    assert!(close(
        &frame(&paths).bbox,
        &rect(319.0, 135.0, 251.0, 125.0)
    ));
}

#[test]
fn a_pending_move_is_what_the_next_read_reports_and_what_the_save_writes() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let target = frame(&handle.read_page_paths(0).expect("paths"));

    apply_edit(
        &handle,
        FfiEditCommand::MovePath {
            item: target,
            to: rect(20.0, 600.0, 0.0, 0.0),
        },
    )
    .expect("move");
    refresh_preview(&handle).expect("preview");

    let expected = rect(20.0, 600.0, 251.0, 125.0);
    assert!(close(
        &frame(&handle.read_page_paths(0).expect("pending")).bbox,
        &expected
    ));
    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert!(close(
        &frame(&reopened.read_page_paths(0).expect("saved")).bbox,
        &expected
    ));
}

#[test]
fn a_resize_then_a_removal_of_the_rule_save_together() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let paths = handle.read_page_paths(0).expect("paths");
    let rule = paths
        .iter()
        .find(|path| path.bbox.height == 0.0)
        .cloned()
        .expect("the rule");

    apply_edit(
        &handle,
        FfiEditCommand::ResizePath {
            item: frame(&paths),
            to: rect(300.0, 100.0, 280.0, 200.0),
        },
    )
    .expect("resize");
    apply_edit(&handle, FfiEditCommand::RemovePath { item: rule }).expect("remove");

    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    let saved_paths = reopened.read_page_paths(0).expect("saved");
    assert_eq!(saved_paths.len(), 1);
    assert!(close(
        &saved_paths[0].bbox,
        &rect(300.0, 100.0, 280.0, 200.0)
    ));
}

/// Undo pops the removal; the inverse is never written, so the save carries
/// the page untouched.
#[test]
fn undoing_a_removal_saves_the_path_back() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let target = frame(&handle.read_page_paths(0).expect("paths"));

    apply_edit(&handle, FfiEditCommand::RemovePath { item: target }).expect("remove");
    assert_eq!(handle.read_page_paths(0).expect("pending").len(), 1);
    assert!(undo(&handle));

    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert_eq!(reopened.read_page_paths(0).expect("saved").len(), 2);
}

/// What the Windows canvas does: drag the frame, then grab a corner right
/// away. The second edit is built from the pending read, which the untouched
/// bytes do not contain, so it folds into the first.
#[test]
fn a_resize_right_after_a_move_folds_into_one_edit_that_saves() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let original = frame(&handle.read_page_paths(0).expect("paths"));

    apply_edit(
        &handle,
        FfiEditCommand::MovePath {
            item: original,
            to: rect(339.0, 115.0, 0.0, 0.0),
        },
    )
    .expect("move");
    let moved = frame(&handle.read_page_paths(0).expect("pending"));
    apply_edit(
        &handle,
        FfiEditCommand::ResizePath {
            item: moved.clone(),
            to: rect(339.0, 115.0, 271.0, 145.0),
        },
    )
    .expect("resize after a move");
    let resized = frame(&handle.read_page_paths(0).expect("pending"));
    apply_edit(
        &handle,
        FfiEditCommand::MovePath {
            item: resized,
            to: rect(10.0, 10.0, 0.0, 0.0),
        },
    )
    .expect("move after a resize");

    let expected = rect(10.0, 10.0, 271.0, 145.0);
    assert!(close(
        &frame(&handle.read_page_paths(0).expect("pending")).bbox,
        &expected
    ));
    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert!(close(
        &frame(&reopened.read_page_paths(0).expect("saved")).bbox,
        &expected
    ));
    assert!(undo(&handle), "the folded edit is one undo step");
    assert!(close(
        &frame(&handle.read_page_paths(0).expect("undone")).bbox,
        &rect(319.0, 135.0, 251.0, 125.0)
    ));
}

#[test]
fn removing_a_moved_path_removes_it() {
    let handle = open_from_bytes(table_page(), None).expect("open");
    let original = frame(&handle.read_page_paths(0).expect("paths"));
    apply_edit(
        &handle,
        FfiEditCommand::MovePath {
            item: original,
            to: rect(20.0, 600.0, 0.0, 0.0),
        },
    )
    .expect("move");
    let moved = frame(&handle.read_page_paths(0).expect("pending"));

    apply_edit(&handle, FfiEditCommand::RemovePath { item: moved }).expect("remove");

    assert_eq!(handle.read_page_paths(0).expect("pending").len(), 1);
    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert_eq!(reopened.read_page_paths(0).expect("saved").len(), 1);
}
