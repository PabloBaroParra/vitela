//! The `Engine`'s extract and split surface against the real pdfium, in a
//! headless browser (B26 T-254). Run like `tests/engine.rs`.
#![cfg(target_arch = "wasm32")]

use pdf_wasm::{plan_split, EditCommand};
use wasm_bindgen_test::*;

#[path = "support/browser.rs"]
mod browser;
#[path = "support/pdfs.rs"]
mod pdfs;

use browser::{engine, error_name};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
async fn the_extracted_pdf_holds_the_pages_as_they_are_now() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["one", "two", "three", "four"]), None)
        .unwrap();
    assert!(engine.extraction_refusal(doc).unwrap().is_none());
    assert!(!engine.extract_source_is_signed(doc).unwrap());
    engine
        .apply_edit(doc, &EditCommand::remove_pages(0, 1))
        .unwrap();

    // Positions 0-1 are now "two" and "three".
    let bytes = engine.extract_pages_to_bytes(doc, vec![0, 1]).unwrap();
    let extracted = engine.open(bytes, None).unwrap();
    assert_eq!(engine.page_count(extracted).unwrap(), 2);
    let mut page = engine.render_page(extracted, 0, 36, false).unwrap();
    assert!(!page.take_rgba().is_empty());

    // Not an edit: the session still has its one step to undo, and its pages.
    assert_eq!(engine.page_count(doc).unwrap(), 3);
    assert!(engine.can_undo(doc).unwrap());
    assert!(!engine.can_redo(doc).unwrap());
}

#[wasm_bindgen_test]
async fn a_selection_the_core_cannot_cut_is_refused() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["one", "two"]), None)
        .unwrap();
    for pages in [vec![], vec![1, 0]] {
        let err = engine.extract_pages_to_bytes(doc, pages).err().unwrap();
        assert_eq!(error_name(err), "InvalidPageSelection");
    }
    let err = engine.extract_pages_to_bytes(doc, vec![7]).err().unwrap();
    assert_eq!(error_name(err), "PageIndexOutOfBounds");
}

#[wasm_bindgen_test]
async fn a_document_withholding_extraction_is_refused_ahead() {
    let mut engine = engine().await;
    let doc = engine
        .open(
            pdfs::restricted_document(),
            Some(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
    assert_eq!(
        engine.extraction_refusal(doc).unwrap().as_deref(),
        Some("this document does not permit extracting its pages")
    );
    let err = engine.extract_pages_to_bytes(doc, vec![0]).err().unwrap();
    assert_eq!(error_name(err), "UnsupportedOperation");
}

#[wasm_bindgen_test]
async fn a_signed_source_says_so_and_is_still_extracted() {
    let mut engine = engine().await;
    let doc = engine.open(pdfs::SIGNED.to_vec(), None).unwrap();
    assert!(engine.extract_source_is_signed(doc).unwrap());
    let bytes = engine.extract_pages_to_bytes(doc, vec![0]).unwrap();
    let extracted = engine.open(bytes, None).unwrap();
    assert_eq!(engine.page_count(extracted).unwrap(), 1);
}

#[wasm_bindgen_test]
async fn a_split_plan_and_its_parts() {
    let mut engine = engine().await;
    let doc = engine
        .open(pdfs::text_document(&["1", "2", "3", "4", "5"]), None)
        .unwrap();
    let parts = plan_split("2,4".into(), 5, "report.pdf".into()).unwrap();
    let summary: Vec<_> = parts
        .iter()
        .map(|part| (part.first(), part.last(), part.file_name()))
        .collect();
    assert_eq!(
        summary,
        vec![
            (0, 1, "report-part1.pdf".to_string()),
            (2, 3, "report-part2.pdf".to_string()),
            (4, 4, "report-part3.pdf".to_string()),
        ]
    );
    for (part, pages) in parts.iter().zip([2, 2, 1]) {
        let bytes = engine
            .extract_pages_to_bytes(doc, (part.first()..=part.last()).collect())
            .unwrap();
        let opened = engine.open(bytes, None).unwrap();
        assert_eq!(engine.page_count(opened).unwrap(), pages);
    }

    let err = plan_split("5".into(), 5, "report.pdf".into())
        .err()
        .unwrap();
    assert_eq!(error_name(err), "InvalidPageSelection");
}
