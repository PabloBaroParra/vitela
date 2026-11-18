//! The `Engine`'s annotation surface against the real pdfium, in a headless
//! browser (B26 T-236). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{free_text_layout, stamp_placement, EditCommand, Engine};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

const YELLOW: u32 = 0xffff00;

/// Every `/Highlight` the saved bytes carry, as its `/Rect`, read back with
/// lopdf — independent of everything the engine does.
fn saved_highlights(bytes: &[u8]) -> Vec<Vec<f32>> {
    let document = lopdf::Document::load_mem(bytes).expect("saved bytes parse");
    document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| {
            dict.get(b"Subtype")
                .and_then(lopdf::Object::as_name)
                .is_ok_and(|subtype| subtype == b"Highlight")
        })
        .map(|dict| {
            dict.get(b"Rect")
                .and_then(lopdf::Object::as_array)
                .expect("a rect")
                .iter()
                .map(|value| value.as_float().expect("a number"))
                .collect()
        })
        .collect()
}

fn open_text(engine: &mut Engine) -> u32 {
    engine
        .open(pdfs::text_document(&["one", "two"]), None)
        .unwrap()
}

#[wasm_bindgen_test]
async fn a_highlight_saves_reopens_with_lopdf_and_its_undo_takes_it_out() {
    let mut engine = engine().await;
    let doc = open_text(&mut engine);
    assert!(engine.annotation_editing_allowed(doc).unwrap());

    let added = engine
        .apply_edit(
            doc,
            &EditCommand::add_highlight(1, 72.0, 690.0, 120.0, 30.0, YELLOW),
        )
        .unwrap();
    assert_eq!(added, Some(0));
    assert!(engine.can_undo(doc).unwrap());
    assert!(
        !engine.preview_stale(doc).unwrap(),
        "the shell draws it; the pixels need no refresh"
    );
    let snapshot = engine.annotations(doc).unwrap();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(
        (snapshot[0].id(), snapshot[0].page(), snapshot[0].kind()),
        (0, 1, "highlight".to_string())
    );
    assert_eq!(snapshot[0].rect(), [72.0, 690.0, 120.0, 30.0]);
    assert_eq!(snapshot[0].color(), Some(YELLOW));

    let saved = engine.save_to_bytes(doc, false).unwrap();
    assert_eq!(saved_highlights(&saved), [vec![72.0, 690.0, 192.0, 720.0]]);
    // The saved file opens, and the highlight is now the file's own.
    let reopened = engine.open(saved, None).unwrap();
    assert_eq!(engine.page_count(reopened).unwrap(), 2);
    assert!(engine.annotations(reopened).unwrap().is_empty());

    assert!(engine.undo(doc).unwrap());
    assert!(engine.annotations(doc).unwrap().is_empty());
    assert!(!engine.preview_stale(doc).unwrap());
    assert!(saved_highlights(&engine.save_to_bytes(doc, false).unwrap()).is_empty());
    assert!(engine.redo(doc).unwrap());
    assert_eq!(engine.annotations(doc).unwrap().len(), 1);
}

#[wasm_bindgen_test]
async fn without_the_annotate_permission_an_annotation_edit_is_edit_not_permitted() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::restricted_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    assert!(!engine.annotation_editing_allowed(doc).unwrap());
    for edit in [
        EditCommand::add_highlight(0, 72.0, 690.0, 120.0, 30.0, YELLOW),
        // The permission is asked before the position.
        EditCommand::add_highlight(9, 72.0, 690.0, 120.0, 30.0, YELLOW),
        EditCommand::remove_annotation(0),
    ] {
        let err = engine.apply_edit(doc, &edit).unwrap_err();
        assert_eq!(error_name(err), "EditNotPermitted");
    }
    assert!(!engine.can_undo(doc).unwrap());
    // Reading needs no permission.
    assert!(engine.annotations(doc).unwrap().is_empty());
    assert!(engine.comments(doc).unwrap().is_empty());
}

