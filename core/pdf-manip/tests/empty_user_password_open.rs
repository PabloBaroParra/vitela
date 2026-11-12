//! Opening a document whose user password is **empty**: it opens without a
//! prompt, yet its `/P` can still forbid copying, annotating, assembly...
//!
//! lopdf unlocks such a file during the plain unauthenticated load and strips
//! `/Encrypt` from the trailer, so the document looks unencrypted to
//! `is_encrypted()`. Every `open_document*` entry point must still report the
//! real `SecurityContext`, or each permission gate downstream waves the
//! document through as unrestricted.

// Only the restricted-document builder is needed here.
#[allow(dead_code)]
mod support;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter, Rc4CryptFilter};
use lopdf::xref::XrefType;
use lopdf::{EncryptionState, EncryptionVersion, Object, Permissions};
use pdf_document::{Credential, EncryptionCredentials, SecurityHandler};
use pdf_manip::{
    annotation_editing_is_allowed, document_assembly_is_allowed, open_document,
    open_document_from_bytes, open_document_with_credentials_from_bytes,
    open_document_with_passwords, open_document_with_passwords_from_bytes,
    read_security_context_from_bytes, text_extraction_is_allowed, ManipError, OpenCredentials,
};

const OWNER_PASSWORD: &str = "owner-promptless";

/// AES-128, printable only: copying, annotating and assembly all forbidden.
fn promptless_aes() -> Vec<u8> {
    support::restricted_pdf("", OWNER_PASSWORD, Permissions::PRINTABLE)
}

