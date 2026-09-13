//! `graft_pages` and digital signatures (checklist "Seguridad y firmas",
//! `docs/batch-pdf-assembly.md` section 5, items 7 and 10).
//!
//! Two different answers for two different facts, and the split is the point:
//!
//! - a selected page carrying a **signature widget** is **refused**. The
//!   widget is the appearance; the signature dictionary and the byte range it
//!   covers stay in the source. An imported page showing "signed by …" with
//!   nothing behind it is worse than one that lost the signature.
//! - a **signed source** whose selected pages carry no such widget is
//!   **reported**. Those pages arrive whole; what does not arrive is any of
//!   the signing, and the user is told before they commit.

#[allow(dead_code)]
mod support;

use pdf_manip::{graft_pages, graft_report, GraftWarning, LopdfDocument};

fn destination() -> LopdfDocument {
    LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["D1"]))
}

fn signed_source() -> LopdfDocument {
    LopdfDocument::from_lopdf(support::structures::pdf_signed_on_its_first_page())
}

#[test]
fn graft_pages_refuses_a_selected_page_that_carries_a_signature() {
    let destination = destination();
    let source = signed_source();

    let error = graft_pages(&destination, 0, &source, &[0]).expect_err("page 0 is signed");

    let message = error.to_string();
    assert!(message.contains("page 0"), "{message}");
    assert!(message.contains("signature"), "{message}");
    assert_eq!(
        support::labels(&destination),
        vec!["D1"],
        "a refused graft leaves the destination exactly as it was"
    );
}

/// A signature is a form field, so the general widget check would also have
/// matched. Telling the user their page "has form fields" when what it really
/// has is somebody's signature hides the only fact that mattered — the more
/// specific refusal has to win.
#[test]
fn a_signature_is_refused_as_a_signature_and_not_as_a_form_field() {
    let error = graft_pages(&destination(), 0, &signed_source(), &[0]).expect_err("page 0");

    assert!(
        !error.to_string().contains("form fields"),
        "a signature must not be reported as a generic form field: {error}"
    );
}

/// The gate a shell calls at selection time refuses exactly what the import
/// refuses, so a user cannot be told an import is fine and then have it fail.
#[test]
fn graft_report_refuses_the_signed_page_the_import_refuses() {
    let source = signed_source();

    let reported = graft_report(&source, &[0]).expect_err("page 0 is signed");
    let grafted = graft_pages(&destination(), 0, &source, &[0]).expect_err("page 0 is signed");

    assert_eq!(reported.to_string(), grafted.to_string());
}

#[test]
fn an_unsigned_page_of_a_signed_source_imports_and_warns() {
    let destination = destination();
    let source = signed_source();

    let outcome = graft_pages(&destination, 1, &source, &[1]).expect("page 1 carries no widget");

    assert_eq!(support::labels(&outcome.document), vec!["D1", "Plain"]);
    assert!(
        outcome
            .report
            .warnings()
            .contains(&GraftWarning::SourceSignaturesNotImported),
        "a signed source must say so: {:?}",
        outcome.report.warnings()
    );
    assert!(!outcome.report.is_lossless());
}

/// The warning names the cost in the user's terms — that the pages come but
/// the signature does not — without claiming the source file was harmed.
#[test]
fn the_signature_warning_explains_what_did_not_travel() {
    let message = GraftWarning::SourceSignaturesNotImported.to_string();

    assert!(message.contains("signed"), "{message}");
    assert!(message.contains("signature"), "{message}");
}

#[test]
fn an_unsigned_source_raises_no_signature_warning() {
    let source = LopdfDocument::from_lopdf(support::build_pdf_with_pages(&["A", "B"]));

    let report = graft_report(&source, &[0]).expect("a plain page imports cleanly");

    assert!(report.is_lossless(), "{:?}", report.warnings());
}
