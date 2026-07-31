//! Integration tests for the annotation-editing gate: resolving a document's
//! `SecurityContext` and applying `annotation_editing_is_allowed` to it.
//!
//! The unit tests in `src/security.rs` cover the bit arithmetic. What needs a
//! real document is *where the context comes from*: a shell that asks the
//! wrong loader gets `None` for a document that carries real restrictions,
//! and `None` means "unencrypted, everything permitted". These tests pin the
//! loader contract so that mistake cannot be made silently.

// Only the builders are needed here; `page_label` belongs to the manipulation
// tests that share this helper module.
#[allow(dead_code)]
mod support;

use lopdf::Permissions;
use pdf_document::Credential;
use pdf_manip::{annotation_editing_is_allowed, read_security_context_from_bytes};

const USER_PASSWORD: &str = "user-no-annots";
const OWNER_PASSWORD: &str = "owner-no-annots";

/// `/P` granting printing but neither copying nor annotating.
fn no_annotate_pdf(user_password: &str) -> Vec<u8> {
    support::restricted_pdf(user_password, OWNER_PASSWORD, Permissions::PRINTABLE)
}

#[test]
fn a_document_that_forbids_annotating_denies_editing_for_the_user() {
    let bytes = no_annotate_pdf(USER_PASSWORD);

    let security = read_security_context_from_bytes(&bytes, Some(USER_PASSWORD))
        .expect("probe should succeed with the user password")
        .expect("an encrypted document must report a SecurityContext");

    assert_eq!(security.credential, Credential::User);
    assert!(
        !annotation_editing_is_allowed(Some(&security)),
        "a /P without the annotate bit must deny annotation editing"
    );
}

#[test]
fn the_owner_of_a_no_annotate_document_may_still_edit() {
    let bytes = no_annotate_pdf(USER_PASSWORD);

    let security = read_security_context_from_bytes(&bytes, Some(OWNER_PASSWORD))
        .expect("probe should succeed with the owner password")
        .expect("an encrypted document must report a SecurityContext");

    assert_eq!(security.credential, Credential::Owner);
    assert!(annotation_editing_is_allowed(Some(&security)));
}

/// The trap this whole test file exists for.
///
/// A document whose **user password is empty** opens with no prompt, so
/// lopdf's unauthenticated load already decrypts it in place and drops
/// `/Encrypt` from the trailer. `open_document` checks `is_encrypted()` and
/// therefore reports **no** `SecurityContext` at all for it — while the
/// document still forbids annotating. A gate fed from the open path would read
/// `None`, conclude "unencrypted, everything permitted", and hand the user a
/// full annotation toolbar on a document that denies it.
///
/// `read_security_context` recovers the permissions from lopdf's decoded
/// `EncryptionState` instead, which is why every permission gate has to source
/// its context from the probe and never from the open.
#[test]
fn an_empty_user_password_document_keeps_its_restrictions_through_the_probe() {
    let bytes = no_annotate_pdf("");

    let (_doc, opened) =
        pdf_manip::open_document_from_bytes(&bytes, None).expect("opens with no password");
    assert!(
        opened.is_none(),
        "the open path drops the security context for this shape — if this ever \
         starts reporting one, the probe-only rule below can be revisited"
    );
    assert!(
        annotation_editing_is_allowed(opened.as_ref()),
        "which is exactly why gating on the open path's context is unsafe"
    );

    let probed = read_security_context_from_bytes(&bytes, None)
        .expect("an empty user password must probe without a credential")
        .expect("the restrictions survive in the decoded encryption state");

    assert_eq!(probed.credential, Credential::User);
    assert!(
        !annotation_editing_is_allowed(Some(&probed)),
        "the probe must refuse what the open path waved through"
    );
}

#[test]
fn an_unencrypted_document_permits_annotation_editing() {
    let bytes = {
        let mut doc = support::build_pdf_with_pages(&["plain"]);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save fixture");
        bytes
    };

    let security =
        read_security_context_from_bytes(&bytes, None).expect("unencrypted probe must succeed");

    assert!(security.is_none());
    assert!(annotation_editing_is_allowed(security.as_ref()));
}
