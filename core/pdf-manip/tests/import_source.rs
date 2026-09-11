//! Integration tests for `open_import_source_from_bytes` — opening a PDF as
//! the *source* of an import, and refusing one whose permissions forbid
//! copying its pages out.
//!
//! This gate used to live in the Linux GTK4 shell, which meant every other
//! shell had to remember to write it again. The rule is the same one
//! `text_extraction_is_allowed` states (`/P` bit 5, PDF 1.7 table 22): taking
//! a page out of a document and putting it in another one is extraction,
//! whatever the destination does with it afterwards.

// The page-content helpers belong to the manipulation tests that share this
// module.
#[allow(dead_code)]
mod support;

use lopdf::Permissions;
use pdf_manip::{open_import_source_from_bytes, ManipError};

const USER_PASSWORD: &str = "user-no-copy";
const OWNER_PASSWORD: &str = "owner-no-copy";

/// Printing allowed, copying denied — the shape of source the gate exists for.
fn no_copy_pdf(user_password: &str) -> Vec<u8> {
    support::restricted_pdf(user_password, OWNER_PASSWORD, Permissions::PRINTABLE)
}

fn copyable_pdf(user_password: &str) -> Vec<u8> {
    support::restricted_pdf(
        user_password,
        OWNER_PASSWORD,
        Permissions::PRINTABLE | Permissions::COPYABLE,
    )
}

#[test]
fn an_unencrypted_source_opens_with_no_security_context() {
    let mut bytes = Vec::new();
    support::build_pdf_with_pages(&["S1", "S2"])
        .save_to(&mut bytes)
        .expect("save fixture");

    let (document, security) =
        open_import_source_from_bytes(&bytes, None).expect("an unencrypted source has to open");

    assert!(security.is_none());
    assert_eq!(document.as_lopdf().get_pages().len(), 2);
}

#[test]
fn a_source_that_forbids_copying_is_refused() {
    // Empty user password: it opens with no prompt at all and still forbids
    // copying — the single most common shape of restricted PDF, and the one
    // a gate that only looks at `is_encrypted` waves straight through.
    let bytes = no_copy_pdf("");

    let error = open_import_source_from_bytes(&bytes, None)
        .expect_err("a source that forbids copying must not open");

    assert!(matches!(error, ManipError::SourceForbidsCopying));
    assert_eq!(
        error.to_string(),
        "the PDF does not permit copying its pages"
    );
}

#[test]
fn a_source_that_forbids_copying_is_refused_behind_its_user_password() {
    let bytes = no_copy_pdf(USER_PASSWORD);

    let error = open_import_source_from_bytes(&bytes, Some(USER_PASSWORD))
        .expect_err("the user credential does not lift the restriction");

    assert!(matches!(error, ManipError::SourceForbidsCopying));
}

#[test]
fn the_owner_of_a_no_copy_source_may_still_import_it() {
    // The owner password authenticates the party that set the restriction in
    // the first place, so the bitmask does not apply to it.
    let bytes = no_copy_pdf(USER_PASSWORD);

    let (document, security) = open_import_source_from_bytes(&bytes, Some(OWNER_PASSWORD))
        .expect("the owner credential has to open its own document");

    assert_eq!(document.as_lopdf().get_pages().len(), 1);
    assert!(
        security.is_some(),
        "an encrypted source reports its context"
    );
}

#[test]
fn a_source_that_permits_copying_opens_with_its_pages_decrypted() {
    let bytes = copyable_pdf(USER_PASSWORD);

    let (document, security) = open_import_source_from_bytes(&bytes, Some(USER_PASSWORD))
        .expect("a copyable source has to open");

    assert!(security.is_some());
    // Decrypted, not merely loaded: the page content has to read back.
    let page = *document
        .as_lopdf()
        .get_pages()
        .get(&1)
        .expect("the source's page");
    assert_eq!(support::page_label(document.as_lopdf(), page), "restricted");
}

/// The shell prompts for a password on this error, so it must arrive
/// unchanged rather than folded into the refusal.
#[test]
fn a_wrong_password_is_reported_rather_than_refused_as_a_permission() {
    let bytes = copyable_pdf(USER_PASSWORD);

    let error = open_import_source_from_bytes(&bytes, Some("not-the-password"))
        .expect_err("a wrong password must not open the source");

    assert!(matches!(error, ManipError::WrongPassword));
}

#[test]
fn a_missing_password_is_reported_rather_than_refused_as_a_permission() {
    let bytes = copyable_pdf(USER_PASSWORD);

    let error = open_import_source_from_bytes(&bytes, None)
        .expect_err("an encrypted source must not open without its password");

    assert!(matches!(
        error,
        ManipError::WrongPassword | ManipError::PasswordRequired
    ));
}