/// The same shape under RC4 (`/V 4`, `/CFM /V2`) — the handler the bug was
/// first reproduced with.
fn promptless_rc4() -> Vec<u8> {
    let mut doc = support::build_pdf_with_pages(&["restricted"]);
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = Object::string_literal("promptless-rc4-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);

    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Rc4CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: OWNER_PASSWORD,
        user_password: "",
        permissions: Permissions::PRINTABLE,
    };
    let state = EncryptionState::try_from(version).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

/// Bumped on every call: parallel test threads can read the same
/// `SystemTime` on Windows, so pid + nanos alone is not unique.
static TEMP_DIR_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A scratch file whose directory removes itself when the test ends.
struct TempPdf(PathBuf);

impl TempPdf {
    fn new(bytes: &[u8]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pdf-manip-promptless-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            TEMP_DIR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("promptless.pdf");
        std::fs::write(&path, bytes).unwrap();
        TempPdf(path)
    }
}

impl Drop for TempPdf {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[test]
fn a_promptless_open_reports_the_restrictive_permissions() {
    for (label, bytes) in [("aes", promptless_aes()), ("rc4", promptless_rc4())] {
        let (doc, security) =
            open_document_from_bytes(&bytes, None).expect("opens without a password");
        let security = security.unwrap_or_else(|| {
            panic!("{label}: an encrypted document must report its SecurityContext")
        });

        assert_eq!(security.credential, Credential::User, "{label}");
        assert_eq!(
            security.credentials,
            EncryptionCredentials::user(""),
            "{label}"
        );
        assert!(!text_extraction_is_allowed(Some(&security)), "{label}");
        assert!(!annotation_editing_is_allowed(Some(&security)), "{label}");
        assert!(!document_assembly_is_allowed(Some(&security)), "{label}");
        assert_eq!(doc.as_lopdf().get_pages().len(), 1, "{label}");
    }
}

#[test]
fn the_open_and_the_probe_agree_on_a_promptless_document() {
    for bytes in [promptless_aes(), promptless_rc4()] {
        let probed = read_security_context_from_bytes(&bytes, None).unwrap();
        let (_, opened) = open_document_from_bytes(&bytes, None).unwrap();
        assert_eq!(opened, probed);
    }
}

#[test]
fn the_handler_is_recovered_from_the_unlocked_state() {
    let (_, aes) = open_document_from_bytes(&promptless_aes(), None).unwrap();
    let (_, rc4) = open_document_from_bytes(&promptless_rc4(), None).unwrap();

    assert_eq!(aes.unwrap().handler, SecurityHandler::Aes128);
    assert_eq!(rc4.unwrap().handler, SecurityHandler::Rc4_128);
}

#[test]
fn the_path_based_open_reports_the_same_context() {
    let file = TempPdf::new(&promptless_aes());

    let (_, security) = open_document(&file.0, None).expect("opens without a password");
    let security = security.expect("an encrypted document must report its SecurityContext");

    assert_eq!(security.credential, Credential::User);
    assert!(!text_extraction_is_allowed(Some(&security)));
}

#[test]
fn the_empty_password_passed_explicitly_opens_as_the_user() {
    let (_, security) = open_document_from_bytes(&promptless_aes(), Some("")).unwrap();
    let security = security.expect("encrypted");

    assert_eq!(security.credential, Credential::User);
    assert!(!text_extraction_is_allowed(Some(&security)));
}

#[test]
fn the_owner_password_lifts_the_restrictions() {
    for bytes in [promptless_aes(), promptless_rc4()] {
        let (doc, security) =
            open_document_from_bytes(&bytes, Some(OWNER_PASSWORD)).expect("owner opens");
        let security = security.expect("encrypted");

        assert_eq!(security.credential, Credential::Owner);
        assert_eq!(
            security.credentials,
            EncryptionCredentials::owner(OWNER_PASSWORD)
        );
        assert!(text_extraction_is_allowed(Some(&security)));
        assert_eq!(doc.as_lopdf().get_pages().len(), 1);
    }
}

#[test]
fn a_wrong_password_is_refused_rather_than_ignored() {
    assert!(matches!(
        open_document_from_bytes(&promptless_aes(), Some("not-the-owner")),
        Err(ManipError::WrongPassword)
    ));
}

#[test]
fn both_passwords_open_as_owner_with_complete_credentials() {
    let (doc, security) =
        open_document_with_passwords_from_bytes(&promptless_aes(), "", OWNER_PASSWORD)
            .expect("both passwords open");
    let security = security.expect("encrypted");

    assert_eq!(security.credential, Credential::Owner);
    assert_eq!(
        security.credentials,
        EncryptionCredentials::both("", OWNER_PASSWORD)
    );
    let (_, as_user) = open_document_from_bytes(&promptless_aes(), None).unwrap();
    assert_eq!(security.permissions, as_user.unwrap().permissions);
    assert_eq!(doc.as_lopdf().get_pages().len(), 1);
}

#[test]
fn the_path_based_dual_open_matches_the_bytes_one() {
    let bytes = promptless_aes();
    let file = TempPdf::new(&bytes);

    let (_, from_path) = open_document_with_passwords(&file.0, "", OWNER_PASSWORD).unwrap();
    let (_, from_bytes) =
        open_document_with_passwords_from_bytes(&bytes, "", OWNER_PASSWORD).unwrap();

    assert!(from_path.is_some());
    assert_eq!(from_path, from_bytes);
}

#[test]
fn the_dual_open_refuses_a_wrong_owner_or_user_password() {
    let bytes = promptless_aes();

    assert!(matches!(
        open_document_with_passwords_from_bytes(&bytes, "", "not-the-owner"),
        Err(ManipError::WrongPassword)
    ));
    assert!(matches!(
        open_document_with_passwords_from_bytes(&bytes, "not-the-user", OWNER_PASSWORD),
        Err(ManipError::WrongPassword)
    ));
}

#[test]
fn every_credential_shape_goes_through_the_same_rule() {
    let bytes = promptless_aes();

    let (_, none) =
        open_document_with_credentials_from_bytes(&bytes, &OpenCredentials::NoPassword).unwrap();
    let (_, owner) = open_document_with_credentials_from_bytes(
        &bytes,
        &OpenCredentials::Password(OWNER_PASSWORD.into()),
    )
    .unwrap();

    assert_eq!(none.unwrap().credential, Credential::User);
    assert_eq!(owner.unwrap().credential, Credential::Owner);
}

#[test]
fn the_probe_resolves_the_owner_password_the_same_way_the_open_does() {
    let bytes = promptless_aes();

    let probed = read_security_context_from_bytes(&bytes, Some(OWNER_PASSWORD)).unwrap();
    let (_, opened) = open_document_from_bytes(&bytes, Some(OWNER_PASSWORD)).unwrap();

    assert_eq!(
        probed.as_ref().map(|s| s.credential),
        Some(Credential::Owner)
    );
    assert_eq!(probed, opened);
    assert!(matches!(
        read_security_context_from_bytes(&bytes, Some("not-the-owner")),
        Err(ManipError::WrongPassword)
    ));
}

/// AES-128 streams with `/StrF /Identity`: strings are stored in the clear,
/// and lopdf's crypt-filter map holds only the `/CF` entries, so the string
/// filter names nothing in it. The handler must still resolve — the same
/// document opened with a real user password reads `/CF/StdCF/CFM` and
/// opens fine, and an empty user password must not make it unopenable.
fn promptless_aes_identity_strings() -> Vec<u8> {
    promptless_aes_with_filters(b"StdCF", b"StdCF", b"Identity")
}

/// A promptless AES-128 document whose single `/CF` entry is `cf_name`, with
/// the given stream and string filters.
fn promptless_aes_with_filters(
    cf_name: &[u8],
    stream_filter: &[u8],
    string_filter: &[u8],
) -> Vec<u8> {
    let mut doc = support::build_pdf_with_pages(&["restricted"]);
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = Object::string_literal("promptless-identity-fixture-id");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);

    let crypt_filter: Arc<dyn CryptFilter> =
        Arc::new(lopdf::encryption::crypt_filters::Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(cf_name.to_vec(), crypt_filter)]),
        stream_filter: stream_filter.to_vec(),
        string_filter: string_filter.to_vec(),
        owner_password: OWNER_PASSWORD,
        user_password: "",
        permissions: Permissions::PRINTABLE,
    };
    let state = EncryptionState::try_from(version).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

