//! Integration tests (T-022, TDD): merge/split over fixtures built by the
//! shared test-support helper. See spec.md "Merge Documents" / "Split
//! Document".

// Only the page builders are needed here; the encrypted-fixture builder
// belongs to the permission tests that share this helper module.
#[allow(dead_code)]
mod support;

use pdf_manip::{merge, split, LopdfDocument};

#[test]
fn merge_preserves_selection_order_across_documents() {
    let doc_a = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["A1", "A2"]));
    let doc_b = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["B1"]));

    let merged = merge(&[doc_a, doc_b]).expect("merge should succeed");

    let pages = merged.as_lopdf().get_pages();
    assert_eq!(pages.len(), 3, "merged document has all pages");

    let labels: Vec<String> = pages
        .values()
        .map(|&id| support::page_label(merged.as_lopdf(), id))
        .collect();
    assert_eq!(labels, vec!["A1", "A2", "B1"], "pages in selection order");
}

#[test]
fn merge_of_empty_slice_is_an_error() {
    let result = merge(&[]);
    assert!(result.is_err(), "merging zero documents must be rejected");
}

#[test]
fn merge_of_single_document_preserves_its_pages() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["Only1", "Only2"]));
    let merged = merge(&[doc]).expect("merge of one document should succeed");

    let pages = merged.as_lopdf().get_pages();
    let labels: Vec<String> = pages
        .values()
        .map(|&id| support::page_label(merged.as_lopdf(), id))
        .collect();
    assert_eq!(labels, vec!["Only1", "Only2"]);
}

#[test]
fn split_at_page_boundary_produces_two_documents_with_correct_ranges() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&[
        "P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "P9", "P10",
    ]));

    let (left, right) = split(&doc, 5).expect("split should succeed");

    assert_eq!(left.as_lopdf().get_pages().len(), 5);
    assert_eq!(right.as_lopdf().get_pages().len(), 5);

    let left_labels: Vec<String> = left
        .as_lopdf()
        .get_pages()
        .values()
        .map(|&id| support::page_label(left.as_lopdf(), id))
        .collect();
    assert_eq!(left_labels, vec!["P1", "P2", "P3", "P4", "P5"]);

    let right_labels: Vec<String> = right
        .as_lopdf()
        .get_pages()
        .values()
        .map(|&id| support::page_label(right.as_lopdf(), id))
        .collect();
    assert_eq!(right_labels, vec!["P6", "P7", "P8", "P9", "P10"]);
}

#[test]
fn split_rejects_boundary_that_leaves_a_side_empty() {
    let doc = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["P1", "P2"]));

    assert!(
        split(&doc, 0).is_err(),
        "after_page = 0 leaves left side empty"
    );
    assert!(
        split(&doc, 2).is_err(),
        "after_page = total leaves right side empty"
    );
}
