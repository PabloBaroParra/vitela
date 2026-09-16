//! Batch 24, T-196: compression seen from a shell.
//!
//! `pdf-compress` proves the squeezing and `pdf-save` proves the gates; what
//! is tested here is the only thing that exists once they reach the boundary
//! — that a shell can pick a preset, run the save, and read what happened
//! without linking either crate. Every assertion is made through `pdf-ffi`'s
//! own exported functions, the same way a generated Swift/C#/Kotlin binding
//! would reach them.

use pdf_ffi::{
    compress_presets, compressed_save_will_invalidate_signatures, compression_refusal,
    open_from_bytes, open_with_passwords_from_bytes, save_compressed_to_bytes,
    save_compressed_to_path, save_to_bytes, will_invalidate_signatures, FfiCompressOutcome,
    FfiCompressPreset, FfiCompressRefusal, FfiCompressWork, FfiError, FfiSaveIntent,
    FfiSignatureAcknowledgement,
};

fn fixture_bytes(kind: &str, name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(kind)
        .join(name);
    std::fs::read(path).expect("fixture must be readable")
}

/// Enough repeated text on enough pages that a repack has something to win —
/// the same generator `pdf-save`'s own compressed-save suite uses.
fn compressible_bytes(pages: u32) -> Vec<u8> {
    let mut document = gen_fixtures::build_multi_page_document(pages, "compressible");
    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .expect("fixture must serialise");
    bytes
}

fn temp_path(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pdf-ffi-compress-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("doc.pdf")
}

// ---------------------------------------------------------------------
// Preset: what a compress dialog is allowed to offer
// ---------------------------------------------------------------------

/// The list a preset dialog populates from, in the core's own order —
/// strongest-preserving first. A shell that hard-codes three arms of its own
/// is exactly what `pdf_compress::CompressPreset::all` exists to prevent, and
/// it can only prevent it if the list crosses the boundary.
#[test]
fn every_preset_crosses_the_boundary_in_the_order_the_core_lists_them() {
    assert_eq!(
        compress_presets(),
        vec![
            FfiCompressPreset::Lossless,
            FfiCompressPreset::Balanced,
            FfiCompressPreset::Small,
        ]
    );
}

// ---------------------------------------------------------------------
// Execution, and the report that comes back with the bytes
// ---------------------------------------------------------------------

/// The headline, from a shell's side: ask for a smaller file, get one, and
/// get told what it cost. The report has to describe the bytes actually
/// handed over — a shell shows those numbers to a user.
#[test]
fn a_compressed_save_returns_smaller_bytes_and_a_report_describing_them() {
    let original = compressible_bytes(8);
    let handle = open_from_bytes(original.clone(), None).expect("fixture opens");

    let plain = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("an ordinary save succeeds");

    let compressed = save_compressed_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
        FfiCompressPreset::Lossless,
    )
    .expect("and so does a compressed one");

    assert_eq!(compressed.report.outcome, FfiCompressOutcome::Reduced);
    assert!(
        compressed.bytes.len() < plain.len(),
        "compressed save produced {} bytes where the ordinary one produced {}",
        compressed.bytes.len(),
        plain.len()
    );
    assert_eq!(
        compressed.report.after_bytes,
        compressed.bytes.len() as u64,
        "the report must describe the bytes the caller got"
    );
    assert_eq!(
        compressed.report.saved_bytes,
        compressed.report.before_bytes - compressed.report.after_bytes
    );
    assert!(compressed.report.refusals.is_empty());
    assert!(
        compressed.bytes.windows(6).any(|w| w == b"ObjStm"),
        "the saving has to come from a real repack, not from a shorter re-serialisation"
    );

    assert_eq!(
        lopdf::Document::load_mem(&compressed.bytes)
            .expect("the result must reload")
            .get_pages()
            .len(),
        8,
        "a smaller file that lost a page is not a compression"
    );
}

