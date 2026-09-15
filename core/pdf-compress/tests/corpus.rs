//! The guardians: what must stay true of every real document in the corpus.
//!
//! Nothing here prints; every one of these fails a pull request. T-197 widens
//! the corpus itself (a scan, a pure-vector file, one with real transparency)
//! by adding rows to [`common::CORPUS`], which is why the list lives there
//! rather than in this file.
//!
//! The measurement that produced the tables in `docs/batch-compress.md` is in
//! `measure.rs`; the pixel-for-pixel comparison is in `render_unchanged.rs`.

mod common;

use common::{page_count, read, Fixture, CORPUS};
use pdf_compress::{compress, CompressPreset, Outcome, Refusal};

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
            let compressed = compress(&input, preset)
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

        let compressed = compress(&input, CompressPreset::Lossless).expect("compressible");

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
                let compressed = compress(&input, preset).expect("not an error");

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

        let once = compress(&input, CompressPreset::Lossless)
            .unwrap_or_else(|err| panic!("{} could not be compressed: {err}", fixture.path))
            .into_bytes();
        let twice = compress(&once, CompressPreset::Lossless).expect("compressible");

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
