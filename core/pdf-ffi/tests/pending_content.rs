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
    apply_edit, open_from_bytes, redo, save_to_bytes, undo, FfiContentTextRun, FfiEditCommand,
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

#[test]
fn deleting_a_retyped_run_saves_and_restores_the_original_in_one_undo_step() {
    let handle = open_single_line("Hello world");
    let original = only_run(&handle);
    retype(&handle, original.clone(), "Goodbye wide wide world");
    apply_edit(
        &handle,
        FfiEditCommand::RemoveTextRun {
            item: only_run(&handle),
        },
    )
    .expect("deleting a pending retype should amend its command");

    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save removal against the original snapshot");
    let reopened = open_from_bytes(saved, None).expect("reopen deletion");
    assert!(reopened.read_page_content(0).unwrap().text_runs.is_empty());

    assert!(undo(&handle));
    assert_eq!(only_run(&handle), original);
    assert!(
        !undo(&handle),
        "retype and deletion must share one undo step"
    );
    assert!(redo(&handle));
    assert!(handle.read_page_content(0).unwrap().text_runs.is_empty());
}

#[test]
fn deleting_a_retyped_run_does_not_remove_another_runs_pending_edit() {
    let mut document = gen_fixtures::build_multi_line_page_document(&["First line", "Second line"]);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    let handle = open_from_bytes(bytes, None).unwrap();
    let original = handle.read_page_content(0).unwrap().text_runs;
    retype(&handle, original[0].clone(), "Changed first");
    retype(&handle, original[1].clone(), "Changed second");
    let current = handle.read_page_content(0).unwrap().text_runs;
    apply_edit(
        &handle,
        FfiEditCommand::RemoveTextRun {
            item: current
                .into_iter()
                .find(|run| run.id == original[0].id)
                .unwrap(),
        },
    )
    .expect("amend the first run despite the later edit");
    assert_eq!(only_run(&handle).text, "Changed second");
    assert!(
        undo(&handle),
        "the later second-run edit stays the latest step"
    );
    assert_eq!(only_run(&handle).text, "Second line");
    assert!(undo(&handle));
    assert_eq!(handle.read_page_content(0).unwrap().text_runs, original);
    assert!(!undo(&handle));
}

fn insert(handle: &pdf_ffi::DocumentHandle, font: &str) -> FfiContentTextRun {
    let mut item = handle.read_page_content(0).unwrap().text_runs.remove(0);
    item.id = 0;
    item.resource_font_name = font.to_string();
    item.bbox.y = 40.0;
    item.text = "Inserted text".to_string();
    apply_edit(handle, FfiEditCommand::InsertTextRun { item }).unwrap();
    handle
        .read_page_content(0)
        .unwrap()
        .text_runs
        .into_iter()
        .find(|run| run.resource_font_name == font)
        .unwrap()
}

fn move_run(handle: &pdf_ffi::DocumentHandle, item: FfiContentTextRun, x: f64, y: f64) {
    let to = pdf_ffi::FfiRect { x, y, ..item.bbox };
    apply_edit(handle, FfiEditCommand::MoveTextRun { item, to }).expect("move text");
}

#[test]
fn repeated_moves_save_the_final_position_and_share_one_undo_step() {
    let handle = open_single_line("Original text");
    let original = only_run(&handle);
    move_run(&handle, original.clone(), 80.0, 90.0);
    move_run(&handle, only_run(&handle), 120.0, 140.0);
    let final_run = only_run(&handle);
    assert_eq!(final_run.bbox.x, 120.0);
    assert_eq!(final_run.bbox.y, 140.0);
    assert_eq!(final_run.text, original.text);
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save amended move");
    let reopened = open_from_bytes(saved, None).unwrap();
    let written = only_run(&reopened);
    assert!((written.bbox.x - 120.0).abs() < 0.01);
    assert!((written.bbox.y - 140.0).abs() < 0.01);
    assert!(undo(&handle));
    assert_eq!(only_run(&handle), original);
    assert!(!undo(&handle));
    assert!(redo(&handle));
    assert_eq!(only_run(&handle), final_run);
}

