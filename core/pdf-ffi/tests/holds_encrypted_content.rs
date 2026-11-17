//! `DocumentHandle::holds_encrypted_content` — the question a shell asks
//! before writing a session's bytes somewhere the user never chose (crash
//! recovery). Any trace of encryption answers yes: the shell cannot
//! re-encrypt what it writes, so a protected document is never autosaved.

use pdf_ffi::{import_prepared, open_from_bytes, prepare_import};

const AES_FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/encrypted/aes_128_user_and_owner.pdf"
));

fn plain_pdf() -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

#[test]
fn a_plain_document_holds_no_encrypted_content() {
    let handle = open_from_bytes(plain_pdf(), None).expect("opens");

    assert!(!handle.holds_encrypted_content());
}

#[test]
fn a_document_opened_with_a_password_holds_encrypted_content() {
    let handle =
        open_from_bytes(AES_FIXTURE.to_vec(), Some("user-aes-pass".into())).expect("unlocks");

    assert!(handle.holds_encrypted_content());
}

#[test]
fn the_owner_password_still_counts_as_encrypted() {
    let handle =
        open_from_bytes(AES_FIXTURE.to_vec(), Some("owner-aes-pass".into())).expect("unlocks");

    assert!(
        handle.holds_encrypted_content(),
        "full permissions do not make the bytes any less protected"
    );
}

/// A page grafted from a protected PDF carries that PDF's content into
/// every save of the session, decrypted.
#[test]
fn importing_from_an_encrypted_pdf_makes_the_session_hold_encrypted_content() {
    let handle = open_from_bytes(plain_pdf(), None).expect("opens");
    let source =
        prepare_import(AES_FIXTURE.to_vec(), Some("owner-aes-pass".into())).expect("prepares");

    import_prepared(&handle, vec![source], 1).expect("imports");

    assert!(handle.holds_encrypted_content());
}