#[test]
fn an_identity_string_filter_still_resolves_the_handler() {
    let bytes = promptless_aes_identity_strings();

    let (_, as_user) = open_document_from_bytes(&bytes, None).expect("opens without a prompt");
    let as_user = as_user.expect("encrypted");
    assert_eq!(as_user.handler, SecurityHandler::Aes128);
    assert!(!text_extraction_is_allowed(Some(&as_user)));

    let (_, as_owner) =
        open_document_from_bytes(&bytes, Some(OWNER_PASSWORD)).expect("the owner opens");
    assert_eq!(as_owner.unwrap().credential, Credential::Owner);
}

/// A `/CF` entry that is not called `StdCF` is legal, and the no-password
/// open resolves it. The owner password must lift the restrictions on the
/// same file instead of failing on a handler lookup that only reads `StdCF`.
#[test]
fn the_owner_password_opens_a_crypt_filter_not_named_stdcf() {
    let bytes = promptless_aes_with_filters(b"MyCF", b"MyCF", b"MyCF");

    let (_, as_user) = open_document_from_bytes(&bytes, None).expect("opens without a prompt");
    assert_eq!(as_user.expect("encrypted").handler, SecurityHandler::Aes128);

    let (_, as_owner) =
        open_document_from_bytes(&bytes, Some(OWNER_PASSWORD)).expect("the owner opens");
    let as_owner = as_owner.expect("encrypted");
    assert_eq!(as_owner.credential, Credential::Owner);
    assert_eq!(as_owner.handler, SecurityHandler::Aes128);
    assert!(text_extraction_is_allowed(Some(&as_owner)));

    let probed = read_security_context_from_bytes(&bytes, Some(OWNER_PASSWORD)).unwrap();
    assert_eq!(probed, Some(as_owner));

    assert!(matches!(
        open_document_from_bytes(&bytes, Some("not-the-owner")),
        Err(ManipError::WrongPassword)
    ));
}

/// Encrypts a one-page fixture with `version_for` and saves it, keeping the
/// classic xref table and trailer `/ID` every other builder here sets up.
fn promptless_with(
    file_id: &str,
    version_for: impl FnOnce(&lopdf::Document) -> EncryptionVersion<'_>,
) -> Vec<u8> {
    let mut doc = support::build_pdf_with_pages(&["restricted"]);
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let file_id = Object::string_literal(file_id);
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);

    let state = EncryptionState::try_from(version_for(&doc)).expect("build encryption state");
    doc.encrypt(&state).expect("encrypt fixture");

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    bytes
}

