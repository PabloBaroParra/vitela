//! Text blocks through the FFI (B25 T-207): read, move, move again, retype,
//! remove, undo — and what a reopened save says about each.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_ffi::{
    apply_edit, move_text_block, open_from_bytes, read_page_text_blocks, remove_text_block,
    retype_text_block, save_to_bytes, undo, FfiEditCommand, FfiSaveIntent,
    FfiSignatureAcknowledgement, FfiTextBlock,
};

/// The reference quote's heading, written the way its generator wrote it —
/// three show-text operators — and a "Total" column far to its right.
fn heading_page() -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let content = b"BT /F1 12 Tf 100 700 Td (PRESUPUES) Tj (TO N 3) Tj (.725/1) Tj \
                    200 0 Td (Total) Tj ET"
        .to_vec();
    let content_id = document.add_object(Stream::new(dictionary! {}, content));
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
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

fn heading(handle: &pdf_ffi::DocumentHandle) -> FfiTextBlock {
    read_page_text_blocks(handle, 0)
        .expect("blocks")
        .into_iter()
        .find(|block| block.text.starts_with("PRESUPUES"))
        .expect("the heading block")
}

fn texts(handle: &pdf_ffi::DocumentHandle) -> Vec<String> {
    read_page_text_blocks(handle, 0)
        .expect("blocks")
        .into_iter()
        .map(|block| block.text)
        .collect()
}

#[test]
fn the_split_heading_reads_as_one_block_and_the_column_as_another() {
    let handle = open_from_bytes(heading_page(), None).expect("open");

    let blocks = read_page_text_blocks(&handle, 0).expect("blocks");

    let texts: Vec<&str> = blocks.iter().map(|block| block.text.as_str()).collect();
    assert_eq!(texts, ["PRESUPUESTO N 3.725/1", "Total"]);
    assert_eq!(blocks[0].runs.len(), 3);
    assert!(blocks
        .iter()
        .flat_map(|block| &block.runs)
        .all(|run| run.page == 0));
}

#[test]
fn a_moved_block_is_one_undo_step_and_reads_back_where_it_went() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    let before = heading(&handle);

    move_text_block(&handle, before.runs.clone(), 50.0, -100.0).expect("move");

    let after = heading(&handle);
    assert_eq!(after.runs.len(), 3);
    assert!((after.bbox.x - (before.bbox.x + 50.0)).abs() < 1e-3);
    assert!((after.bbox.y - (before.bbox.y - 100.0)).abs() < 1e-3);

    assert!(undo(&handle));
    assert!(!undo(&handle), "the whole block move was one step");
    assert!((heading(&handle).bbox.x - before.bbox.x).abs() < 1e-3);
}

/// The T-206 trap for blocks: the second move names the runs where the
/// first put them, which the file does not have. It must fold, not fail.
#[test]
fn a_block_moved_twice_folds_into_one_step_and_saves_the_final_place() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    let original = heading(&handle);

    // Down first: moved right on its own line, the heading would end a word
    // gap from "Total" and read as one block with it.
    move_text_block(&handle, original.runs.clone(), 0.0, -100.0).expect("first move");
    move_text_block(&handle, heading(&handle).runs, 50.0, 0.0).expect("second move");

    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    let moved = heading(&reopened);
    assert!((moved.bbox.x - (original.bbox.x + 50.0)).abs() < 1e-3);
    assert!((moved.bbox.y - (original.bbox.y - 100.0)).abs() < 1e-3);

    assert!(undo(&handle));
    assert!(!undo(&handle), "both moves are one step");
}

#[test]
fn a_retyped_block_saves_as_one_run_and_leaves_the_column_alone() {
    let handle = open_from_bytes(heading_page(), None).expect("open");

    retype_text_block(
        &handle,
        heading(&handle).runs,
        "FACTURA 12".to_string(),
        false,
    )
    .expect("retype");

    assert_eq!(texts(&handle), ["FACTURA 12", "Total"]);
    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert_eq!(texts(&reopened), ["FACTURA 12", "Total"]);
    let heading_runs = heading_runs(&reopened);
    assert_eq!(heading_runs, 1);
}

fn heading_runs(handle: &pdf_ffi::DocumentHandle) -> usize {
    read_page_text_blocks(handle, 0)
        .expect("blocks")
        .into_iter()
        .find(|block| block.text.starts_with("FACTURA"))
        .expect("the retyped block")
        .runs
        .len()
}

#[test]
fn a_removed_block_is_gone_after_a_save_and_back_after_an_undo() {
    let handle = open_from_bytes(heading_page(), None).expect("open");

    remove_text_block(&handle, heading(&handle).runs).expect("remove");
    assert_eq!(texts(&handle), ["Total"]);
    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert_eq!(texts(&reopened), ["Total"]);

    assert!(undo(&handle));
    assert_eq!(texts(&handle), ["PRESUPUESTO N 3.725/1", "Total"]);
}

