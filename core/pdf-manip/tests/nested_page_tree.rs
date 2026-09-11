//! Integration tests for *nested* page trees — the shape every other test in
//! this crate avoids by building a single `/Pages` node with every page
//! hanging off it.
//!
//! Two different things break on a nested tree, and both are covered here:
//! an attribute a page inherits from more than one hop up the `/Parent`
//! chain (what a graft reads out of the source), and the root `/Kids` not
//! being the document's page list (what a graft, and an insertion, write into
//! the destination).

// The encrypted-fixture builder belongs to the permission tests that share
// this helper module.
#[allow(dead_code)]
mod support;

use lopdf::{Document, ObjectId};
use pdf_document::{Orientation, PageSize};
use pdf_manip::{graft_pages, insert_blank_page, remove_page, LopdfDocument};

fn grafted(
    destination: &LopdfDocument,
    index: usize,
    source: &LopdfDocument,
    pages: &[usize],
) -> LopdfDocument {
    graft_pages(destination, index, source, pages)
        .expect("graft should succeed")
        .document
}

fn page_at(document: &LopdfDocument, number: u32) -> ObjectId {
    *document
        .as_lopdf()
        .get_pages()
        .get(&number)
        .unwrap_or_else(|| panic!("page {number} should exist"))
}

fn integer(document: &LopdfDocument, page: ObjectId, key: &[u8]) -> Option<i64> {
    document
        .as_lopdf()
        .get_dictionary(page)
        .expect("page dict")
        .get(key)
        .and_then(|value| value.as_i64())
        .ok()
}

fn root_count(document: &LopdfDocument) -> i64 {
    let lopdf = document.as_lopdf();
    let root = lopdf
        .catalog()
        .expect("catalog")
        .get(b"Pages")
        .and_then(|value| value.as_reference())
        .expect("page tree root");
    lopdf
        .get_dictionary(root)
        .expect("root dict")
        .get(b"Count")
        .and_then(|value| value.as_i64())
        .expect("the page tree root has to carry a /Count")
}

#[test]
fn a_page_inherits_across_two_hops_and_the_nearest_ancestor_wins() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    // S2 hangs off the intermediate node: its geometry lives two hops up on
    // the root, its rotation one hop up on the branch, and the root sets a
    // different rotation that must lose.
    let source = LopdfDocument::from_lopdf(support::pdf_with_a_nested_page_tree(&["S1", "S2"]));

    let result = grafted(&destination, 1, &source, &[1]);

    let imported = page_at(&result, 2);
    let media_box: Vec<i64> = result
        .as_lopdf()
        .get_dictionary(imported)
        .expect("page dict")
        .get(b"MediaBox")
        .and_then(|value| value.as_array())
        .expect("geometry inherited two hops up has to be materialized")
        .iter()
        .map(|value| value.as_i64().expect("integer"))
        .collect();
    assert_eq!(media_box, vec![0, 0, 300, 400]);
    assert_eq!(
        integer(&result, imported, b"Rotate"),
        Some(180),
        "the branch's rotation has to win over the root's"
    );
    assert!(
        result
            .as_lopdf()
            .get_dictionary(imported)
            .expect("page dict")
            .get(b"Resources")
            .is_ok(),
        "resources inherited two hops up have to be materialized"
    );
}

#[test]
fn the_sources_intermediate_page_tree_node_does_not_come_along() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_a_nested_page_tree(&["S1", "S2"]));

    let result = grafted(&destination, 1, &source, &[1]);

    assert_eq!(
        support::page_tree_node_count(&result),
        1,
        "only the destination's own page tree node may survive the graft"
    );
    assert_eq!(support::labels(&result), vec!["D1", "S2"]);
}