/// RC4 with a 40-bit key (`/V 1`, `/R 2`): the oldest standard handler, and
/// still what many "no copy" PDFs from old producers carry.
fn promptless_rc4_40() -> Vec<u8> {
    promptless_with("promptless-rc4-40-fixture-id", |doc| {
        EncryptionVersion::V1 {
            document: doc,
            owner_password: OWNER_PASSWORD,
            user_password: "",
            permissions: Permissions::PRINTABLE,
        }
    })
}

/// AES-256 (`/V 5`, `/R 6`, `/CFM /AESV3`) — the PDF 2.0 handler.
fn promptless_aes_256() -> Vec<u8> {
    // Fixed key: the fixture is deterministic, and the per-object IV keeps
    // the ciphertexts distinct anyway. Static so it outlives the borrow the
    // version takes alongside the document's.
    static FILE_ENCRYPTION_KEY: [u8; 32] = [7; 32];
    promptless_with("promptless-aes-256-fixture-id", |_| {
        let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
        EncryptionVersion::V5 {
            encrypt_metadata: true,
            crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
            file_encryption_key: &FILE_ENCRYPTION_KEY,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: OWNER_PASSWORD,
            user_password: "",
            permissions: Permissions::PRINTABLE,
        }
    })
}

/// Every role an open can resolve, for a promptless fixture of one handler:
/// no password is the restricted user, the owner password lifts the
/// restrictions, a wrong one is refused, and the dual open carries both.
fn assert_promptless_roles(label: &str, bytes: &[u8], handler: SecurityHandler) {
    let (doc, as_user) =
        open_document_from_bytes(bytes, None).unwrap_or_else(|e| panic!("{label}: {e}"));
    let as_user = as_user.unwrap_or_else(|| panic!("{label}: must report its context"));
    assert_eq!(as_user.credential, Credential::User, "{label}");
    assert_eq!(
        as_user.credentials,
        EncryptionCredentials::user(""),
        "{label}"
    );
    assert_eq!(as_user.handler, handler, "{label}");
    assert!(!text_extraction_is_allowed(Some(&as_user)), "{label}");
    assert!(!annotation_editing_is_allowed(Some(&as_user)), "{label}");
    assert!(!document_assembly_is_allowed(Some(&as_user)), "{label}");
    assert_eq!(doc.as_lopdf().get_pages().len(), 1, "{label}");

    let (_, as_owner) = open_document_from_bytes(bytes, Some(OWNER_PASSWORD))
        .unwrap_or_else(|e| panic!("{label}: the owner opens: {e}"));
    let as_owner = as_owner.unwrap_or_else(|| panic!("{label}: encrypted"));
    assert_eq!(as_owner.credential, Credential::Owner, "{label}");
    assert_eq!(
        as_owner.credentials,
        EncryptionCredentials::owner(OWNER_PASSWORD),
        "{label}"
    );
    assert_eq!(as_owner.handler, handler, "{label}");
    assert!(text_extraction_is_allowed(Some(&as_owner)), "{label}");

    assert!(
        matches!(
            open_document_from_bytes(bytes, Some("not-the-owner")),
            Err(ManipError::WrongPassword)
        ),
        "{label}"
    );

    let (_, both) = open_document_with_passwords_from_bytes(bytes, "", OWNER_PASSWORD)
        .unwrap_or_else(|e| panic!("{label}: both passwords open: {e}"));
    let both = both.unwrap_or_else(|| panic!("{label}: encrypted"));
    assert_eq!(both.credential, Credential::Owner, "{label}");
    assert_eq!(
        both.credentials,
        EncryptionCredentials::both("", OWNER_PASSWORD),
        "{label}"
    );
    assert_eq!(both.handler, handler, "{label}");
    assert_eq!(both.permissions, as_user.permissions, "{label}");
}

/// `/V 1` maps to the RC4 handler, like `/V 2`.
#[test]
fn a_promptless_rc4_40_document_resolves_every_role() {
    assert_promptless_roles("rc4-40", &promptless_rc4_40(), SecurityHandler::Rc4_128);
}

#[test]
fn a_promptless_aes_256_document_resolves_every_role() {
    assert_promptless_roles("aes-256", &promptless_aes_256(), SecurityHandler::Aes256);
}