/// A real saving with an empty tally, which is not a contradiction and is a
/// trap worth freezing before a dialog is built on it.
///
/// The two wins that shrank this document — object streams and a
/// cross-reference stream — are the *write format*, and `Work` counts stages,
/// not formats. Its eight content streams are 51 bytes each, so flate cannot
/// beat them and `pdf-compress` rightly declines to claim it did. A shell
/// that renders "nothing was done" from an all-zero tally would be lying
/// about a file that just lost a third of its size: what it reads is the
/// outcome and the byte counts.
#[test]
fn a_real_saving_can_arrive_with_nothing_in_the_tally() {
    let handle = open_from_bytes(compressible_bytes(8), None).expect("fixture opens");

    let compressed = save_compressed_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
        FfiCompressPreset::Lossless,
    )
    .expect("compressed save");

    assert_eq!(compressed.report.outcome, FfiCompressOutcome::Reduced);
    assert!(compressed.report.saved_bytes > 0);
    assert_eq!(
        compressed.report.work,
        FfiCompressWork::default(),
        "no stage beat what it was given here, and the tally must say so"
    );
}

/// The never-grow guarantee, asked from the far side of the boundary on
/// every preset a shell can offer. `pdf-compress` enforces it structurally;
/// this checks that nothing on the way out here found a way around it.
#[test]
fn no_preset_a_shell_can_offer_ever_grows_the_file() {
    let handle = open_from_bytes(compressible_bytes(4), None).expect("fixture opens");

    let plain = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("an ordinary save succeeds");

    for preset in compress_presets() {
        let compressed = save_compressed_to_bytes(
            &handle,
            FfiSaveIntent::Default,
            FfiSignatureAcknowledgement::Unacknowledged,
            preset,
        )
        .expect("compressed save");

        assert!(
            compressed.bytes.len() <= plain.len(),
            "{preset:?} produced {} bytes where the ordinary save produced {}",
            compressed.bytes.len(),
            plain.len()
        );
    }
}

/// The path convenience, and the one thing it must not lose on the way: the
/// report. `save_to_path` can return nothing because there is nothing to
/// know; a compression that a user asked for has a result they are owed.
#[test]
fn the_path_convenience_writes_the_same_bytes_and_still_returns_the_report() {
    let handle = open_from_bytes(compressible_bytes(8), None).expect("fixture opens");
    let path = temp_path("to-path");

    let report = save_compressed_to_path(
        &handle,
        path.to_string_lossy().into_owned(),
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
        FfiCompressPreset::Lossless,
    )
    .expect("compressed save to path");

    let written = std::fs::read(&path).expect("the file must exist");
    assert_eq!(report.after_bytes, written.len() as u64);
    assert_eq!(report.outcome, FfiCompressOutcome::Reduced);
    assert_eq!(
        lopdf::Document::load_mem(&written)
            .expect("the written file must reload")
            .get_pages()
            .len(),
        8
    );
}

// ---------------------------------------------------------------------
// The two gates, asked and answered through the boundary
// ---------------------------------------------------------------------

/// The question a shell asks before it even offers the button: this document
/// cannot be compressed, and here is the sentence to show. Asked before the
/// save, because after it there is nothing left to decide.
#[test]
fn a_protected_document_says_up_front_that_it_cannot_be_compressed() {
    let handle = open_with_passwords_from_bytes(
        fixture_bytes("encrypted", "rc4_128_user_and_owner.pdf"),
        "user-rc4-pass".to_string(),
        "owner-rc4-pass".to_string(),
    )
    .expect("fixture opens with both passwords");

    assert_eq!(
        compression_refusal(&handle, FfiSaveIntent::Default),
        Some(FfiCompressRefusal::EncryptedDocumentNotRewritable)
    );
}

