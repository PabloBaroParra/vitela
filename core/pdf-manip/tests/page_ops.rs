//! Integration tests (T-023, TDD): rotate/reorder/extract/delete page ops.
//! See spec.md "Rotate, Reorder, Extract, Delete Pages".

mod support;

use pdf_manip::{delete_pages, extract_pages, reorder_pages, rotate_page, LopdfDocument};

fn labels_in_page_order(doc: &LopdfDocument) -> Vec<String> {
    doc.as_lopdf()
        .get_pages()
        .values()
        .map(|&id| support::page_label(doc.as_lopdf(), id))
        .collect()
}

fn rotate_value(doc: &LopdfDocument, page_number: u32) -> i64 {
    let page_id = *doc.as_lopdf().get_pages().get(&page_number).unwrap();
    doc.as_lopdf()
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Rotate")
        .and_then(|o| o.as_i64())
        .unwrap_or(0)
}

#[test]
fn rotate_page_sets_and_accumulates_rotate_entry() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1", "P2"]));

    let rotated_once = rotate_page(&doc, 1, 90).expect("rotate should succeed");
    assert_eq!(rotate_value(&rotated_once, 1), 90);

    let rotated_twice = rotate_page(&rotated_once, 1, 90).expect("rotate should succeed");
    assert_eq!(rotate_value(&rotated_twice, 1), 180);
}

#[test]
fn rotate_page_normalizes_modulo_360() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1"]));
    let rotated = rotate_page(&doc, 1, 450).expect("rotate should succeed"); // 450 % 360 = 90
    assert_eq!(rotate_value(&rotated, 1), 90);
}

#[test]
fn rotate_page_rejects_out_of_range_page_number() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1"]));
    assert!(rotate_page(&doc, 2, 90).is_err());
    assert!(rotate_page(&doc, 0, 90).is_err());
}

#[test]
fn delete_page_removes_exactly_one_page_and_keeps_others_intact() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&[
        "P1", "P2", "P3", "P4", "P5",
    ]));

    let after = delete_pages(&doc, &[3]).expect("delete should succeed");
    assert_eq!(labels_in_page_order(&after), vec!["P1", "P2", "P4", "P5"]);
}

#[test]
fn delete_pages_rejects_out_of_range_page_number() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1", "P2"]));
    assert!(delete_pages(&doc, &[5]).is_err());
}

#[test]
fn reorder_pages_rearranges_kids_to_requested_sequence() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1", "P2", "P3"]));

    let reordered = reorder_pages(&doc, &[3, 1, 2]).expect("reorder should succeed");
    assert_eq!(labels_in_page_order(&reordered), vec!["P3", "P1", "P2"]);
}

#[test]
fn reorder_pages_rejects_non_permutation_input() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1", "P2", "P3"]));

    assert!(
        reorder_pages(&doc, &[1, 1, 2]).is_err(),
        "duplicate page number"
    );
    assert!(reorder_pages(&doc, &[1, 2]).is_err(), "wrong length");
    assert!(
        reorder_pages(&doc, &[1, 2, 4]).is_err(),
        "out-of-range page number"
    );
}

#[test]
fn extract_pages_produces_document_with_only_requested_pages_in_requested_order() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1", "P2", "P3", "P4"]));

    let extracted = extract_pages(&doc, &[3, 1]).expect("extract should succeed");
    assert_eq!(labels_in_page_order(&extracted), vec!["P3", "P1"]);
}

#[test]
fn extract_pages_rejects_unknown_page_number() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1", "P2"]));
    assert!(extract_pages(&doc, &[5]).is_err());
}

#[test]
fn extract_pages_rejects_empty_selection() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1"]));
    assert!(extract_pages(&doc, &[]).is_err());
}