#[wasm_bindgen_test]
async fn changes_are_steps_and_failures_are_named() {
    let mut engine = engine().await;
    let doc = open_text(&mut engine);
    engine
        .apply_edit(
            doc,
            &EditCommand::add_free_text(0, 72.0, 600.0, 120.0, 40.0, "Hello box".into()),
        )
        .unwrap();
    engine
        .apply_edit(doc, &EditCommand::move_annotation(0, 10.0, -20.0))
        .unwrap();
    engine
        .apply_edit(doc, &EditCommand::retext_annotation(0, "Bye".into()))
        .unwrap();
    // The same text again records nothing.
    assert_eq!(
        engine
            .apply_edit(doc, &EditCommand::retext_annotation(0, "Bye".into()))
            .unwrap(),
        None
    );

    let entry = &engine.annotations(doc).unwrap()[0];
    assert_eq!(entry.kind(), "freeText");
    assert_eq!(entry.rect(), [82.0, 580.0, 120.0, 40.0]);
    assert_eq!(entry.contents(), "Bye");
    assert_eq!(entry.color(), None);
    let layout = entry.layout().expect("a free-text box has its layout");
    assert_eq!(layout.texts(), ["Bye"]);

    for (edit, name) in [
        (EditCommand::remove_annotation(7), "AnnotationNotFound"),
        (
            EditCommand::recolor_annotation(0, YELLOW),
            "UnsupportedOperation",
        ),
        (
            EditCommand::add_free_text(0, 72.0, 600.0, 120.0, 40.0, "日本".into()),
            "EncodingGap",
        ),
        (
            EditCommand::add_stamp(0, b"no image".to_vec(), 0.0, 0.0, 10.0, 10.0),
            "InvalidImage",
        ),
        (
            EditCommand::add_highlight(2, 0.0, 0.0, 10.0, 10.0, YELLOW),
            "PageIndexOutOfBounds",
        ),
    ] {
        assert_eq!(error_name(engine.apply_edit(doc, &edit).unwrap_err()), name);
    }

    // Three steps: add, move, retext.
    for _ in 0..3 {
        assert!(engine.undo(doc).unwrap());
    }
    assert!(!engine.can_undo(doc).unwrap());
}

#[wasm_bindgen_test]
async fn a_page_move_moves_the_annotations_and_only_it_stales_the_preview() {
    let mut engine = engine().await;
    let doc = open_text(&mut engine);
    engine
        .apply_edit(
            doc,
            &EditCommand::add_ink(0, vec![10.0, 10.0, 50.0, 80.0], 0x0000ff),
        )
        .unwrap();
    engine
        .apply_edit(doc, &EditCommand::move_pages(0, 1, 1))
        .unwrap();
    assert!(engine.preview_stale(doc).unwrap());
    let ink = &engine.annotations(doc).unwrap()[0];
    assert_eq!((ink.kind(), ink.page()), ("ink".to_string(), 1));
    assert_eq!(ink.points(), [10.0, 10.0, 50.0, 80.0]);
    assert!(ink.rect().is_empty());

    engine.refresh_preview(doc).unwrap();
    assert!(!engine.preview_stale(doc).unwrap());
    assert!(engine.undo(doc).unwrap(), "the page move");
    assert!(engine.preview_stale(doc).unwrap());
    engine.refresh_preview(doc).unwrap();
    assert!(engine.undo(doc).unwrap(), "the ink");
    assert!(!engine.preview_stale(doc).unwrap());
}

#[wasm_bindgen_test]
async fn comments_list_the_files_notes_then_the_sessions() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::commented_document(&["one"]), None)
        .unwrap();
    engine
        .apply_edit(
            doc,
            &EditCommand::add_note(0, 100.0, 100.0, 20.0, 20.0, "Mine".into()),
        )
        .unwrap();

    let comments = engine.comments(doc).unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].contents(), pdfs::FILE_NOTE);
    assert_eq!(
        comments[0].author().as_deref(),
        Some(pdfs::FILE_NOTE_AUTHOR)
    );
    assert_eq!(comments[0].annotation_id(), None);
    assert_eq!(comments[0].rect(), [40.0, 600.0, 20.0, 20.0]);
    assert_eq!(comments[1].contents(), "Mine");
    assert_eq!(comments[1].author(), None);
    assert_eq!(comments[1].annotation_id(), Some(0));
}

#[wasm_bindgen_test]
fn the_free_text_and_stamp_geometry_is_the_cores() {
    let layout = free_text_layout("Hello box", 120.0, 30.0).unwrap();
    assert_eq!(layout.texts(), ["Hello box"]);
    assert!(layout.font_size_pt() > 0.0);
    assert!(!layout.overflow());
    assert_eq!(
        error_name(free_text_layout("日", 120.0, 30.0).unwrap_err()),
        "EncodingGap"
    );

    assert_eq!(
        stamp_placement(pdfs::PNG_1X1, 100.0, 500.0).unwrap(),
        [100.0, 356.0, 144.0, 144.0]
    );
    assert_eq!(
        error_name(stamp_placement(b"nope", 0.0, 0.0).unwrap_err()),
        "InvalidImage"
    );
}