/// And what running it anyway does: not an error — the ordinary save's own
/// bytes, plus the reason nothing was squeezed. A shell that shows the report
/// tells the user why their file is the same size.
#[test]
fn compressing_a_protected_document_writes_the_ordinary_bytes_and_says_why() {
    let handle = open_with_passwords_from_bytes(
        fixture_bytes("encrypted", "rc4_128_user_and_owner.pdf"),
        "user-rc4-pass".to_string(),
        "owner-rc4-pass".to_string(),
    )
    .expect("fixture opens with both passwords");

    let plain = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("the protected save itself still works");

    let compressed = save_compressed_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
        FfiCompressPreset::Small,
    )
    .expect("a refused compression is not an error");

    assert_eq!(compressed.report.outcome, FfiCompressOutcome::NoGain);
    assert_eq!(
        compressed.report.refusals,
        vec![FfiCompressRefusal::EncryptedDocumentNotRewritable]
    );
    assert_eq!(compressed.report.work, FfiCompressWork::default());
    assert_eq!(
        compressed.bytes, plain,
        "a refused compression hands back the save's own bytes, byte for byte"
    );
}

/// The yes-path for a protected document, which is not "compress anyway" but
/// a separately consented strip — and the boundary is where that consent
/// becomes a durable audit entry, exactly as it is for an ordinary save.
#[test]
fn stripping_protection_clears_the_way_and_is_recorded_as_consent() {
    let handle = open_with_passwords_from_bytes(
        fixture_bytes("encrypted", "rc4_128_user_and_owner.pdf"),
        "user-rc4-pass".to_string(),
        "owner-rc4-pass".to_string(),
    )
    .expect("fixture opens with both passwords");

    assert_eq!(
        compression_refusal(&handle, FfiSaveIntent::StripProtection),
        None
    );

    let compressed = save_compressed_to_bytes(
        &handle,
        FfiSaveIntent::StripProtection,
        FfiSignatureAcknowledgement::Unacknowledged,
        FfiCompressPreset::Lossless,
    )
    .expect("a consented strip compresses like anything else");

    assert!(compressed.report.refusals.is_empty());
    assert!(
        !lopdf::Document::load_mem(&compressed.bytes)
            .expect("must reload")
            .is_encrypted(),
        "a strip writes plaintext, which is the whole reason it can be compressed"
    );
}

/// The signature gate, and the query that makes it answerable in time. With
/// nothing edited, an ordinary save of this file appends and breaks nothing
/// — compressing it rewrites every offset the `/ByteRange` covers. The two
/// questions have different answers, so a shell needs both.
#[test]
fn a_signed_file_answers_the_compressed_question_differently_from_the_ordinary_one() {
    let handle = open_from_bytes(fixture_bytes("signed", "rsa2048_sha256.pdf"), None)
        .expect("fixture opens");

    assert!(
        !will_invalidate_signatures(&handle, FfiSaveIntent::Default).expect("the ordinary query"),
        "nothing was edited, so an ordinary save of this file appends"
    );
    assert!(
        compressed_save_will_invalidate_signatures(&handle, FfiSaveIntent::Default)
            .expect("the compressed query"),
        "compressing the same file rewrites it, so the shell must be able to warn first"
    );
}

/// And the refusal that backs the warning up: a shell that never asked does
/// not get to break a signature by calling the other function.
#[test]
fn compressing_a_signed_file_is_refused_until_the_shell_says_the_user_was_asked() {
    let handle = open_from_bytes(fixture_bytes("signed", "rsa2048_sha256.pdf"), None)
        .expect("fixture opens");

    assert!(matches!(
        save_compressed_to_bytes(
            &handle,
            FfiSaveIntent::Default,
            FfiSignatureAcknowledgement::Unacknowledged,
            FfiCompressPreset::Lossless,
        ),
        Err(FfiError::SignaturesWouldBeInvalidated)
    ));

    let acknowledged = save_compressed_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::ProceedAndInvalidate,
        FfiCompressPreset::Lossless,
    )
    .expect("once the user has said yes, the compression runs");

    assert!(
        acknowledged.report.refusals.is_empty(),
        "the caller asked for exactly this, so nothing was refused"
    );
}