#[test]
fn grafting_into_a_nested_destination_inserts_at_the_page_index() {
    // D1 hangs off the root, D2 and D3 off the intermediate node — so the
    // root has two `/Kids` for three pages, and page index 2 is *inside* the
    // branch rather than at any position in the root's `/Kids`.
    let destination =
        LopdfDocument::from_lopdf(support::pdf_with_a_nested_page_tree(&["D1", "D2", "D3"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1"]));

    let result = grafted(&destination, 2, &source, &[0]);

    assert_eq!(support::labels(&result), vec!["D1", "D2", "S1", "D3"]);
}

#[test]
fn grafting_into_a_nested_destination_keeps_the_page_counts_honest() {
    let destination =
        LopdfDocument::from_lopdf(support::pdf_with_a_nested_page_tree(&["D1", "D2", "D3"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1", "S2"]));

    let result = grafted(&destination, 1, &source, &[0, 1]);

    assert_eq!(
        root_count(&result),
        5,
        "/Count is the number of pages in the subtree, not the number of kids"
    );
    assert_eq!(result.as_lopdf().get_pages().len(), 5);
}

#[test]
fn appending_past_the_last_page_of_a_nested_destination_lands_at_the_end() {
    let destination =
        LopdfDocument::from_lopdf(support::pdf_with_a_nested_page_tree(&["D1", "D2", "D3"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1"]));

    let result = grafted(&destination, 99, &source, &[0]);

    assert_eq!(support::labels(&result), vec!["D1", "D2", "D3", "S1"]);
    assert_eq!(root_count(&result), 4);
}

/// Structure assertions can pass on a document no reader will open: the
/// grafted page's `/Parent` has to be the node that actually lists it, or a
/// reader walking back up resolves inheritance against the wrong branch.
#[test]
fn a_grafted_page_is_parented_to_the_node_that_lists_it() {
    let destination =
        LopdfDocument::from_lopdf(support::pdf_with_a_nested_page_tree(&["D1", "D2", "D3"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1"]));

    let result = grafted(&destination, 2, &source, &[0]);

    let imported = page_at(&result, 3);
    let parent = result
        .as_lopdf()
        .get_dictionary(imported)
        .expect("page dict")
        .get(b"Parent")
        .and_then(|value| value.as_reference())
        .expect("a grafted page has to be parented");
    let kids = result
        .as_lopdf()
        .get_dictionary(parent)
        .expect("parent dict")
        .get(b"Kids")
        .and_then(|value| value.as_array())
        .expect("parent kids")
        .iter()
        .filter_map(|value| value.as_reference().ok())
        .collect::<Vec<_>>();
    assert!(
        kids.contains(&imported),
        "the page's /Parent must be the node whose /Kids lists it"
    );
}

#[test]
fn a_nested_graft_serializes_and_reloads_with_every_page_intact() {
    let destination =
        LopdfDocument::from_lopdf(support::pdf_with_a_nested_page_tree(&["D1", "D2", "D3"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1"]));

    let result = grafted(&destination, 1, &source, &[0]);

    let mut bytes = Vec::new();
    result
        .into_lopdf()
        .save_to(&mut bytes)
        .expect("a grafted document has to serialize");
    let reloaded = LopdfDocument::from_lopdf(Document::load_mem(&bytes).expect("it has to reload"));

    assert_eq!(support::labels(&reloaded), vec!["D1", "S1", "D2", "D3"]);
}

#[test]
fn a_blank_page_inserted_into_a_nested_destination_lands_at_the_page_index() {
    let document =
        LopdfDocument::from_lopdf(support::pdf_with_a_nested_page_tree(&["D1", "D2", "D3"]));

    let result = insert_blank_page(&document, 2, PageSize::A4, Orientation::Portrait)
        .expect("insert should succeed");

    assert_eq!(result.as_lopdf().get_pages().len(), 4);
    assert_eq!(root_count(&result), 4);
    // The blank page has no `Tj`, so it is the one position `labels` cannot
    // read: everything around it kept its order.
    let labelled: Vec<String> = [1, 2, 4]
        .into_iter()
        .map(|number| support::page_label(result.as_lopdf(), page_at(&result, number)))
        .collect();
    assert_eq!(labelled, vec!["D1", "D2", "D3"]);
}

#[test]
fn removing_a_page_from_a_nested_destination_leaves_the_counts_honest() {
    let document =
        LopdfDocument::from_lopdf(support::pdf_with_a_nested_page_tree(&["D1", "D2", "D3"]));

    let result = remove_page(&document, 1).expect("remove should succeed");

    assert_eq!(support::labels(&result), vec!["D1", "D3"]);
    assert_eq!(root_count(&result), 2);
}
