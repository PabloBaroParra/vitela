//! The guardians: what must stay true of every real document in the corpus.
//!
//! Nothing here prints; every one of these fails a pull request. The corpus
//! itself lives in [`common::CORPUS`] rather than in this file, so T-197's
//! rows — a scan, a reused XObject, real transparency, a pure-vector page and
//! a file with no slack left — were added once and every harness picked them
//! up.
//!
//! What belongs here is what must be true of *every* document. What one
//! particular fixture was added to prove is next door in `image_stage.rs`.
//! The measurement that produced the tables in `docs/batch-compress.md` is in
//! `measure.rs`; the pixel-for-pixel comparison is in `render_unchanged.rs`.

mod common;

use common::{fixture, page_count, read, Fixture, CORPUS};
use pdf_compress::{compress, CompressPreset, Outcome, Refusal, SignedDocuments};

/// The guarantee, against files nobody in this repository wrote to be
/// compressible. T-197 owns the widened corpus; this is the same assertion on
/// the corpus that exists today.
#[test]
fn no_real_document_grows_under_any_preset() {
    for fixture in CORPUS {
        let Some(input) = read(fixture) else {
            continue;
        };

        for preset in CompressPreset::all() {
            let compressed = compress(&input, preset, SignedDocuments::LeaveAlone)
                .unwrap_or_else(|err| panic!("{} could not be compressed: {err}", fixture.path));

            assert!(
                compressed.bytes().len() <= input.len(),
                "{preset:?} grew {} from {} to {} bytes",
                fixture.path,
                input.len(),
                compressed.bytes().len()
            );
        }
    }
}

/// The failure the never-grow rule cannot see: a smaller file with less in
/// it. Every document that came back changed must still hold every page it
/// arrived with.
#[test]
fn no_real_document_loses_a_page() {
    for fixture in CORPUS {
        let Some(input) = read(fixture) else {
            continue;
        };
        let Some(before) = page_count(&input) else {
            continue;
        };

        let compressed = compress(
            &input,
            CompressPreset::Lossless,
            SignedDocuments::LeaveAlone,
        )
        .expect("compressible");

        assert_eq!(
            page_count(compressed.bytes()),
            Some(before),
            "{} came back with a different number of pages",
            fixture.path
        );
    }
}

/// The two traps, on the real protected files rather than built ones. A
/// document this crate cannot repack without taking something away must come
/// back byte for byte, with the refusal that explains why nothing happened.
///
/// The signed half is the one the measurement found: before the check landed,
/// `rsa2048_sha256.pdf` compressed to 92% smaller with its page intact,
/// because the 92% was the signature.
#[test]
fn a_protected_document_comes_back_exactly_as_it_arrived() {
    let cases = [
        ("/encrypted/", Refusal::EncryptedDocumentNotRewritable),
        ("/signed/", Refusal::SignaturesWouldBeInvalidated),
    ];

    for (kind, expected) in cases {
        let matching: Vec<&Fixture> = CORPUS
            .iter()
            .filter(|fixture| fixture.path.contains(kind))
            .collect();
        assert!(
            !matching.is_empty(),
            "the corpus must keep at least one {kind} document"
        );

        for fixture in matching {
            let input = read(fixture).expect("the protected fixtures are committed");

            for preset in CompressPreset::all() {
                let compressed =
                    compress(&input, preset, SignedDocuments::LeaveAlone).expect("not an error");

                assert_eq!(
                    compressed.bytes(),
                    input.as_slice(),
                    "{} was rewritten; a protected document must be handed back untouched",
                    fixture.path
                );
                assert_eq!(compressed.report().outcome(), Outcome::NoGain);
                assert_eq!(
                    compressed.report().refusals(),
                    std::slice::from_ref(&expected),
                    "{} must say why it was left alone",
                    fixture.path
                );
            }
        }
    }
}

/// Decision 3's "no toca un solo píxel", taken all the way: the lossless
/// preset does not merely leave images alone, it never asks about them. A
/// non-zero count here would mean the image stage ran under a preset that has
/// promised it would not, which is the cheapest possible early warning that
/// the pipeline's gate has come loose.
#[test]
fn lossless_never_looks_at_an_image() {
    for fixture in CORPUS {
        let Some(input) = read(fixture) else {
            continue;
        };

        let compressed = compress(
            &input,
            CompressPreset::Lossless,
            SignedDocuments::LeaveAlone,
        )
        .expect("compressible");

        assert_eq!(
            compressed.report().work().images_skipped,
            0,
            "{} reported image work under a preset that does not touch images",
            fixture.path
        );
    }
}

