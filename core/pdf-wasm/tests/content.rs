//! The `Engine`'s page-content surface against the real pdfium, in a
//! headless browser (B26 T-258). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
async fn reads_runs_images_paths_blocks_style_and_fonts() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::content_document(), None).unwrap();
    assert!(engine.content_editing_allowed(doc).unwrap());
    assert!(engine.content_editing_refusal(doc).unwrap().is_none());
    assert_eq!(engine.content_base_generation(doc).unwrap(), 0.0);

    let mut content = engine.read_page_content(doc, 0).unwrap();
    let runs = content.take_runs();
    let texts: Vec<String> = runs.iter().map(|run| run.text()).collect();
    assert_eq!(texts, ["Hello", " world", "First line", "Second line"]);
    assert_eq!(runs[0].font_name(), "F1");
    assert_eq!(runs[0].font_kind(), "standard14");
    assert!(runs[0].editable());
    assert_eq!(runs[0].bbox().len(), 4);
    let images = content.take_images();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].bbox(), [72.0, 500.0, 100.0, 50.0]);
    let paths = content.take_paths();
    let paints: Vec<String> = paths.iter().map(|path| path.paint()).collect();
    assert_eq!(paints, ["stroke", "fill"]);

    let lines = engine.read_text_blocks(doc, 0).unwrap();
    assert_eq!(lines[0].text(), "Hello world");
    assert_eq!(lines[0].run_ids(), [runs[0].id(), runs[1].id()]);
    let paragraphs = engine.read_paragraph_blocks(doc, 0).unwrap();
    assert!(paragraphs
        .iter()
        .any(|block| block.text() == "First line\nSecond line"));

    let style = engine
        .text_run_style(doc, 0, runs[0].id())
        .unwrap()
        .expect("a style");
    assert_eq!(style.base_font(), "Helvetica");
    assert_eq!(style.font_size(), 12.0);
    assert_eq!(
        error_name(engine.text_run_style(doc, 0, 0.5).err().unwrap()),
        "UnsupportedOperation"
    );
    assert_eq!(
        engine.page_font_families(doc, 0).unwrap(),
        ["F1", "Helvetica"]
    );
}

#[wasm_bindgen_test]
async fn the_gates_refuse_with_the_cores_words() {
    let mut engine = engine().await;
    let unmodifiable = engine
        .open(
            pdfs::unmodifiable_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    assert!(!engine.content_editing_allowed(unmodifiable).unwrap());
    assert!(engine
        .content_editing_refusal(unmodifiable)
        .unwrap()
        .is_some());
    assert!(engine.read_page_content(unmodifiable, 0).is_ok());

    let restricted = engine
        .open(
            pdfs::restricted_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    assert_eq!(
        error_name(engine.read_page_content(restricted, 0).err().unwrap()),
        "TextExtractionDenied"
    );
    assert_eq!(
        error_name(engine.read_page_content(restricted, 3).err().unwrap()),
        "TextExtractionDenied",
        "the permission is asked before the page"
    );

    let undecodable = engine.open(pdfs::undecodable_document(), None).unwrap();
    let mut content = engine.read_page_content(undecodable, 0).unwrap();
    assert!(!content.take_runs()[0].editable());
}
