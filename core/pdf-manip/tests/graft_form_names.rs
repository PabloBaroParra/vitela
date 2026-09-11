//! Name collisions when an imported form meets a form that is already there
//! (checklist "Estructuras de documento", `docs/batch-pdf-assembly.md`
//! section 4, item "resolver colisiones de nombres de campos sin fusionarlos
//! silenciosamente").
//!
//! Two things in a form are addressed by name rather than by object, and both
//! collide:
//!
//! - a **field name** (`/T`). Two top-level fields sharing one `/T` are not
//!   two fields in PDF 32000-1:2008 section 12.7.3.2 — they are one field
//!   with two widgets, and they share a value. Appending a colliding import
//!   to `/Fields` would therefore not import a field, it would overwrite the
//!   destination's. That is the silent merge this file exists to forbid.
//! - a **default resource name** (`/DR`). A field's `/DA` names a font by
//!   resource name; if the destination binds that name to a different font,
//!   the imported field keeps its text and quietly changes typeface the first
//!   time a viewer regenerates its appearance.
//!
//! Neither is refused — a rename loses nothing — but a field rename is
//! visible to whoever fills the form, so it is reported.

#[allow(dead_code)]
mod support;

use pdf_manip::{graft_pages, GraftWarning, LopdfDocument};
use support::{form_reads, forms};

#[test]
fn a_colliding_field_name_is_renamed_instead_of_merged() {
    let destination =
        LopdfDocument::from_lopdf(forms::destination_with_a_form_field("Name", "theirs"));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Name", "ours"));

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("a colliding name is resolved, not refused")
        .document;

    let mut names = form_reads::form_field_names(&result);
    names.sort();
    assert_eq!(names, vec!["Name", "Name-imported"]);
    assert_eq!(
        form_reads::field_value(&result, "Name"),
        "theirs",
        "the field that was already there must not take the imported value"
    );
    assert_eq!(
        form_reads::field_value(&result, "Name-imported"),
        "ours",
        "and the imported one must keep its own"
    );
}

#[test]
fn a_renamed_field_is_reported() {
    let destination =
        LopdfDocument::from_lopdf(forms::destination_with_a_form_field("Name", "theirs"));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Name", "ours"));

    let outcome = graft_pages(&destination, 1, &source, &[0]).expect("the import should succeed");

    assert_eq!(
        outcome.report.warnings(),
        [GraftWarning::FormFieldRenamed {
            page: 0,
            from: "Name".into(),
            to: "Name-imported".into(),
        }],
        "renaming somebody's field behind their back is exactly the kind of \
         quiet change the report exists to surface"
    );
    assert!(!outcome.report.is_lossless());
}

#[test]
fn a_second_collision_takes_the_next_free_name() {
    let destination =
        LopdfDocument::from_lopdf(forms::destination_with_a_form_field("Name", "theirs"));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Name", "ours"));

    let once = graft_pages(&destination, 1, &source, &[0])
        .expect("the first import")
        .document;
    let outcome = graft_pages(&once, 2, &source, &[0]).expect("the second import");

    let mut names = form_reads::form_field_names(&outcome.document);
    names.sort();
    assert_eq!(names, vec!["Name", "Name-imported", "Name-imported-2"]);
    assert_eq!(
        outcome.report.warnings(),
        [GraftWarning::FormFieldRenamed {
            page: 0,
            from: "Name".into(),
            to: "Name-imported-2".into(),
        }]
    );
}

#[test]
fn a_name_the_destination_does_not_have_is_left_alone() {
    let destination =
        LopdfDocument::from_lopdf(forms::destination_with_a_form_field("Theirs", "theirs"));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Ours", "ours"));

    let outcome = graft_pages(&destination, 1, &source, &[0]).expect("no collision here");

    let mut names = form_reads::form_field_names(&outcome.document);
    names.sort();
    assert_eq!(names, vec!["Ours", "Theirs"]);
    assert!(
        outcome.report.is_lossless(),
        "an import that changed nobody's field name has nothing to report: {:?}",
        outcome.report.warnings()
    );
}

#[test]
fn a_colliding_default_resource_is_rebound_and_the_field_follows_it() {
    let destination =
        LopdfDocument::from_lopdf(forms::destination_with_a_form_field("Theirs", "theirs"));
    let source =
        LopdfDocument::from_lopdf(forms::pdf_with_a_form_field_on_first_page("Ours", "ours"));

    let result = graft_pages(&destination, 1, &source, &[0])
        .expect("the import should succeed")
        .document;

    assert_eq!(
        form_reads::default_resource_fonts(&result),
        vec![
            ("Helv".to_string(), "Courier".to_string()),
            ("Helv-imported".to_string(), "Helvetica".to_string()),
        ],
        "both fonts have to survive: overwriting /Helv would restyle every \
         field the destination already had"
    );
    let field = form_reads::form_field(&result, "Ours").expect("the imported field");
    assert_eq!(
        form_reads::text(&field, b"DA"),
        "/Helv-imported 0 Tf 0 g",
        "a /DA still naming /Helv would resolve to the destination's Courier"
    );
    assert_eq!(
        form_reads::text(
            &form_reads::form_field(&result, "Theirs").expect("the destination's field"),
            b"DA",
        ),
        forms::DEFAULT_APPEARANCE,
        "the destination's own field keeps naming the resource it always named"
    );
}

#[test]
fn the_rename_warning_names_both_names_and_the_page() {
    let message = GraftWarning::FormFieldRenamed {
        page: 3,
        from: "Signature Date".into(),
        to: "Signature Date-imported".into(),
    }
    .to_string();

    assert!(message.contains("page 3"), "{message}");
    assert!(message.contains("\"Signature Date\""), "{message}");
    assert!(message.contains("\"Signature Date-imported\""), "{message}");
}
