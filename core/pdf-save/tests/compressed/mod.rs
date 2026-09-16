//! Fixtures the two compressed-save suites share (Batch 24, T-195).
//!
//! Shared rather than copied because the split between them is by *what is
//! being asked* — what a compressed save produces, and which documents it
//! refuses — and both questions need the same three files to ask it of. The
//! same arrangement `pdf-compress`'s own `tests/common` uses.

// Each test binary compiles this module whole and uses the part it needs;
// the unused half is not dead code, it is the other suite's.
#![allow(dead_code)]

use pdf_save::{SaveInput, SaveIntent, SignatureAcknowledgement};

pub fn fixture_path(kind: &str, name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(kind)
        .join(name)
}

fn temp_pdf_path(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pdf-save-compressed-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("doc.pdf")
}

/// A real on-disk file with enough repeated text on enough pages that a
/// repack has something to win — built by the shared generator, like every
/// other fixture in this suite.
pub fn text_pdf(label: &str) -> std::path::PathBuf {
    let mut doc = gen_fixtures::build_multi_page_document(8, "compressible");
    let path = temp_pdf_path(label);
    doc.save(&path).unwrap();
    path
}

/// The same thing carrying a signature dictionary — the shape
/// `pdf_manip::document_has_signatures` recognises.
pub fn signed_pdf(label: &str) -> std::path::PathBuf {
    use lopdf::dictionary;

    let mut doc = gen_fixtures::build_multi_page_document(4, "signed");
    doc.add_object(dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "adbe.pkcs7.detached",
    });
    let path = temp_pdf_path(label);
    doc.save(&path).unwrap();
    path
}

/// A save input over a document nobody has told anything about signatures —
/// the default a shell reaches by not thinking, and the one the gates are
/// written against.
pub fn unacknowledged<'a>(
    document: &'a pdf_document::Document,
    base: &'a pdf_manip::LopdfDocument,
    original_bytes: &'a [u8],
) -> SaveInput<'a> {
    SaveInput {
        document,
        base,
        original_bytes: Some(original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::none(),
    }
}

pub fn apply_command(document: &mut pdf_document::Document, command: pdf_document::Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}
