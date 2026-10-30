//! A second, different edit of an item that already has an unsaved edit no
//! longer asks for a save and reopen.
//!
//! The queued edits are checked against the file as it was opened, so an
//! item an earlier edit changed is not there to be found. Before this, the
//! only way out was the reader saving and reopening the file by hand. The
//! core now does the same thing in memory, a **checkpoint**: it writes the
//! queued edits into a new base and empties the queue, then retries the edit
//! against that base. Undo walks back through a checkpoint as if it were not
//! there.

use pdf_ffi::{
    apply_edit, open_from_bytes, redo, save_to_bytes, undo, FfiColor, FfiContentTextRun,
    FfiEditCommand, FfiRect, FfiSaveIntent, FfiSignatureAcknowledgement,
};
use std::sync::Arc;

fn open_lines(lines: &[&str]) -> Arc<pdf_ffi::DocumentHandle> {
    let mut doc = gen_fixtures::build_multi_line_page_document(lines);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("serialize fixture");
    open_from_bytes(bytes, None).expect("fixture should open")
}

fn runs(handle: &pdf_ffi::DocumentHandle) -> Vec<FfiContentTextRun> {
    handle.read_page_content(0).expect("read").text_runs
}

fn only_run(handle: &pdf_ffi::DocumentHandle) -> FfiContentTextRun {
    let mut runs = runs(handle);
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
    .expect("retype");
}

fn move_to(handle: &pdf_ffi::DocumentHandle, item: FfiContentTextRun, x: f64, y: f64) {
    let to = FfiRect { x, y, ..item.bbox };
    apply_edit(handle, FfiEditCommand::MoveTextRun { item, to }).expect("move");
}

