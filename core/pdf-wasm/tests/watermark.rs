//! The `Engine`'s watermark surface against the real pdfium, in a headless
//! browser (B26 T-256). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{watermark_defaults, EditCommand};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

fn draft(pages: Vec<u32>, text: &str) -> EditCommand {
    let look = watermark_defaults();
    EditCommand::add_text_watermark(
        pages,
        text.into(),
        look.font_size(),
        look.text_opacity(),
        look.text_rotation_degrees(),
        look.text_behind(),
    )
}

#[wasm_bindgen_test]
async fn a_text_stamp_is_undoable_refreshes_the_preview_and_is_saved() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["one", "two"]), None)
        .unwrap();
    assert!(engine.watermark_refusal(doc).unwrap().is_none());
    let before = engine.render_page(doc, 0, 36, false).unwrap().take_rgba();

    assert_eq!(engine.paint_revision(doc).unwrap(), 0.0);
    engine.apply_edit(doc, &draft(vec![0, 1], "DRAFT")).unwrap();
    assert!(engine.paint_revision(doc).unwrap() > 0.0);
    assert!(engine.can_undo(doc).unwrap());
    assert!(engine.preview_stale(doc).unwrap());
    engine.refresh_preview(doc).unwrap();
    let stamped = engine.render_page(doc, 0, 36, false).unwrap().take_rgba();
    assert_ne!(before, stamped, "the preview shows the stamp");

    let saved = engine.save_to_bytes(doc, false).unwrap();
    let reopened = engine.open(saved, None).unwrap();
    let runs = engine.text_runs(reopened, 1).unwrap();
    assert!(runs.iter().any(|run| run.text().contains("DRAFT")));

    assert!(engine.undo(doc).unwrap());
    assert!(!engine.can_undo(doc).unwrap());
    assert_eq!(engine.paint_revision(doc).unwrap(), 0.0);
}

#[wasm_bindgen_test]
async fn a_picture_stamp_and_its_refusals() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::text_document(&["one"]), None).unwrap();
    let look = watermark_defaults();
    let bad = EditCommand::add_picture_watermark(
        vec![0],
        b"not a picture".to_vec(),
        look.picture_scale(),
        look.picture_opacity(),
        look.picture_rotation_degrees(),
        look.picture_behind(),
    );
    assert_eq!(
        error_name(engine.apply_edit(doc, &bad).err().unwrap()),
        "InvalidImage"
    );
    let gap = engine
        .apply_edit(doc, &draft(vec![0], "日本"))
        .err()
        .unwrap();
    assert_eq!(error_name(gap), "EncodingGap");
    let good = EditCommand::add_picture_watermark(
        vec![0],
        pdfs::PNG_1X1.to_vec(),
        look.picture_scale(),
        look.picture_opacity(),
        look.picture_rotation_degrees(),
        look.picture_behind(),
    );
    engine.apply_edit(doc, &good).unwrap();
    assert!(engine.can_undo(doc).unwrap());
}

#[wasm_bindgen_test]
async fn a_document_that_withholds_modification_refuses_a_stamp() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::restricted_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    assert!(engine.watermark_refusal(doc).unwrap().is_some());
    let err = engine
        .apply_edit(doc, &draft(vec![0], "DRAFT"))
        .err()
        .unwrap();
    assert_eq!(error_name(err), "EditNotPermitted");
    assert!(!engine.can_undo(doc).unwrap());
}
