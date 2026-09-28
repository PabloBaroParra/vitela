//! `read_page_content` describes the page as the reader sees it — pending
//! edits included — not the bytes it was opened from.
//!
//! A retyped run is painted by the refreshed preview straight away, so a
//! shell that re-reads the page (after Organize, an undo, or any reset of its
//! own caches) has to get the new text and the box it now occupies back.
//! Reading the base bytes instead handed back the old text and the old,
//! shorter box: the words the reader had just added were not clickable, and
//! a second retype opened on text the page no longer showed.

use pdf_ffi::{
    apply_edit, open_from_bytes, save_to_bytes, undo, FfiContentTextRun, FfiEditCommand,
    FfiSaveIntent, FfiSignatureAcknowledgement,
};
use std::sync::Arc;

fn open_single_line(line: &str) -> Arc<pdf_ffi::DocumentHandle> {
    let mut doc = gen_fixtures::build_multi_line_page_document(&[line]);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("serialize fixture");
    open_from_bytes(bytes, None).expect("fixture should open")
}

fn only_run(handle: &pdf_ffi::DocumentHandle) -> FfiContentTextRun {
    let mut runs = handle.read_page_content(0).expect("read").text_runs;
    assert_eq!(runs.len(), 1, "{runs:?}");
    runs.remove(0)
}

fn retype(handle: &pdf_ffi::DocumentHandle, item: FfiContentTextRun, after: &str) {
    apply_edit(
        handle,
        FfiEditCommand::ReplaceTextRunContent {
            item,
            after: after.to_string(),
        },
    )
    .expect("retyping a standard-14 run should succeed");
}

#[test]
fn a_pending_retype_is_read_back_with_its_new_text_and_the_box_it_now_fills() {
    let handle = open_single_line("Hello world");
    let before = only_run(&handle);

    retype(&handle, before.clone(), "Hello wide wide world");

    let after = only_run(&handle);
    assert_eq!(after.text, "Hello wide wide world");
    assert_eq!(after.id, before.id, "a retyped run keeps its identity");
    assert_eq!(after.bbox.x, before.bbox.x);
    assert!(
        after.bbox.width > before.bbox.width * 1.5,
        "the box must cover the added words: {:?} vs {:?}",
        after.bbox,
        before.bbox
    );
}

#[test]
fn an_undone_retype_is_read_back_as_the_page_holds_it_again() {
    let handle = open_single_line("Hello world");
    let before = only_run(&handle);
    retype(&handle, before.clone(), "Goodbye world");

    assert!(undo(&handle));

    assert_eq!(only_run(&handle), before);
}

#[test]
fn retyping_the_run_as_read_back_amends_the_edit_and_saves() {
    // What a shell holds after re-reading is the run as it now reads, not the
    // snapshot the queued command was built from. Folding it in has to keep
    // that original snapshot, or the save would look for text the file never
    // had.
    let handle = open_single_line("Hello world");
    retype(&handle, only_run(&handle), "Goodbye world");

    retype(&handle, only_run(&handle), "Goodbye moon");

    assert_eq!(only_run(&handle).text, "Goodbye moon");
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("a save after retyping the re-read run must succeed");
    let reopened = open_from_bytes(saved, None).expect("reopen");
    assert_eq!(only_run(&reopened).text, "Goodbye moon");
    assert!(undo(&handle));
    assert!(!undo(&handle), "both retypes must stay one undo step");
}