fn saved_and_reopened(handle: &pdf_ffi::DocumentHandle) -> Arc<pdf_ffi::DocumentHandle> {
    let bytes = save_to_bytes(
        handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");
    open_from_bytes(bytes, None).expect("reopen")
}

fn insert(handle: &pdf_ffi::DocumentHandle, text: &str) -> FfiContentTextRun {
    let mut item = runs(handle).remove(0);
    item.id = 0;
    item.resource_font_name = "InsertedFont".to_string();
    item.bbox.y = 40.0;
    item.text = text.to_string();
    apply_edit(handle, FfiEditCommand::InsertTextRun { item }).expect("insert");
    runs(handle)
        .into_iter()
        .find(|run| run.text == text)
        .expect("inserted run is read back")
}

#[test]
fn a_retyped_run_can_be_moved_without_saving_first() {
    let handle = open_lines(&["Original text"]);
    retype(&handle, only_run(&handle), "Changed text");

    move_to(&handle, only_run(&handle), 120.0, 140.0);

    let moved = only_run(&handle);
    assert_eq!(moved.text, "Changed text");
    assert!((moved.bbox.x - 120.0).abs() < 0.01, "{:?}", moved.bbox);
    assert!((moved.bbox.y - 140.0).abs() < 0.01, "{:?}", moved.bbox);
    let written = only_run(&saved_and_reopened(&handle));
    assert_eq!(written.text, "Changed text");
    assert!((written.bbox.x - 120.0).abs() < 0.01, "{:?}", written.bbox);
}

#[test]
fn a_moved_run_can_be_retyped_without_saving_first() {
    let handle = open_lines(&["Original text"]);
    move_to(&handle, only_run(&handle), 80.0, 90.0);

    retype(&handle, only_run(&handle), "Changed after the move");

    let written = only_run(&saved_and_reopened(&handle));
    assert_eq!(written.text, "Changed after the move");
    assert!((written.bbox.x - 80.0).abs() < 0.01, "{:?}", written.bbox);
    assert!((written.bbox.y - 90.0).abs() < 0.01, "{:?}", written.bbox);
}

#[test]
fn a_pending_insertion_can_be_deleted_without_saving_first() {
    let handle = open_lines(&["Original text"]);
    let inserted = insert(&handle, "Inserted text");

    apply_edit(&handle, FfiEditCommand::RemoveTextRun { item: inserted })
        .expect("delete the insertion");

    assert_eq!(only_run(&handle).text, "Original text");
    assert_eq!(only_run(&saved_and_reopened(&handle)).text, "Original text");
}

#[test]
fn only_the_target_run_changes_when_a_checkpoint_rebinds_it() {
    let handle = open_lines(&["First line", "Second line"]);
    let before = runs(&handle);
    retype(&handle, before[0].clone(), "First line, longer now");
    let current = runs(&handle)
        .into_iter()
        .find(|run| run.text == "First line, longer now")
        .unwrap();

    move_to(&handle, current, 200.0, 300.0);

    let after = runs(&saved_and_reopened(&handle));
    let second = after.iter().find(|run| run.text == "Second line").unwrap();
    assert_eq!(second.bbox, before[1].bbox);
    let first = after
        .iter()
        .find(|run| run.text == "First line, longer now")
        .unwrap();
    assert!((first.bbox.x - 200.0).abs() < 0.01, "{:?}", first.bbox);
}

#[test]
fn undo_and_redo_walk_through_a_checkpoint_one_step_at_a_time() {
    let handle = open_lines(&["Original text"]);
    let original = only_run(&handle);
    retype(&handle, only_run(&handle), "Changed text");
    let retyped = only_run(&handle);
    move_to(&handle, retyped.clone(), 120.0, 140.0);
    let moved = only_run(&handle);

    assert!(handle.can_undo());
    assert!(undo(&handle), "undo the move");
    let back = only_run(&handle);
    assert_eq!(back.text, "Changed text");
    assert!(
        (back.bbox.x - retyped.bbox.x).abs() < 0.01,
        "{:?}",
        back.bbox
    );
    assert!(undo(&handle), "undo the retype, across the checkpoint");
    assert_eq!(only_run(&handle), original);
    assert!(!undo(&handle));
    assert!(!handle.can_undo());

    assert!(handle.can_redo());
    assert!(redo(&handle));
    assert_eq!(only_run(&handle).text, "Changed text");
    assert!(redo(&handle), "redo the move, across the checkpoint");
    let again = only_run(&handle);
    assert_eq!(again.text, moved.text);
    assert!((again.bbox.x - 120.0).abs() < 0.01, "{:?}", again.bbox);
    assert!(!redo(&handle));
    assert!(!handle.can_redo());
}

#[test]
fn a_fresh_edit_after_undoing_across_a_checkpoint_drops_the_redo() {
    let handle = open_lines(&["Original text"]);
    retype(&handle, only_run(&handle), "Changed text");
    move_to(&handle, only_run(&handle), 120.0, 140.0);
    assert!(undo(&handle));
    assert!(undo(&handle));

    retype(&handle, only_run(&handle), "Another change");

    assert!(!handle.can_redo());
    assert!(!redo(&handle));
    assert_eq!(
        only_run(&saved_and_reopened(&handle)).text,
        "Another change"
    );
}

#[test]
fn a_session_with_a_pending_annotation_still_refuses_and_keeps_its_history() {
    let handle = open_lines(&["Original text"]);
    apply_edit(
        &handle,
        FfiEditCommand::AddHighlight {
            page: 0,
            rect: FfiRect {
                x: 10.0,
                y: 10.0,
                width: 40.0,
                height: 10.0,
            },
            color: FfiColor {
                r: 255,
                g: 255,
                b: 0,
            },
        },
    )
    .expect("highlight");
    retype(&handle, only_run(&handle), "Changed text");
    let current = only_run(&handle);

    let to = FfiRect {
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
    assert!(undo(&handle));
    assert!(!undo(&handle));
}

/// The Windows editor sends the run it opened over with every pause in the
/// typing. After the first write took a checkpoint, that run names an item
/// of the old base; it must still fold into the retype the checkpoint
/// queued, not become a second edit of the same run that no save can replay.
#[test]
fn an_editor_resending_the_run_it_opened_over_folds_after_a_checkpoint() {
    // Two lines, so the checkpoint renumbers them: the moved run is painted
    // last once written, and the id the editor holds then names the other.
    let handle = open_lines(&["First line", "Second line"]);
    let original = runs(&handle);
    move_to(&handle, original[0].clone(), 80.0, 90.0);
    let opened = runs(&handle)
        .into_iter()
        .find(|run| run.text == "First line")
        .unwrap();

    retype(&handle, opened.clone(), "Changed");
    retype(&handle, opened.clone(), "Changed again");
    retype(&handle, opened, "Changed for good");

    let written = runs(&saved_and_reopened(&handle));
    let texts: Vec<&str> = written.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(texts.len(), 2, "{texts:?}");
    assert!(texts.contains(&"Changed for good"), "{texts:?}");
    assert!(texts.contains(&"Second line"), "{texts:?}");
    let changed = written
        .iter()
        .find(|run| run.text == "Changed for good")
        .unwrap();
    assert!((changed.bbox.x - 80.0).abs() < 0.01, "{:?}", changed.bbox);
    assert!(undo(&handle), "the three writes are one step");
    assert!(runs(&handle).iter().any(|run| run.text == "First line"));
    assert!(undo(&handle), "then the move");
    assert_eq!(runs(&handle), original);
    assert!(!undo(&handle));
}

#[test]
fn a_session_says_whether_it_can_checkpoint() {
    let handle = open_lines(&["Original text"]);
    assert!(!handle.content_edits_can_checkpoint(), "nothing is queued");
    move_to(&handle, only_run(&handle), 80.0, 90.0);
    assert!(handle.content_edits_can_checkpoint());

    apply_edit(
        &handle,
        FfiEditCommand::AddHighlight {
            page: 0,
            rect: FfiRect {
                x: 10.0,
                y: 10.0,
                width: 40.0,
                height: 10.0,
            },
            color: FfiColor {
                r: 255,
                g: 255,
                b: 0,
            },
        },
    )
    .expect("highlight");
    assert!(
        !handle.content_edits_can_checkpoint(),
        "an annotation is queued"
    );
}

/// One line drawn with character spacing (`Tc`), as the reader's quote
/// software writes them. A retyped run's box is then an estimate that the
/// written text does not match, so a block move checked against the pending
/// retype found nothing and was refused until a save.
fn spaced_line() -> Arc<pdf_ffi::DocumentHandle> {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let content = b"BT /F1 9 Tf 0.4 Tc 330 650 Td (11130 Chiclana) Tj ET \
                    BT /F1 9 Tf 0.4 Tc 330 686 Td (Domingo Parra) Tj ET"
        .to_vec();
    let content_id = document.add_object(Stream::new(dictionary! {}, content));
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
        "Encoding" => "WinAnsiEncoding",
    });
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
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
    open_from_bytes(bytes, None).expect("fixture should open")
}

#[test]
fn a_retyped_spaced_run_can_be_moved_as_a_block_without_saving_first() {
    let handle = spaced_line();
    let run = runs(&handle)
        .into_iter()
        .find(|run| run.text == "11130 Chiclana")
        .unwrap();
    retype(&handle, run.clone(), "11130 Chiclana de la Frontera");
    let block = pdf_ffi::read_page_text_blocks(&handle, 0)
        .expect("blocks")
        .into_iter()
        .find(|block| block.text.starts_with("11130 Chiclana de la Frontera"))
        .expect("the retyped block");

    pdf_ffi::move_text_block(&handle, block.runs, 0.0, -30.0).expect("move the retyped block");

    let written = runs(&saved_and_reopened(&handle));
    let moved = written
        .iter()
        .find(|run| run.text == "11130 Chiclana de la Frontera")
        .expect("retyped text survives");
    assert!(
        (moved.bbox.y - (run.bbox.y - 30.0)).abs() < 0.01,
        "{:?}",
        moved.bbox
    );
    assert!(written.iter().any(|run| run.text == "Domingo Parra"));
    assert!(undo(&handle), "undo the move");
    assert!(undo(&handle), "undo the retype, across the checkpoint");
    assert!(!undo(&handle));
}