/// One run of a moved block deleted on its own: it folds into the queued
/// move, the rest of the block stays where it was moved to, and the pair is
/// one undo step.
#[test]
fn a_run_of_a_moved_block_can_be_deleted_alone_before_a_save() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    move_text_block(&handle, heading(&handle).runs, 0.0, -100.0).expect("move");
    let run = heading(&handle).runs.remove(0);

    apply_edit(&handle, FfiEditCommand::RemoveTextRun { item: run }).expect("folds into the move");

    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    // Moved runs are repainted on top, after everything else on the page.
    assert_eq!(texts(&reopened), ["Total", "TO N 3.725/1"]);
    assert!(undo(&handle));
    assert!(!undo(&handle), "the move and the removal are one step");
}

/// A run retyped on its own after its block moved would be validated against
/// a file that has it elsewhere. The shell must get a reason, not the
/// generic refusal — and the queued move must survive.
#[test]
fn a_run_of_a_moved_block_cannot_be_retyped_alone_before_a_save() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    move_text_block(&handle, heading(&handle).runs, 0.0, -100.0).expect("move");
    let run = heading(&handle).runs.remove(0);

    let error = apply_edit(
        &handle,
        FfiEditCommand::ReplaceTextRunContent {
            item: run,
            after: "X".to_string(),
        },
    )
    .expect_err("refused until saved");

    assert!(error.to_string().contains("save the document"), "{error}");
    assert!(undo(&handle));
    assert!(!undo(&handle));
}

/// Retyping a block that is moved and not saved touches queued runs with a
/// different edit, which does not fold.
#[test]
fn a_moved_block_cannot_be_retyped_before_a_save() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    move_text_block(&handle, heading(&handle).runs, 50.0, 0.0).expect("move");

    let error = retype_text_block(&handle, heading(&handle).runs, "X".to_string(), false)
        .expect_err("refused until saved");

    assert!(error.to_string().contains("save the document"), "{error}");
}

/// The Windows editor writes as the reader types, sending the block it
/// opened over with each pause. Every write after the first folds.
#[test]
fn a_block_retyped_again_folds_into_one_step() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    let opened = heading(&handle).runs;

    retype_text_block(&handle, opened.clone(), "FACT".to_string(), false).expect("first");
    retype_text_block(&handle, opened, "FACTURA".to_string(), false).expect("second");

    assert_eq!(texts(&handle), ["FACTURA", "Total"]);
    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert_eq!(texts(&reopened), ["FACTURA", "Total"]);
    assert!(undo(&handle));
    assert!(!undo(&handle), "both retypes are one step");
}

/// After a block retype the page reads back one run with the new text, and
/// clicking it again edits that run on its own — which has to land in the
/// queued batch, not fail against the file.
#[test]
fn the_run_left_by_a_block_retype_can_be_retyped_on_its_own() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    retype_text_block(&handle, heading(&handle).runs, "FACT".to_string(), false).expect("block");
    let left = read_page_text_blocks(&handle, 0).expect("blocks")[0]
        .runs
        .clone();
    assert_eq!(left.len(), 1);

    apply_edit(
        &handle,
        FfiEditCommand::ReplaceTextRunContent {
            item: left[0].clone(),
            after: "FACTURA".to_string(),
        },
    )
    .expect("folds into the batch");

    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert_eq!(texts(&reopened), ["FACTURA", "Total"]);
    assert!(undo(&handle));
    assert!(!undo(&handle));
}

/// Moved, then deleted before a save: the reader sees the block where it
/// landed and presses Delete. The removal folds into the queued move.
#[test]
fn a_moved_block_can_be_deleted_before_a_save() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    move_text_block(&handle, heading(&handle).runs, 0.0, -100.0).expect("move");

    remove_text_block(&handle, heading(&handle).runs).expect("removal folds into the move");

    assert_eq!(texts(&handle), ["Total"]);
    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert_eq!(texts(&reopened), ["Total"]);
    assert!(undo(&handle));
    assert!(!undo(&handle), "the move and the removal are one step");
    assert_eq!(texts(&handle), ["PRESUPUESTO N 3.725/1", "Total"]);
}

/// Retyped, then deleted: the page reads back one run, and removing that
/// block has to take the whole retype with it.
#[test]
fn a_retyped_block_can_be_deleted_before_a_save() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    retype_text_block(&handle, heading(&handle).runs, "FACTURA".to_string(), false)
        .expect("retype");
    let left = read_page_text_blocks(&handle, 0).expect("blocks")[0]
        .runs
        .clone();

    remove_text_block(&handle, left).expect("removal folds into the retype");

    assert_eq!(texts(&handle), ["Total"]);
    let reopened = open_from_bytes(saved(&handle), None).expect("reopen");
    assert_eq!(texts(&reopened), ["Total"]);
}

#[test]
fn a_block_with_no_runs_is_refused() {
    let handle = open_from_bytes(heading_page(), None).expect("open");
    assert!(remove_text_block(&handle, Vec::new()).is_err());
    assert!(!undo(&handle));
}
