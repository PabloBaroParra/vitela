//! Integration tests for the text-extraction gate: reading a document's
//! `SecurityContext` without decrypting it (`read_security_context`) and
//! applying the permission rule (`text_extraction_is_allowed`) to it.
//!
//! The committed encrypted corpus (`tests/fixtures/encrypted/`) is generated
//! with `PRINTABLE | COPYABLE`, so it can only cover the permitted case. The
//! *denied* case — the one the gate exists for — needs a document whose
//! `/P` clears the copy bit, built here rather than added to the shared
//! corpus.

// Only the page builder is needed here; `page_label` belongs to the
// manipulation tests that share this helper module.
#[allow(dead_code)]
mod support;

use std::path::{Path, PathBuf};

use lopdf::Permissions;
use pdf_document::Credential;
use pdf_manip::{read_security_context, read_security_context_from_bytes, ManipError};

const USER_PASSWORD: &str = "user-no-copy";
const OWNER_PASSWORD: &str = "owner-no-copy";

fn corpus_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("encrypted")
        .join(name)
}

/// A scratch directory that removes itself when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pdf-manip-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Builds an AES-128 encrypted single-page PDF whose `/P` grants printing but
/// **not** copying — the shape of document the extraction gate must refuse.
///
/// `user_password` is what the reader has to supply to open it; passing `""`
/// produces the very common "opens with no prompt at all, yet forbids
/// copying" document.
fn no_copy_pdf(user_password: &str) -> Vec<u8> {
    support::restricted_pdf(user_password, OWNER_PASSWORD, Permissions::PRINTABLE)
}

#[test]
fn a_document_that_forbids_copying_denies_extraction_for_the_user() {
    let bytes = no_copy_pdf(USER_PASSWORD);

    let security = read_security_context_from_bytes(&bytes, Some(USER_PASSWORD))
        .expect("probe should succeed with the user password")
        .expect("an encrypted document must report a SecurityContext");

    assert_eq!(security.credential, Credential::User);
    assert!(
        !pdf_manip::text_extraction_is_allowed(Some(&security)),
        "a /P without the copy bit must deny text extraction"
    );
}

#[test]
fn the_owner_of_a_no_copy_document_may_still_extract() {
    let bytes = no_copy_pdf(USER_PASSWORD);

    let security = read_security_context_from_bytes(&bytes, Some(OWNER_PASSWORD))
        .expect("probe should succeed with the owner password")
        .expect("an encrypted document must report a SecurityContext");

    assert_eq!(security.credential, Credential::Owner);
    assert!(
        pdf_manip::text_extraction_is_allowed(Some(&security)),
        "the owner credential bypasses the permission bitmask"
    );
}

#[test]
fn a_document_with_an_empty_user_password_is_probed_without_one() {
    // Opens with no prompt — pdfium (and every other reader) lets the user
    // straight in — but still forbids copying. Probing it must not report
    // "password required"; it must report the real, restrictive permissions.
    let bytes = no_copy_pdf("");

    let security = read_security_context_from_bytes(&bytes, None)
        .expect("an empty user password must probe without a credential")
        .expect("an encrypted document must report a SecurityContext");

    assert_eq!(security.credential, Credential::User);
    assert!(!pdf_manip::text_extraction_is_allowed(Some(&security)));
}

#[test]
fn a_wrong_password_is_reported_rather_than_silently_permitted() {
    let bytes = no_copy_pdf(USER_PASSWORD);

    assert!(matches!(
        read_security_context_from_bytes(&bytes, Some("not-the-password")),
        Err(ManipError::WrongPassword)
    ));
}

#[test]
fn an_unencrypted_document_reports_no_security_context() {
    let dir = TempDir::new("plain-probe");
    let path = dir.0.join("plain.pdf");
    let mut doc = support::build_pdf_with_pages(&["plain"]);
    doc.save(&path).unwrap();

    let security = read_security_context(&path, None).expect("unencrypted probe must succeed");
    assert!(security.is_none());
    assert!(pdf_manip::text_extraction_is_allowed(security.as_ref()));
}

#[test]
fn the_probe_agrees_with_a_full_open_on_the_committed_corpus() {
    // The probe skips the decrypting load; it must still resolve the same
    // handler, credential role and permissions the real open does, or the
    // GTK4 shell and the FFI boundary would be gating on different facts.
    for (fixture, password) in [
        ("rc4_128_user_and_owner.pdf", "user-rc4-pass"),
        ("rc4_128_user_and_owner.pdf", "owner-rc4-pass"),
        ("aes_128_user_and_owner.pdf", "user-aes-pass"),
        ("aes_128_user_and_owner.pdf", "owner-aes-pass"),
    ] {
        let path = corpus_path(fixture);
        let probed = read_security_context(&path, Some(password))
            .expect("probe should succeed")
            .expect("encrypted fixture must report a SecurityContext");
        let (_doc, opened) =
            pdf_manip::open_document(&path, Some(password)).expect("open should succeed");
        let opened = opened.expect("encrypted fixture must report a SecurityContext");

        assert_eq!(probed, opened, "{fixture} with {password}");
    }
}
