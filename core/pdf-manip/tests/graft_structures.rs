//! The rest of what the source catalog owned (checklist "Estructuras de
//! documento", `docs/batch-pdf-assembly.md` section 4, items 4 and 7-9).
//!
//! Each structure gets one of two answers and never a third. Optional content
//! is **refused**, because importing it produces wrong output rather than
//! merely poorer output. Outlines and tagged structure are **reported**,
//! because refusing them would refuse most real documents. What no structure
//! gets is silence — which is what the last test pins: a lossless import is
//! observably lossless, so a report with warnings in it means something.

#[allow(dead_code)]
mod support;

use pdf_manip::{graft_pages, graft_report, GraftWarning, LopdfDocument};

use support::structures;

#[test]
fn a_page_using_optional_content_is_refused() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(structures::pdf_with_optional_content());

    let error = graft_pages(&destination, 1, &source, &[0]).expect_err("page 0 has a layer");

    assert_eq!(
        error.to_string(),
        "page 0 uses optional content (layers); importing layered content is not supported yet"
    );
    assert_eq!(
        support::labels(&destination),
        vec!["D1"],
        "a refused graft leaves the destination exactly as it was"
    );
}

#[test]
fn optional_content_is_refused_before_the_import_is_attempted() {
    let source = LopdfDocument::from_lopdf(structures::pdf_with_optional_content());

    let error = graft_report(&source, &[0]).expect_err("page 0 has a layer");

    assert_eq!(
        error.to_string(),
        "page 0 uses optional content (layers); importing layered content is not supported yet"
    );
}

#[test]
fn a_tagged_page_imports_and_reports_the_structure_it_leaves_behind() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(structures::pdf_with_tagged_pages());

    let outcome = graft_pages(&destination, 1, &source, &[0]).expect("a tagged page still imports");

    assert_eq!(
        outcome.report.warnings(),
        [GraftWarning::TaggedStructureNotImported { page: 0 }],
        "refusing every tagged document would refuse almost every office PDF"
    );
    assert_eq!(support::labels(&outcome.document), vec!["D1", "First"]);
}

#[test]
fn each_imported_tagged_page_is_reported_on_its_own() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(structures::pdf_with_tagged_pages());

    let outcome = graft_pages(&destination, 1, &source, &[0, 1]).expect("both pages import");

    assert_eq!(
        outcome.report.warnings(),
        [
            GraftWarning::TaggedStructureNotImported { page: 0 },
            GraftWarning::TaggedStructureNotImported { page: 1 },
        ]
    );
}

#[test]
fn only_the_bookmarks_pointing_into_the_selection_are_reported() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(structures::pdf_with_an_outline_over_both_pages());

    let outcome = graft_pages(&destination, 1, &source, &[0]).expect("page 0 should graft");

    assert_eq!(
        outcome.report.warnings(),
        [GraftWarning::OutlinesNotImported { entries: 1 }],
        "the entry for the page nobody imported is not this import's business"
    );
}

#[test]
fn importing_every_page_reports_every_bookmark() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(structures::pdf_with_an_outline_over_both_pages());

    let outcome = graft_pages(&destination, 1, &source, &[0, 1]).expect("both pages should graft");

    assert_eq!(
        outcome.report.warnings(),
        [GraftWarning::OutlinesNotImported { entries: 2 }],
        "the whole outline pointed into the selection, so the whole outline is lost"
    );
}

#[test]
fn an_ordinary_import_reports_nothing_at_all() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["S1", "S2"]));

    let outcome = graft_pages(&destination, 1, &source, &[0]).expect("graft should succeed");

    assert!(
        outcome.report.is_lossless(),
        "a lossless import has to be observably lossless, or a warning means nothing"
    );
}
