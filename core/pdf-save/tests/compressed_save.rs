//! Batch 24, T-195: what a compressed save produces.
//!
//! `pdf-compress` has its own corpus and proves the squeezing; what is tested
//! here is the part that only exists once the two crates meet — which writer a
//! compressed save picks, what it hands back, and what it leaves alone. Which
//! documents it refuses is `compressed_save_gates.rs`.

mod compressed;

use compressed::{apply_command, fixture_path, text_pdf, unacknowledged};
use pdf_compress::{CompressPreset, Outcome};
use pdf_document::{Command, PageId};
use pdf_save::{save_document, save_document_compressed};

/// The headline: the same save, run twice, and the compressed one is
/// smaller — while still being the same document.
///
/// Both halves matter. A smaller file that lost a page would pass the first
/// assertion on its own, which is the exact failure `pdf-compress`'s session
/// re-read exists to catch; asserting the page count here checks that the
/// guard is still wired up from this side of the boundary.
#[test]
fn a_compressed_save_is_smaller_than_the_same_save_uncompressed() {
    let path = text_pdf("smaller");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let input = unacknowledged(&document, &base, &original_bytes);

    let plain = save_document(input).expect("an ordinary save succeeds");
    let compressed =
        save_document_compressed(input, CompressPreset::Lossless).expect("and so does this one");

    assert_eq!(compressed.report.outcome(), Outcome::Reduced);
    assert!(
        compressed.bytes.len() < plain.len(),
        "compressed save produced {} bytes where the ordinary one produced {}",
        compressed.bytes.len(),
        plain.len()
    );
    assert!(
        compressed.bytes.len() < original_bytes.len(),
        "and smaller than the file it was opened from: {} bytes from {}",
        compressed.bytes.len(),
        original_bytes.len()
    );
    assert_eq!(
        compressed.report.after(),
        compressed.bytes.len() as u64,
        "the report must describe the bytes the caller got"
    );
    assert!(compressed.report.refusals().is_empty());

    let reloaded = lopdf::Document::load_mem(&compressed.bytes).expect("must reload");
    assert_eq!(reloaded.get_pages().len(), 8);
}

/// The writer choice, seen from outside. Nothing was edited, so an ordinary
/// save of this document appends a revision onto the original bytes — and a
/// compressed save cannot, because a repack is not expressible as an append.
#[test]
fn a_compressed_save_rewrites_where_an_ordinary_one_would_append() {
    let path = text_pdf("writer");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let input = unacknowledged(&document, &base, &original_bytes);

    let plain = save_document(input).expect("an ordinary save succeeds");
    assert!(
        plain.starts_with(&original_bytes),
        "with nothing edited, the ordinary save must be an incremental append"
    );

    let compressed =
        save_document_compressed(input, CompressPreset::Lossless).expect("compressed save");
    assert!(
        !compressed.bytes.starts_with(&original_bytes),
        "a compressed save is a rewrite, not an append"
    );
    assert!(
        compressed.bytes.windows(6).any(|w| w == b"ObjStm"),
        "a compressed save must pack its dictionaries into object streams"
    );
}

/// Decision 2, from the caller's side: compressing is not an edit, so it
/// leaves the undo history exactly as it found it. The document carries one
/// real undoable edit so the comparison has something to protect.
#[test]
fn compressing_never_touches_the_edit_log() {
    let path = text_pdf("edit-log");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let page0 = document.pages[0].id;
    apply_command(
        &mut document,
        Command::RotatePage {
            page: page0,
            delta_degrees: 90,
        },
    );
    let edits_before = document.pending_edits.entries().to_vec();
    assert_eq!(edits_before.len(), 1);

    save_document_compressed(
        unacknowledged(&document, &base, &original_bytes),
        CompressPreset::Small,
    )
    .expect("compressed save");

    assert_eq!(
        document.pending_edits.entries(),
        edits_before.as_slice(),
        "compression must never append to (or alter) the EditLog"
    );
    assert!(matches!(
        document.pending_edits.entries(),
        [Command::RotatePage { .. }]
    ));
}

/// The never-grow guarantee, seen from this side of the boundary and on every
/// preset: whatever the preset, the compressed save is never larger than the
/// save it compressed. `pdf-compress` enforces this structurally; this is the
/// check that the entry point here did not find a way around it.
#[test]
fn no_preset_ever_makes_a_save_bigger() {
    let path = fixture_path("content-edit", "reportlab_embedded_subset.pdf");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();

    let page0 = document.pages[0].id;
    apply_command(
        &mut document,
        Command::RotatePage {
            page: page0,
            delta_degrees: 90,
        },
    );

    let input = unacknowledged(&document, &base, &original_bytes);
    let plain = save_document(input).expect("an ordinary save succeeds");

    for preset in CompressPreset::all() {
        let compressed = save_document_compressed(input, preset).expect("compressed save");

        assert!(
            compressed.bytes.len() <= plain.len(),
            "{preset:?} produced {} bytes where the ordinary save produced {}",
            compressed.bytes.len(),
            plain.len()
        );
        assert_eq!(
            lopdf::Document::load_mem(&compressed.bytes)
                .expect("must reload")
                .get_pages()
                .len(),
            document.pages.len(),
            "{preset:?} changed the page count"
        );
    }
}

/// A `PageId` the base does not have, so the save itself fails. The point is
/// that the compressed entry point reports the *save's* failure rather than
/// swallowing it into a `NoGain` report — a compression that never ran
/// because there were no bytes to compress is not a compression that found
/// nothing to gain.
#[test]
fn a_save_that_fails_is_not_reported_as_a_compression_that_gained_nothing() {
    let path = text_pdf("failing");
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();
    document.pages[0] = pdf_document::Page::imported(
        PageId(0),
        pdf_document::ImportedDocumentId(7),
        0,
        pdf_document::PageSize::Letter,
        pdf_document::Orientation::Portrait,
        pdf_document::Rotation::None,
    );

    let result = save_document_compressed(
        unacknowledged(&document, &base, &original_bytes),
        CompressPreset::Balanced,
    );

    assert!(result.is_err());
}
