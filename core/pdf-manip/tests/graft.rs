//! Integration tests for `graft_pages` (TDD): copying real pages from one
//! document into another without rasterizing them, and without dragging the
//! source's catalog, page tree or unrelated pages along.

// The encrypted-fixture builder belongs to the permission tests that share
// this helper module.
#[allow(dead_code)]
mod support;

use lopdf::Document;
use pdf_manip::{graft_pages, LopdfDocument};

#[test]
fn graft_pages_inserts_the_source_page_content_at_the_requested_index() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1", "D2"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1", "S2", "S3"]));

    let result = graft_pages(&destination, 1, &source, &[2]).expect("graft should succeed");

    assert_eq!(support::labels(&result), vec!["D1", "S3", "D2"]);
}

#[test]
fn graft_pages_keeps_the_selection_order_and_lands_contiguously() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1", "S2", "S3"]));

    let result = graft_pages(&destination, 0, &source, &[2, 0]).expect("graft should succeed");

    assert_eq!(support::labels(&result), vec!["S3", "S1", "D1"]);
}

#[test]
fn graft_pages_appends_when_the_index_is_past_the_last_page() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1"]));

    let result = graft_pages(&destination, 99, &source, &[0]).expect("graft should succeed");

    assert_eq!(support::labels(&result), vec!["D1", "S1"]);
}

#[test]
fn graft_pages_survives_overlapping_object_ids_between_the_two_documents() {
    // Both fixtures are built the same way, so their object ids overlap
    // exactly — the case a naive copy silently corrupts.
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1", "D2"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1", "S2"]));

    let result = graft_pages(&destination, 2, &source, &[0, 1]).expect("graft should succeed");

    assert_eq!(support::labels(&result), vec!["D1", "D2", "S1", "S2"]);
}

#[test]
fn graft_pages_materializes_attributes_the_source_page_only_inherited() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_inherited_attributes(&["S1"]));

    let result = graft_pages(&destination, 1, &source, &[0]).expect("graft should succeed");

    let grafted = *result.as_lopdf().get_pages().get(&2).expect("second page");
    let page = result
        .as_lopdf()
        .get_dictionary(grafted)
        .expect("page dict");
    let media_box: Vec<i64> = page
        .get(b"MediaBox")
        .and_then(|value| value.as_array())
        .expect("grafted page carries its own MediaBox")
        .iter()
        .map(|value| value.as_i64().expect("integer"))
        .collect();
    assert_eq!(media_box, vec![0, 0, 300, 400]);
    assert_eq!(
        page.get(b"Rotate").and_then(|value| value.as_i64()).ok(),
        Some(90),
        "inherited rotation has to survive the reparent"
    );
    assert!(
        page.get(b"Resources").is_ok(),
        "inherited resources have to survive the reparent"
    );
}

#[test]
fn graft_pages_copies_the_object_graph_the_page_reaches() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_inherited_attributes(&["S1"]));

    let result = graft_pages(&destination, 1, &source, &[0]).expect("graft should succeed");

    // The font lives two hops from the page: page -> Resources -> Font -> F9.
    assert!(
        result
            .as_lopdf()
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .any(|dict| {
                dict.get(b"BaseFont").and_then(|value| value.as_name()).ok()
                    == Some(b"Courier".as_slice())
            }),
        "the source page's font must come along with it"
    );
}

#[test]
fn graft_pages_reparents_the_page_onto_the_destination_page_tree() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1"]));

    let result = graft_pages(&destination, 1, &source, &[0]).expect("graft should succeed");

    let lopdf = result.as_lopdf();
    let root_pages = lopdf
        .catalog()
        .expect("catalog")
        .get(b"Pages")
        .and_then(|value| value.as_reference())
        .expect("pages root");
    let grafted = *lopdf.get_pages().get(&2).expect("second page");
    assert_eq!(
        lopdf
            .get_dictionary(grafted)
            .expect("page dict")
            .get(b"Parent")
            .and_then(|value| value.as_reference())
            .ok(),
        Some(root_pages),
    );
}

#[test]
fn graft_pages_leaves_the_source_catalog_and_page_tree_behind() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1"]));

    let result = graft_pages(&destination, 1, &source, &[0]).expect("graft should succeed");

    let lopdf = result.as_lopdf();
    let count = |name: &[u8]| {
        lopdf
            .objects
            .values()
            .filter(|object| object.type_name().unwrap_or_default() == name)
            .count()
    };
    assert_eq!(
        count(b"Catalog"),
        1,
        "the source catalog must not come along"
    );
    assert_eq!(
        count(b"Pages"),
        1,
        "the source page tree must not come along"
    );
}

#[test]
fn graft_pages_carries_the_page_annotations() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_a_link_to_its_second_page());

    let result = graft_pages(&destination, 1, &source, &[0]).expect("graft should succeed");

    let grafted = *result.as_lopdf().get_pages().get(&2).expect("second page");
    let annots = result
        .as_lopdf()
        .get_dictionary(grafted)
        .expect("page dict")
        .get(b"Annots")
        .and_then(|value| value.as_array())
        .expect("the page's annotations come with it")
        .clone();
    assert_eq!(annots.len(), 1);
    let annotation = annots[0].as_reference().expect("indirect annotation");
    assert!(
        result.as_lopdf().get_dictionary(annotation).is_ok(),
        "the annotation object itself has to be copied, not just referenced"
    );
}

