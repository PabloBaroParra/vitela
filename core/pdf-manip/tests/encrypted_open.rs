//! Integration tests (T-025/T-026, TDD): decrypt-on-open via SecurityContext
//! against the encrypted-PDF corpus (`tests/fixtures/encrypted/`, Batch 0),
//! plus the wrong-password error path. See spec.md "Open Password-Protected
//! PDF".

use pdf_document::{Credential, EncryptionCredentials, SecurityHandler};
use pdf_manip::{open_document, open_document_with_passwords, ManipError};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("encrypted")
        .join(name)
}

#[test]
fn opens_rc4_fixture_with_correct_user_password() {
    let (doc, security) = open_document(
        &fixture_path("rc4_128_user_and_owner.pdf"),
        Some("user-rc4-pass"),
    )
    .expect("should open with correct user password");

    assert_eq!(doc.as_lopdf().get_pages().len(), 1);
    let security = security.expect("encrypted open must return a SecurityContext");
    assert_eq!(security.handler, SecurityHandler::Rc4_128);
    assert_eq!(security.credential, Credential::User);
}

#[test]
fn opens_rc4_fixture_with_correct_owner_password() {
    let (_doc, security) = open_document(
        &fixture_path("rc4_128_user_and_owner.pdf"),
        Some("owner-rc4-pass"),
    )
    .expect("should open with correct owner password");

    let security = security.expect("encrypted open must return a SecurityContext");
    assert_eq!(security.handler, SecurityHandler::Rc4_128);
    assert_eq!(security.credential, Credential::Owner);
}

#[test]
fn opens_aes_fixture_with_correct_user_password() {
    let (doc, security) = open_document(
        &fixture_path("aes_128_user_and_owner.pdf"),
        Some("user-aes-pass"),
    )
    .expect("should open with correct user password");

    assert_eq!(doc.as_lopdf().get_pages().len(), 1);
    let security = security.expect("encrypted open must return a SecurityContext");
    assert_eq!(security.handler, SecurityHandler::Aes128);
    assert_eq!(security.credential, Credential::User);
}

#[test]
fn opens_aes_fixture_with_correct_owner_password() {
    let (_doc, security) = open_document(
        &fixture_path("aes_128_user_and_owner.pdf"),
        Some("owner-aes-pass"),
    )
    .expect("should open with correct owner password");

    let security = security.expect("encrypted open must return a SecurityContext");
    assert_eq!(security.handler, SecurityHandler::Aes128);
    assert_eq!(security.credential, Credential::Owner);
}

#[test]
fn opening_with_both_passwords_preserves_the_full_encryption_contract() {
    let (_doc, security) = open_document_with_passwords(
        &fixture_path("aes_128_user_and_owner.pdf"),
        "user-aes-pass",
        "owner-aes-pass",
    )
    .expect("both valid passwords should open the document");

    let security = security.expect("encrypted open must return a SecurityContext");
    assert_eq!(security.credential, Credential::Owner);
    assert_eq!(
        security.credentials,
        EncryptionCredentials::both("user-aes-pass", "owner-aes-pass")
    );
}

#[test]
fn wrong_password_is_rejected_cleanly_no_panic() {
    for fixture in ["rc4_128_user_and_owner.pdf", "aes_128_user_and_owner.pdf"] {
        let result = open_document(
            &fixture_path(fixture),
            Some("definitely-the-wrong-password"),
        );
        assert!(
            matches!(result, Err(ManipError::WrongPassword)),
            "{fixture} must reject a wrong password cleanly"
        );
    }
}

#[test]
fn missing_password_on_encrypted_document_is_rejected() {
    let result = open_document(&fixture_path("rc4_128_user_and_owner.pdf"), None);
    assert!(matches!(result, Err(ManipError::PasswordRequired)));
}

#[test]
fn unencrypted_document_opens_with_no_security_context() {
    // Uses gen-fixtures' own base-document builder indirectly by building an
    // unencrypted PDF inline (kept dependency-free); reuses this crate's own
    // `LopdfDocument` round trip via a temp file, since `open_document` reads
    // from a path.
    let dir = std::env::temp_dir().join(format!(
        "pdf-manip-open-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plain.pdf");

    use lopdf::dictionary;

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    doc.objects.insert(
        pages_id,
        lopdf::Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<lopdf::Object>::new(),
            "Count" => 0,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.save(&path).unwrap();

    let (_doc, security) = open_document(&path, None).expect("unencrypted open must succeed");
    assert!(
        security.is_none(),
        "unencrypted open has no SecurityContext"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
