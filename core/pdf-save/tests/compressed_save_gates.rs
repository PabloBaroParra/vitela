//! Batch 24, T-195: which documents a compressed save refuses, and how a
//! caller says yes to the one refusal that has a yes.
//!
//! The two gates are not symmetrical and the tests are grouped that way. A
//! signature is the user's to spend, so the refusal is a question and
//! [`SignatureAcknowledgement`] is the answer. Protection is not: bytes that
//! come out encrypted cannot be repacked by anyone, so the refusal is a fact
//! and the only way past it is to write plaintext instead.

mod compressed;

use compressed::{fixture_path, signed_pdf, unacknowledged};
use pdf_compress::{CompressPreset, Outcome, Refusal};
use pdf_document::{AuditActor, AuditEvent};
use pdf_save::{
    compressed_save_will_invalidate_signatures, compression_blocker, save_document,
    save_document_compressed, will_invalidate_signatures, SaveInput, SaveIntent,
    SignatureAcknowledgement,
};

/// The signature gate, and the thing that makes it worth its own query: with
/// nothing edited at all, an ordinary save of this signed file is an append
/// and breaks nothing — while compressing it rewrites every byte offset the
/// `/ByteRange` covers. A "just make this smaller" button is exactly that
/// case, which is why the existing query would have answered the wrong
/// question here.
#[test]
fn compressing_a_signed_file_is_refused_until_the_user_has_been_asked() {
    let path = signed_pdf("signed-refused");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let input = unacknowledged(&document, &base, &original_bytes);

    assert!(
        !will_invalidate_signatures(input).expect("the ordinary query answers"),
        "nothing was edited, so an ordinary save of this file appends and breaks nothing"
    );
    assert!(
        compressed_save_will_invalidate_signatures(input).expect("the compressed query answers"),
        "compressing the same file rewrites it, so the shell must be able to warn first"
    );
    assert!(matches!(
        save_document_compressed(input, CompressPreset::Lossless),
        Err(pdf_save::SaveError::SignaturesWouldBeInvalidated)
    ));
}

/// The path of yes. Once the user has been told, the compression runs — and
/// `pdf-compress` must not refuse a second time on their behalf, which is
/// what the acknowledgement crossing the crate boundary buys.
#[test]
fn compressing_a_signed_file_proceeds_once_the_user_has_said_yes() {
    let path = signed_pdf("signed-consented");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let input = SaveInput {
        signatures: SignatureAcknowledgement::ProceedAndInvalidate,
        ..unacknowledged(&document, &base, &original_bytes)
    };

    let compressed = save_document_compressed(input, CompressPreset::Lossless)
        .expect("an acknowledged save is not refused");

    assert_eq!(compressed.report.outcome(), Outcome::Reduced);
    assert_eq!(
        compressed.report.refusals(),
        &[],
        "the caller asked for exactly this, so nothing was refused"
    );
    assert_eq!(
        lopdf::Document::load_mem(&compressed.bytes)
            .expect("must reload")
            .get_pages()
            .len(),
        4
    );
}

/// The encryption gate. A save that re-applies protection writes ciphertext,
/// and there is no version of the repack that helps it — so the compression
/// is refused, the reason travels in the report, and the caller gets exactly
/// the bytes the ordinary save would have produced.
#[test]
fn a_save_that_stays_protected_reports_the_refusal_and_writes_the_ordinary_bytes() {
    let path = fixture_path("encrypted", "rc4_128_user_and_owner.pdf");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, Some("owner-rc4-pass")).unwrap();
    let security = security.expect("fixture is encrypted");
    let document = pdf_save::document_from_lopdf(&base, Some(security)).unwrap();
    let input = unacknowledged(&document, &base, &original_bytes);

    assert_eq!(
        compression_blocker(input),
        Some(Refusal::EncryptedDocumentNotRewritable)
    );

    let plain = save_document(input).expect("the protected save itself still works");
    let compressed =
        save_document_compressed(input, CompressPreset::Small).expect("and is not an error");

    assert_eq!(compressed.report.outcome(), Outcome::NoGain);
    assert_eq!(
        compressed.report.refusals(),
        &[Refusal::EncryptedDocumentNotRewritable]
    );
    assert_eq!(
        compressed.bytes, plain,
        "a refused compression must hand back the save's own bytes, byte for byte"
    );
    assert!(
        lopdf::Document::load_mem(&compressed.bytes)
            .expect("must reload")
            .is_encrypted(),
        "the refusal must not have cost the document its protection"
    );
}

/// And the yes-path for a protected document, which is not "compress anyway"
/// but a separately consented strip: the output is plaintext, so it
/// compresses like anything else.
#[test]
fn stripping_protection_lets_the_same_document_compress() {
    let path = fixture_path("encrypted", "rc4_128_user_and_owner.pdf");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, Some("owner-rc4-pass")).unwrap();
    let security = security.expect("fixture is encrypted");
    let mut document = pdf_save::document_from_lopdf(&base, Some(security)).unwrap();
    document
        .audit_log
        .record(AuditEvent::StripProtectionConsent, AuditActor::User);

    let input = SaveInput {
        intent: SaveIntent::StripProtection,
        ..unacknowledged(&document, &base, &original_bytes)
    };

    assert_eq!(compression_blocker(input), None);

    let compressed =
        save_document_compressed(input, CompressPreset::Lossless).expect("compressed save");

    let reloaded = lopdf::Document::load_mem(&compressed.bytes).expect("must reload");
    assert!(!reloaded.is_encrypted());
    assert!(compressed.report.refusals().is_empty());
}
