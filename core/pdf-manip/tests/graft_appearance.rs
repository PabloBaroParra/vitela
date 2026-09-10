//! Integration tests (TDD) for what *draws* an imported annotation — the
//! `/AP` appearance streams (checklist "Pruebas del núcleo",
//! `docs/batch-pdf-assembly.md` section 12).
//!
//! Separate from `graft.rs` because it asks a different question. That file
//! asks whether the page's own content arrived; this asks whether what sits
//! on top of the page arrived with the instructions for painting it. The two
//! fail independently: `graft_pages_carries_the_page_annotations` passed for
//! a long time while nothing at all pinned `/AP`.

use pdf_manip::{graft_pages, LopdfDocument};

// Only a slice of the shared fixture module is used here.
#[allow(dead_code)]
mod support;

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

/// Carrying the annotation object is not the same as carrying what draws it.
/// `/AP` reaches its states through a dictionary the traversal has to enter,
/// and each state is a *stream* whose own `/Resources` sit one hop further
/// in. A copy that stops at the annotation dictionary leaves a valid-looking
/// annotation whose appearance every viewer then has to invent — the page
/// still renders, just never again as the author drew it.
#[test]
fn graft_pages_carries_an_annotations_appearance_streams() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(
        support::appearance::pdf_with_an_annotation_with_an_appearance_stream(),
    );

    let result = grafted(&destination, 1, &source, &[0]);

    let lopdf = result.as_lopdf();
    let grafted_page = *lopdf.get_pages().get(&2).expect("second page");
    let annotation = lopdf
        .get_dictionary(grafted_page)
        .expect("page dict")
        .get(b"Annots")
        .and_then(|value| value.as_array())
        .expect("the page's annotations come with it")[0]
        .as_reference()
        .expect("indirect annotation");
    let appearance = lopdf
        .get_dictionary(annotation)
        .expect("the annotation object was copied")
        .get(b"AP")
        .and_then(|value| value.as_dict())
        .expect("the appearance dictionary came with the annotation")
        .clone();

    // Both states, not just the one a viewer shows first.
    for state in [&b"N"[..], &b"R"[..]] {
        let stream_id = appearance
            .get(state)
            .and_then(|value| value.as_reference())
            .expect("appearance state is an indirect stream");
        assert!(
            lopdf
                .get_object(stream_id)
                .and_then(|object| object.as_stream())
                .is_ok(),
            "the appearance stream itself has to be copied, not just referenced"
        );
    }

    // The font lives inside the normal state's own dictionary — page ->
    // Annots -> AP -> N (a stream) -> Resources -> Font -> ApF.
    let normal = appearance
        .get(b"N")
        .and_then(|value| value.as_reference())
        .expect("normal state");
    let resources = lopdf
        .get_object(normal)
        .and_then(|object| object.as_stream())
        .expect("normal appearance stream")
        .dict
        .get(b"Resources")
        .and_then(|value| value.as_reference())
        .expect("the appearance stream's resources are an indirect object");
    let font = lopdf
        .get_dictionary(resources)
        .expect("appearance resources dict")
        .get(b"Font")
        .and_then(|value| value.as_dict())
        .expect("font dict")
        .get(b"ApF")
        .and_then(|value| value.as_reference())
        .expect("the appearance's font reference survived");
    assert!(
        lopdf.get_dictionary(font).is_ok(),
        "the font object the appearance draws with has to come along too"
    );
}
