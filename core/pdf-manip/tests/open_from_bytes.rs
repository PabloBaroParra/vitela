//! Integration tests (T-068 DELTA, TDD): bytes-based decrypt-on-open, the
//! canonical cross-platform entrypoint (Android SAF / iOS security-scoped
//! bookmarks have no direct filesystem path — see spec.md delta
//! "Acceso a archivos vía Storage Access Framework" / design.md §3 "Android
//! shell"). Mirrors `tests/encrypted_open.rs` (the path-based equivalent)
//! against the same fixture corpus, proving byte-buffer opens produce an
//! identical `SecurityContext` to path-based opens.

use pdf_document::{Credential, EncryptionCredentials, SecurityHandler};
use pdf_manip::{open_document_from_bytes, open_document_with_passwords_from_bytes, ManipError};

fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("encrypted")
        .join(name);
    std::fs::read(path).expect("fixture must be readable")
}

#[test]
fn opens_rc4_fixture_from_bytes_with_correct_user_password() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let (doc, security) = open_document_from_bytes(&bytes, Some("user-rc4-pass"))
        .expect("should open with correct user password");

    assert_eq!(doc.as_lopdf().get_pages().len(), 1);
    let security = security.expect("encrypted open must return a SecurityContext");
    assert_eq!(security.handler, SecurityHandler::Rc4_128);
    assert_eq!(security.credential, Credential::User);
}

#[test]
fn opens_aes_fixture_from_bytes_with_correct_owner_password() {
    let bytes = fixture_bytes("aes_128_user_and_owner.pdf");
    let (_doc, security) = open_document_from_bytes(&bytes, Some("owner-aes-pass"))
        .expect("should open with correct owner password");

    let security = security.expect("encrypted open must return a SecurityContext");
    assert_eq!(security.handler, SecurityHandler::Aes128);
    assert_eq!(security.credential, Credential::Owner);
}

#[test]
fn opening_from_bytes_with_both_passwords_preserves_the_full_encryption_contract() {
    let bytes = fixture_bytes("aes_128_user_and_owner.pdf");
    let (_doc, security) =
        open_document_with_passwords_from_bytes(&bytes, "user-aes-pass", "owner-aes-pass")
            .expect("both valid passwords should open the document");

    let security = security.expect("encrypted open must return a SecurityContext");
    assert_eq!(security.credential, Credential::Owner);
    assert_eq!(
        security.credentials,
        EncryptionCredentials::both("user-aes-pass", "owner-aes-pass")
    );
}

#[test]
fn wrong_password_from_bytes_is_rejected_cleanly_no_panic() {
    for fixture in ["rc4_128_user_and_owner.pdf", "aes_128_user_and_owner.pdf"] {
        let bytes = fixture_bytes(fixture);
        let result = open_document_from_bytes(&bytes, Some("definitely-the-wrong-password"));
        assert!(
            matches!(result, Err(ManipError::WrongPassword)),
            "{fixture} must reject a wrong password cleanly"
        );
    }
}

#[test]
fn missing_password_on_encrypted_bytes_is_rejected() {
    let bytes = fixture_bytes("rc4_128_user_and_owner.pdf");
    let result = open_document_from_bytes(&bytes, None);
    assert!(matches!(result, Err(ManipError::PasswordRequired)));
}

#[test]
fn unencrypted_bytes_open_with_no_security_context() {
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
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();

    let (_doc, security) =
        open_document_from_bytes(&bytes, None).expect("unencrypted open must succeed");
    assert!(
        security.is_none(),
        "unencrypted open has no SecurityContext"
    );
}