#[test]
fn repeated_move_does_not_consume_another_runs_later_edit() {
    let mut document = gen_fixtures::build_multi_line_page_document(&["First line", "Second line"]);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    let handle = open_from_bytes(bytes, None).unwrap();
    let original = handle.read_page_content(0).unwrap().text_runs;
    move_run(&handle, original[0].clone(), 80.0, 90.0);
    retype(&handle, original[1].clone(), "Changed second");
    let moved = handle.read_page_content(0).unwrap().text_runs.remove(0);
    move_run(&handle, moved, 120.0, 140.0);
    assert!(undo(&handle));
    let current = handle.read_page_content(0).unwrap().text_runs;
    assert_eq!(current[1], original[1]);
    assert_eq!(current[0].bbox.x, 120.0);
    assert!(undo(&handle));
    assert_eq!(handle.read_page_content(0).unwrap().text_runs, original);
    assert!(!undo(&handle));
}

#[test]
fn moving_after_a_retype_is_refused_without_changing_history() {
    let handle = open_single_line("Original text");
    let original = only_run(&handle);
    retype(&handle, only_run(&handle), "Changed text");
    let current = only_run(&handle);
    let to = pdf_ffi::FfiRect {
        x: 120.0,
        ..current.bbox
    };
    assert!(apply_edit(
        &handle,
        FfiEditCommand::MoveTextRun {
            item: current.clone(),
            to
        }
    )
    .is_err());
    assert_eq!(only_run(&handle), current);
    assert!(undo(&handle));
    assert_eq!(only_run(&handle), original);
    assert!(!undo(&handle));
}

#[test]
fn invalid_repeated_move_preserves_the_previous_destination_and_history() {
    let handle = open_single_line("Original text");
    move_run(&handle, only_run(&handle), 80.0, 90.0);
    let current = only_run(&handle);
    let to = pdf_ffi::FfiRect {
        x: f64::NAN,
        ..current.bbox
    };
    assert!(apply_edit(
        &handle,
        FfiEditCommand::MoveTextRun {
            item: current.clone(),
            to
        }
    )
    .is_err());
    assert_eq!(only_run(&handle), current);
    assert!(undo(&handle));
    assert!(!undo(&handle));
}

#[test]
fn retyping_an_insertion_keeps_its_identity_geometry_and_single_undo_step() {
    let handle = open_single_line("Original text");
    let original = only_run(&handle);
    let inserted = insert(&handle, "InsertedFont");
    retype(&handle, inserted.clone(), "Changed insertion");
    // An inline editor may keep the same target through successive pauses.
    retype(&handle, inserted.clone(), "Final insertion");
    let current = handle.read_page_content(0).unwrap().text_runs;
    assert_eq!(current[1].id, inserted.id);
    assert_eq!(current[1].text, "Final insertion");
    assert_eq!(current[1].bbox.x, inserted.bbox.x);
    assert_eq!(current[1].bbox.y, inserted.bbox.y);
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .unwrap();
    let reopened = open_from_bytes(saved, None).unwrap();
    let runs = reopened.read_page_content(0).unwrap().text_runs;
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[1].text, "Final insertion");
    assert!(undo(&handle));
    assert_eq!(only_run(&handle), original);
    assert!(!undo(&handle), "insertion and retyping must coalesce");
    assert!(redo(&handle));
    assert_eq!(handle.read_page_content(0).unwrap().text_runs, current);
}

#[test]
fn a_reused_insertion_index_does_not_accept_the_retired_targets_font() {
    let handle = open_single_line("Original text");
    let retired = insert(&handle, "FirstInsertedFont");
    assert!(undo(&handle));
    let current = insert(&handle, "SecondInsertedFont");
    assert_eq!(retired.id, current.id, "the log slot has been reused");
    assert!(apply_edit(
        &handle,
        FfiEditCommand::ReplaceTextRunContent {
            item: retired,
            after: "Wrong target".to_string(),
        },
    )
    .is_err());
    assert_eq!(handle.read_page_content(0).unwrap().text_runs[1], current);
    assert!(undo(&handle));
    assert!(!undo(&handle));
}