/// T-193's inventory, against a document this repository did not write to be
/// convenient: every page of `perf_200pg.pdf` paints a raster, and the stage
/// has to find them through `pdf-edit`'s interpreter rather than by reading a
/// resource dictionary.
///
/// It is the end-to-end proof that the reuse works on real producer output —
/// a hand-built fixture can only show that the arithmetic is right. Skipped
/// in a checkout that has not run `gen-fixtures`, like every other generated
/// row.
#[test]
fn a_raster_heavy_document_reports_the_images_the_pass_left_alone() {
    let entry = fixture("perf_200pg.pdf");
    let Some(input) = read(entry) else {
        return;
    };

    let compressed = compress(
        &input,
        CompressPreset::Balanced,
        SignedDocuments::LeaveAlone,
    )
    .expect("compressible");

    assert_eq!(compressed.report().outcome(), Outcome::Reduced);
    assert!(
        compressed.report().work().images_skipped > 0,
        "{} paints a raster on every page and the inventory found none",
        entry.path
    );
    assert_eq!(
        compressed.report().work().images_resampled,
        0,
        "every raster in this corpus is already below the lowest preset's 96 dpi — \
         `perf_200pg.pdf` draws 316 samples across a full page, which is 37 — so the \
         resampler has nothing to do here and must not invent something. The fixture \
         that would exercise it is T-197's."
    );
}

/// The other end of the never-grow guarantee, on a file that has nothing
/// left to give.
///
/// `already_packed.pdf` arrives the way a compressed file leaves: object
/// streams, a cross-reference stream, every stream flated, nothing orphaned
/// and nothing duplicated. Every stage may run over it and every stage will
/// find nothing, so the only honest answer is the user's own bytes back —
/// not an equivalent re-serialisation of them, and not one byte more.
///
/// It is worth a row of its own because the corpus's other route to
/// [`Outcome::NoGain`] is a *refusal*: the encrypted and signed files come
/// back untouched because the crate would not rewrite them. This one comes
/// back untouched having been rewritten and measured, with no refusal to
/// explain it. Those are two different code paths reaching the same word, and
/// before this fixture only the first had a real file behind it.
#[test]
fn a_document_with_no_slack_left_comes_back_byte_for_byte() {
    let entry = fixture("already_packed.pdf");
    let input = read(entry).expect("the compression corpus is committed");

    for preset in CompressPreset::all() {
        let compressed = compress(&input, preset, SignedDocuments::LeaveAlone)
            .unwrap_or_else(|err| panic!("{} could not be compressed: {err}", entry.path));

        assert_eq!(
            compressed.bytes(),
            input.as_slice(),
            "{preset:?} handed back {} bytes for a {}-byte document that had              nothing left to win",
            compressed.bytes().len(),
            input.len()
        );
        assert_eq!(compressed.report().outcome(), Outcome::NoGain);
        assert!(
            compressed.report().refusals().is_empty(),
            "{preset:?} refused a document it was free to rewrite; NoGain here              must mean there was nothing to gain, not that nothing was tried"
        );
    }
}

/// T-192's fixed point, on the real corpus rather than on a built fixture.
///
/// Compressing a document this crate already compressed must return the very
/// same bytes: [`Outcome::NoGain`], nothing dropped, nothing recompressed.
/// Before the prune landed it failed here for a reason no user could ever
/// have seen — the second pass produced a *larger* file, the never-grow
/// guarantee caught it and handed the original back, and the outcome read
/// `NoGain` either way. The guarantee was covering for a repack that leaked
/// one dead cross-reference object and two object ids per round trip.
#[test]
fn compressing_a_compressed_document_is_a_no_op_on_every_fixture() {
    for fixture in CORPUS {
        let Some(input) = read(fixture) else {
            continue;
        };

        let once = compress(
            &input,
            CompressPreset::Lossless,
            SignedDocuments::LeaveAlone,
        )
        .unwrap_or_else(|err| panic!("{} could not be compressed: {err}", fixture.path))
        .into_bytes();
        let twice = compress(&once, CompressPreset::Lossless, SignedDocuments::LeaveAlone)
            .expect("compressible");

        assert_eq!(
            twice.bytes(),
            once.as_slice(),
            "{} was not a fixed point: {} bytes became {}",
            fixture.path,
            once.len(),
            twice.bytes().len()
        );
        assert_eq!(twice.report().outcome(), Outcome::NoGain);
        assert_eq!(twice.report().work(), pdf_compress::Work::default());
    }
}
