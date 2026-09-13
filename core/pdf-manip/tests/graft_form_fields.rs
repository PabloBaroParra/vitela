//! `graft_pages` and AcroForm fields (checklist "Estructuras de documento",
//! `docs/batch-pdf-assembly.md` section 4, items "conservar widgets y
//! apariencias").
//!
//! A form field is not one object, it is three: the widget on the page, the
//! field dictionary behind it, and the `/AcroForm` in the catalog that lists
//! the field and holds the defaults it inherits. A graft copies the page and
//! refuses the catalog, so these tests exist to pin the part that has to be
//! rebuilt rather than copied — and to pin that nothing is rebuilt when the
//! imported pages carry no field at all.
//!
//! Name collisions between an imported field and one the destination already
//! has live in `tests/graft_form_names.rs`.

#[allow(dead_code)]
mod support;

use pdf_manip::{graft_pages, LopdfDocument};
use support::{form_reads, forms};

#[test]
fn an_imported_field_is_listed_in_the_destination_form() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Name", "Ada"));

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("a page with a form field should graft")
        .document;

    assert_eq!(support::labels(&result), vec!["D1", "Form"]);
    assert_eq!(
        form_reads::form_field_names(&result),
        vec!["Name"],
        "the field has to be listed in the destination's /AcroForm, or the \
         widget on the page is a box that only looks like a field"
    );
    let field = form_reads::form_field(&result, "Name").expect("the imported field");
    assert_eq!(
        form_reads::text(&field, b"V"),
        "Ada",
        "the value travels with it"
    );
}

#[test]
fn importing_a_page_without_fields_creates_no_form() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Name", "Ada"));

    let result = graft_pages(&destination, 1, &source, &[1])
        .expect("the plain page should graft")
        .document;

    assert_eq!(support::labels(&result), vec!["D1", "Plain"]);
    assert!(
        !form_reads::has_acroform(&result),
        "a destination that had no form must not grow an empty one"
    );
}

#[test]
fn an_imported_widget_keeps_its_appearance_and_its_page() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Name", "Ada"));

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("a page with a form field should graft")
        .document;

    let widgets = form_reads::page_widgets(&result, 1);
    assert_eq!(widgets.len(), 1, "the widget stays on the page it drew on");
    let appearance = widgets[0]
        .get(b"AP")
        .and_then(|ap| ap.as_dict())
        .expect("the widget keeps its /AP")
        .get(b"N")
        .and_then(|normal| normal.as_reference())
        .expect("/AP /N is an indirect stream");
    assert!(
        result.as_lopdf().get_object(appearance).is_ok(),
        "the appearance stream itself has to come along, not just the /AP that names it"
    );
}

#[test]
fn an_imported_field_carries_the_defaults_it_inherited_from_the_form() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_field_inheriting_the_form_defaults("Name"));

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("a page with a form field should graft")
        .document;

    let field = form_reads::form_field(&result, "Name").expect("the imported field");
    assert_eq!(
        form_reads::text(&field, b"DA"),
        forms::DEFAULT_APPEARANCE,
        "the /DA it read off the source's /AcroForm has to be written onto the \
         field, because that /AcroForm is not coming along"
    );
    assert_eq!(
        field.get(b"Q").and_then(|q| q.as_i64()).ok(),
        Some(1),
        "so does the quadding it inherited the same way"
    );
}

#[test]
fn a_widget_with_no_appearance_asks_the_destination_to_build_one() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(
        forms::pdf_with_a_widget_that_needs_its_appearance_built("Name"),
    );

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("a page with a form field should graft")
        .document;

    assert_eq!(
        form_reads::acroform(&result)
            .get(b"NeedAppearances")
            .and_then(|flag| flag.as_bool())
            .ok(),
        Some(true),
        "the source relied on /NeedAppearances and that flag lives in the \
         catalog it leaves behind; without it the field renders as an empty box"
    );
}

#[test]
fn a_field_keeps_only_the_widgets_whose_pages_were_imported() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(forms::pdf_with_one_field_over_two_pages("Name"));

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("a page with a form field should graft")
        .document;

    let field = form_reads::form_field(&result, "Name").expect("the imported field");
    let kids = field
        .get(b"Kids")
        .and_then(|kids| kids.as_array())
        .expect("the field keeps its /Kids");
    assert_eq!(
        kids.len(),
        1,
        "the widget on the page that was left behind would be a kid with no \
         page to draw on, sharing the field's value from nowhere"
    );
    let kid = kids[0].as_reference().expect("a kid is indirect");
    assert!(
        result.as_lopdf().get_object(kid).is_ok(),
        "the kid that stayed has to still be a real object"
    );
    assert_eq!(form_reads::page_widgets(&result, 1).len(), 1);
}

#[test]
fn a_kid_widget_points_back_at_the_field_that_came_with_it() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(forms::pdf_with_one_field_over_two_pages("Name"));

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("a page with a form field should graft")
        .document;

    let widget = form_reads::page_widgets(&result, 1)
        .first()
        .cloned()
        .expect("the imported widget");
    let parent = widget
        .get(b"Parent")
        .and_then(|parent| parent.as_reference())
        .expect("a kid widget keeps its /Parent");
    let field = result
        .as_lopdf()
        .get_dictionary(parent)
        .expect("the /Parent resolves inside the destination");
    assert_eq!(
        form_reads::text(field, b"T"),
        "Name",
        "a widget whose /Parent dangles is a widget with no field, no value \
         and no name"
    );
}

#[test]
fn the_destination_keeps_the_fields_it_already_had() {
    let destination =
        LopdfDocument::from_lopdf(forms::destination_with_a_form_field("Existing", "kept"));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Name", "Ada"));

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("a page with a form field should graft")
        .document;

    let mut names = form_reads::form_field_names(&result);
    names.sort();
    assert_eq!(names, vec!["Existing", "Name"]);
    let existing =
        form_reads::form_field(&result, "Existing").expect("the destination's own field");
    assert_eq!(form_reads::text(&existing, b"V"), "kept");
}

#[test]
fn a_default_resource_the_destination_lacks_is_brought_over() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Name", "Ada"));

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("a page with a form field should graft")
        .document;

    assert_eq!(
        form_reads::default_resource_fonts(&result),
        vec![("Helv".to_string(), "Helvetica".to_string())],
        "/DA names a resource, and a resource name with no /DR entry behind it \
         is what makes a regenerated appearance fall back to nothing"
    );
    let field = form_reads::form_field(&result, "Name").expect("the imported field");
    assert_eq!(form_reads::text(&field, b"DA"), forms::DEFAULT_APPEARANCE);
}

#[test]
fn an_xfa_form_is_refused() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(forms::pdf_with_an_xfa_form("Name"));

    let error = graft_pages(&destination, 1, &source, &[0]).expect_err("page 0 is an XFA form");

    assert_eq!(
        error.to_string(),
        "page 0 belongs to an XFA form, whose definition cannot be imported; \
         its fields would look right and behave differently"
    );
    assert!(
        support::labels(&destination) == vec!["D1"],
        "a refused graft leaves the destination exactly as it was"
    );
}

#[test]
fn a_signature_widget_is_still_refused() {
    let destination = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]));
    let source = LopdfDocument::from_lopdf(support::structures::pdf_signed_on_its_first_page());

    let error = graft_pages(&destination, 1, &source, &[0]).expect_err("page 0 is signed");

    assert!(
        error.to_string().contains("signature"),
        "accepting ordinary fields must not quietly accept signature fields: {error}"
    );
}
