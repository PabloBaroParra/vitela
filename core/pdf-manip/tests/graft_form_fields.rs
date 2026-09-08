//! `graft_pages` and AcroForm widgets (checklist "Estructuras de documento",
//! `docs/batch-pdf-assembly.md` section 4): a selected page carrying a
//! widget annotation is refused rather than imported, because merging it
//! into the destination's `/AcroForm` needs a name-collision policy this
//! crate does not implement yet.

#[allow(dead_code)]
mod support;

use pdf_manip::{graft_pages, LopdfDocument};

#[test]
fn graft_pages_rejects_a_selected_page_with_a_form_field_widget() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_a_widget_on_first_page());

    let error = graft_pages(&destination, 0, &source, &[0]).expect_err("page 0 has a widget");

    assert_eq!(
        error.to_string(),
        "page 0 has form fields; importing AcroForm fields is not supported yet"
    );
    assert_eq!(
        support::labels(&destination),
        vec!["D1"],
        "a refused graft leaves the destination exactly as it was"
    );
}

#[test]
fn graft_pages_allows_a_source_whose_unselected_page_has_form_fields() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::pdf_with_a_widget_on_first_page());

    let result = graft_pages(&destination, 1, &source, &[1])
        .expect("plain page should graft")
        .document;

    assert_eq!(support::labels(&result), vec!["D1", "Plain"]);
}