#[test]
fn moving_and_retyping_an_insertion_preserves_one_command_and_saves_final_geometry() {
    let handle = open_single_line("Original text");
    let original = only_run(&handle);
    let inserted = insert(&handle, "InsertedFont");
    retype(&handle, inserted.clone(), "Before movement");
    let target = handle.read_page_content(0).unwrap().text_runs.remove(1);
    move_run(&handle, target, 80.0, 90.0);
    let target = handle.read_page_content(0).unwrap().text_runs.remove(1);
    move_run(&handle, target, 120.0, 140.0);
    let target = handle.read_page_content(0).unwrap().text_runs.remove(1);
    retype(&handle, target, "After movement");
    let current = handle.read_page_content(0).unwrap().text_runs.remove(1);
    assert_eq!(current.id, inserted.id);
    assert_eq!(current.resource_font_name, inserted.resource_font_name);
    assert_eq!(current.bbox.height, inserted.bbox.height);
    assert_eq!(current.text, "After movement");
    assert_eq!((current.bbox.x, current.bbox.y), (120.0, 140.0));
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .unwrap();
    let reopened = open_from_bytes(saved, None).unwrap();
    let written = reopened.read_page_content(0).unwrap().text_runs.remove(1);
    assert_eq!(written.text, current.text);
    assert!((written.bbox.x - 120.0).abs() < 0.01);
    assert!((written.bbox.y - 140.0).abs() < 0.01);
    assert!(undo(&handle));
    assert_eq!(only_run(&handle), original);
    assert!(!undo(&handle));
    assert!(redo(&handle));
    assert_eq!(handle.read_page_content(0).unwrap().text_runs[1], current);
}

#[test]
fn moving_a_retired_insertion_target_cannot_redirect_a_reused_log_slot() {
    let handle = open_single_line("Original text");
    let retired = insert(&handle, "FirstInsertedFont");
    assert!(undo(&handle));
    let current = insert(&handle, "SecondInsertedFont");
    let to = pdf_ffi::FfiRect {
        x: 120.0,
        ..retired.bbox
    };
    assert!(apply_edit(&handle, FfiEditCommand::MoveTextRun { item: retired, to }).is_err());
    assert_eq!(handle.read_page_content(0).unwrap().text_runs[1], current);
    assert!(undo(&handle));
    assert!(!undo(&handle));
}

#[test]
fn moving_an_insertion_refuses_stale_placement_and_invalid_coordinates_without_history_changes() {
    let handle = open_single_line("Original text");
    let inserted = insert(&handle, "InsertedFont");
    move_run(&handle, inserted.clone(), 80.0, 90.0);
    let current = handle.read_page_content(0).unwrap().text_runs.remove(1);
    for (item, x) in [(inserted, 120.0), (current.clone(), f64::INFINITY)] {
        let to = pdf_ffi::FfiRect { x, ..item.bbox };
        assert!(apply_edit(&handle, FfiEditCommand::MoveTextRun { item, to }).is_err());
        assert_eq!(handle.read_page_content(0).unwrap().text_runs[1], current);
    }
    assert!(undo(&handle));
    assert!(!undo(&handle));
}

#[test]
fn a_refused_insertion_retype_preserves_the_text_and_its_history() {
    let handle = open_single_line("Original text");
    let inserted = insert(&handle, "InsertedFont");
    assert!(
        apply_edit(
            &handle,
            FfiEditCommand::ReplaceTextRunContent {
                item: inserted.clone(),
                after: "\u{65e5}\u{672c}\u{8a9e}".to_string(),
            },
        )
        .is_err(),
        "unencodable text must be refused before amendment"
    );
    assert_eq!(handle.read_page_content(0).unwrap().text_runs[1], inserted);
    assert!(undo(&handle));
    assert!(!undo(&handle));
}