#[test]
fn graft_pages_does_not_follow_a_reference_into_an_unselected_page() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_a_link_to_its_second_page());

    let result = graft_pages(&destination, 1, &source, &[0]).expect("graft should succeed");

    assert_eq!(support::labels(&result), vec!["D1", "Linked"]);
    assert_eq!(
        support::page_object_count(&result),
        2,
        "the link's destination page must not be dragged in behind it"
    );
}

#[test]
fn graft_pages_rejects_an_empty_selection() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1"]));

    let error = graft_pages(&destination, 0, &source, &[]).expect_err("empty selection");

    assert_eq!(error.to_string(), "page selection must not be empty");
}

#[test]
fn graft_pages_rejects_a_page_the_source_does_not_have() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1"]));

    let error = graft_pages(&destination, 0, &source, &[0, 7]).expect_err("page 7 does not exist");

    assert_eq!(error.to_string(), "invalid page index: 7");
    assert_eq!(
        support::labels(&destination),
        vec!["D1"],
        "a refused graft leaves the destination exactly as it was"
    );
}

#[test]
fn graft_pages_rejects_the_same_source_page_twice() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1", "S2"]));

    let error = graft_pages(&destination, 0, &source, &[1, 0, 1]).expect_err("duplicate page");

    assert_eq!(
        error.to_string(),
        "page 1 appears more than once in the selection"
    );
}

/// Structure assertions can all pass on a document no reader will open. This
/// is the one that actually writes the grafted file out and reads it back.
#[test]
fn a_grafted_document_serializes_and_reloads_with_every_page_intact() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1", "D2"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_inherited_attributes(&["S1", "S2"]));

    let result = graft_pages(&destination, 1, &source, &[1, 0]).expect("graft should succeed");

    let mut bytes = Vec::new();
    result
        .into_lopdf()
        .save_to(&mut bytes)
        .expect("a grafted document has to serialize");
    let reloaded = LopdfDocument::from_lopdf(
        Document::load_mem(&bytes).expect("a grafted document has to reload"),
    );

    assert_eq!(support::labels(&reloaded), vec!["D1", "S2", "S1", "D2"]);
    // The imported pages kept their own geometry rather than inheriting the
    // destination's — the point of materializing before the reparent.
    let imported = *reloaded
        .as_lopdf()
        .get_pages()
        .get(&2)
        .expect("second page");
    let media_box: Vec<i64> = reloaded
        .as_lopdf()
        .get_dictionary(imported)
        .expect("page dict")
        .get(b"MediaBox")
        .and_then(|value| value.as_array())
        .expect("MediaBox survived the round trip")
        .iter()
        .map(|value| value.as_i64().expect("integer"))
        .collect();
    assert_eq!(media_box, vec![0, 0, 300, 400]);
}

/// The shallow cases (a font one hop away, an annotation on the page) can
/// pass while everything an image actually needs is left behind, because
/// those references live inside a *stream's* dictionary. This walks the whole
/// chain: page -> XObject -> image -> SMask, and image -> indexed ColorSpace
/// -> lookup table.
#[test]
fn graft_pages_copies_resources_nested_inside_stream_dictionaries() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_nested_page_resources("S1"));

    let result = graft_pages(&destination, 1, &source, &[0]).expect("graft should succeed");

    let lopdf = result.as_lopdf();
    let grafted = *lopdf.get_pages().get(&2).expect("second page");
    let resources = lopdf
        .get_dictionary(grafted)
        .expect("page dict")
        .get(b"Resources")
        .and_then(|value| value.as_reference())
        .expect("resources are an indirect object");
    let image = lopdf
        .get_dictionary(resources)
        .expect("resources dict")
        .get(b"XObject")
        .and_then(|value| value.as_dict())
        .expect("XObject dict")
        .get(b"Im1")
        .and_then(|value| value.as_reference())
        .expect("the image reference survived");
    let image_dict = lopdf
        .get_object(image)
        .and_then(|object| object.as_stream())
        .expect("the image stream itself was copied")
        .dict
        .clone();

    let smask = image_dict
        .get(b"SMask")
        .and_then(|value| value.as_reference())
        .expect("soft mask reference");
    assert!(
        lopdf.get_object(smask).is_ok(),
        "the soft-mask image hangs off the image stream's own dictionary"
    );

    let colorspace = image_dict
        .get(b"ColorSpace")
        .and_then(|value| value.as_reference())
        .expect("colour space reference");
    let lookup = lopdf
        .get_object(colorspace)
        .and_then(|object| object.as_array())
        .expect("indexed colour space array")[3]
        .as_reference()
        .expect("lookup table reference");
    assert!(
        lopdf.get_object(lookup).is_ok(),
        "an indexed colour space is useless without its lookup table"
    );
}
