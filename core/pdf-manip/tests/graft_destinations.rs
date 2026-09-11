//! Where an imported link ends up pointing (checklist "Estructuras de
//! documento", `docs/batch-pdf-assembly.md` section 4, items 4-6).
//!
//! Three outcomes, and the tests below pin all three: a destination into the
//! selection keeps working, a **named** destination into the selection is
//! rewritten into the explicit one it meant, and a destination that leaves
//! the selection is reported rather than quietly left dead.

#[allow(dead_code)]
mod support;

use lopdf::{Object, ObjectId};
use pdf_manip::{graft_pages, graft_report, GraftWarning, LopdfDocument};

use support::structures;

/// The object id of the 1-indexed page `number` in `document`.
fn page_id(document: &LopdfDocument, number: u32) -> ObjectId {
    *document
        .as_lopdf()
        .get_pages()
        .get(&number)
        .expect("page number is in the document")
}

/// The destination a page's single link annotation points at, read back
/// through whichever of `/Dest` or `/A /D` the fixture used.
fn link_destination(document: &LopdfDocument, page_number: u32) -> Object {
    let doc = document.as_lopdf();
    let annots = doc
        .get_dictionary(page_id(document, page_number))
        .expect("page dict")
        .get(b"Annots")
        .and_then(|value| value.as_array())
        .expect("the page has annotations")
        .clone();
    let annotation = doc
        .get_dictionary(annots[0].as_reference().expect("indirect annotation"))
        .expect("annotation dict");
    if let Ok(dest) = annotation.get(b"Dest") {
        return dest.clone();
    }
    let action = annotation.get(b"A").expect("a /Dest or an /A action");
    let action = match action {
        Object::Reference(id) => doc.get_dictionary(*id).expect("action dict").clone(),
        Object::Dictionary(dict) => dict.clone(),
        other => panic!("unexpected action shape: {other:?}"),
    };
    action.get(b"D").expect("the action's destination").clone()
}

/// The page an explicit destination array names.
fn explicit_target(destination: &Object) -> ObjectId {
    destination
        .as_array()
        .expect("an explicit destination array")
        .first()
        .expect("a destination array names a page first")
        .as_reference()
        .expect("the page is named by reference")
}

#[test]
fn an_explicit_destination_into_the_selection_still_points_at_its_page() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_a_link_to_its_second_page());

    let outcome = graft_pages(&destination, 1, &source, &[0, 1]).expect("both pages should graft");

    // Pages land at 2 and 3: the link is on the first of them, its target is
    // the second.
    assert_eq!(
        explicit_target(&link_destination(&outcome.document, 2)),
        page_id(&outcome.document, 3),
        "renumbering the source once keeps an explicit destination valid"
    );
    assert!(
        outcome.report.is_lossless(),
        "nothing was left behind: {:?}",
        outcome.report.warnings()
    );
}

#[test]
fn a_named_destination_into_the_selection_is_rewritten_as_an_explicit_one() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source =
        LopdfDocument::from_lopdf(structures::pdf_with_a_named_destination_to_its_second_page());

    let outcome = graft_pages(&destination, 1, &source, &[0, 1]).expect("both pages should graft");

    let rewritten = link_destination(&outcome.document, 2);
    assert_eq!(
        explicit_target(&rewritten),
        page_id(&outcome.document, 3),
        "the name tree stayed in the source, so the name had to become the page it meant"
    );
    let view = rewritten.as_array().expect("destination array");
    assert_eq!(
        view[1],
        Object::Name(b"XYZ".to_vec()),
        "the view parameters the name carried are kept, not replaced with a guess"
    );
    assert!(outcome.report.is_lossless());
}

#[test]
fn a_legacy_dests_dictionary_is_resolved_the_same_way() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(structures::pdf_with_a_legacy_named_destination());

    let outcome = graft_pages(&destination, 1, &source, &[0, 1]).expect("both pages should graft");

    assert_eq!(
        explicit_target(&link_destination(&outcome.document, 2)),
        page_id(&outcome.document, 3),
        "a PDF 1.1 /Dests dictionary is a name map too and must resolve"
    );
    assert!(outcome.report.is_lossless());
}

#[test]
fn a_destination_outside_the_selection_is_reported_and_the_page_still_imports() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_a_link_to_its_second_page());

    let outcome = graft_pages(&destination, 1, &source, &[0]).expect("page 0 should graft");

    assert_eq!(
        outcome.report.warnings(),
        [GraftWarning::LinkTargetNotImported { page: 0 }],
        "the link cannot work, and a caller has to be able to say so"
    );
    assert_eq!(support::labels(&outcome.document), vec!["D1", "Linked"]);
}

#[test]
fn a_named_destination_outside_the_selection_is_reported_too() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source =
        LopdfDocument::from_lopdf(structures::pdf_with_a_named_destination_to_its_second_page());

    let outcome = graft_pages(&destination, 1, &source, &[0]).expect("page 0 should graft");

    assert_eq!(
        outcome.report.warnings(),
        [GraftWarning::LinkTargetNotImported { page: 0 }],
        "a name that resolves out of the selection is the same loss as an explicit one"
    );
}

#[test]
fn a_name_the_source_never_defined_is_reported_as_dropped() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(structures::pdf_with_an_undefined_named_destination());

    let outcome = graft_pages(&destination, 1, &source, &[0]).expect("page 0 should graft");

    assert_eq!(
        outcome.report.warnings(),
        [GraftWarning::NamedDestinationDropped {
            page: 0,
            name: "ghost".into(),
        }]
    );
}

#[test]
fn asking_first_gives_the_same_answer_as_importing() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_a_link_to_its_second_page());

    let asked = graft_report(&source, &[0]).expect("the report should be readable");
    let outcome = graft_pages(&destination, 1, &source, &[0]).expect("page 0 should graft");

    assert_eq!(
        asked.warnings(),
        outcome.report.warnings(),
        "a gate that disagreed with the operation it gates would be worse than no gate"
    );
}

#[test]
fn asking_first_refuses_what_importing_refuses() {
    let source = LopdfDocument::from_lopdf(support::forms::pdf_with_an_xfa_form("Name"));

    let error = graft_report(&source, &[0]).expect_err("page 0 is an XFA form");

    assert_eq!(
        error.to_string(),
        "page 0 belongs to an XFA form, whose definition cannot be imported; \
         its fields would look right and behave differently"
    );
}
