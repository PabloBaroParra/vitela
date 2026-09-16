//! The order the stages run in, and nothing else.
//!
//! ## The rule this module exists to keep
//!
//! *Each stage decides what it does; this module decides only when it runs.*
//!
//! Kept thin on purpose. A stage that knows about the stage after it is a
//! stage that cannot be tested on its own, and this is the file every future
//! task edits — so the less it holds, the less each of those tasks can break.
//! The load, the refusals and the write are not here either: they are
//! [`crate::session`], the envelope every stage runs inside.
//!
//! What runs, and in what order:
//!
//! - **T-191**, [`structural`] — flate over streams that arrived unfiltered.
//!   (The other two levers of the repack, object streams and a
//!   cross-reference stream, are the write format rather than a stage; they
//!   live in [`crate::session`].)
//! - **T-193**, [`images`] — take stock of every image the document draws and
//!   measure it against the paper. Runs only under a preset that has an image
//!   policy, because it is the first stage a preset can switch off: measuring
//!   costs an interpreter walk per page, and
//!   [`CompressPreset::Lossless`](crate::CompressPreset::Lossless) has
//!   already promised not to touch a pixel with whatever it finds.
//! - **T-192**, [`prune`] — merge duplicate objects, then delete everything
//!   the trailer cannot reach.
//!
//! **Why the prune runs last.** It is the stage that decides what gets
//! written, so it has to see the graph every earlier stage left behind. Run
//! before the flate pass it would be equally correct and equally useless;
//! run before T-194's image stage it would miss the image XObjects that
//! resampling makes identical. Last is where it collects everything.
//!
//! Still to land here:
//!
//! - **T-194** — the resampler, reading the inventory T-193's stage builds and
//!   rewriting the images it finds oversized for
//!   [`CompressPreset::image_policy`](crate::CompressPreset::image_policy)'s
//!   target. It replaces the [`images`] call below rather than joining it.

use crate::error::CompressError;
use crate::guarantee::Candidate;
use crate::preset::CompressPreset;
use crate::session::{self, Opened};
use crate::{images, prune, structural};

/// The compression pipeline, as far as it has been built.
///
/// Returns an *offer*. [`crate::guarantee`] decides whether it ships.
//
// The preset is read for one thing so far — whether images are in scope at
// all. The numbers inside the policy are T-194's; every preset still repacks
// and prunes the same way.
pub(crate) fn run(input: &[u8], preset: CompressPreset) -> Result<Candidate, CompressError> {
    let mut session = match session::open(input)? {
        Opened::Ready(session) => session,
        Opened::Refused(refusal) => {
            return Ok(Candidate::new(input.to_vec()).refusing(refusal));
        }
    };

    let work = structural::pass(session.document_mut());
    session.record(work);

    if preset.image_policy().is_some() {
        let work = images::pass(session.document_mut());
        session.record(work);
    }

    let work = prune::pass(session.document_mut());
    session.record(work);

    session.finish(input)
}

#[cfg(test)]
mod tests {
    use lopdf::Document;

    use super::*;
    use crate::report::{Refusal, Work};
    use crate::test_fixtures::{encrypted_document, loose_document, signed_document};

    fn reload(bytes: &[u8]) -> Document {
        Document::load_mem(bytes).expect("a candidate must be a readable document")
    }

    fn lossless(input: &[u8]) -> Candidate {
        run(input, CompressPreset::Lossless).expect("a plain document runs the pipeline")
    }

    #[test]
    fn a_loose_document_comes_back_packed_and_smaller() {
        let input = loose_document(12);

        let candidate = lossless(&input);

        assert!(
            candidate.bytes().len() < input.len(),
            "packing 12 loose objects produced {} bytes from {}",
            candidate.bytes().len(),
            input.len()
        );
    }

    #[test]
    fn the_repacked_document_uses_object_streams_and_an_xref_stream() {
        let candidate = lossless(&loose_document(12));

        assert!(
            matches!(
                reload(candidate.bytes())
                    .reference_table
                    .cross_reference_type,
                lopdf::xref::XrefType::CrossReferenceStream
            ),
            "the candidate must carry a cross-reference stream, not a classic table"
        );
        assert!(
            candidate.bytes().windows(6).any(|w| w == b"ObjStm"),
            "the candidate must pack its dictionaries into object streams"
        );
    }

    #[test]
    fn a_stream_that_arrived_unfiltered_is_flated_and_counted() {
        assert_eq!(lossless(&loose_document(3)).work().streams_recompressed, 3);
    }

    /// The pipeline's own contract, independent of what any stage achieves:
    /// whatever comes out is still a readable document with the same pages in
    /// it. Each stage checks its own effect; this checks the chain, so a
    /// later stage cannot quietly lose a page between two passes that each
    /// kept them.
    #[test]
    fn every_preset_returns_a_document_with_the_same_pages() {
        let input = loose_document(4);

        for preset in CompressPreset::all() {
            let candidate = run(&input, preset).expect("a plain document runs the pipeline");

            assert_eq!(
                reload(candidate.bytes()).get_pages().len(),
                4,
                "{preset:?} lost a page somewhere in the pipeline"
            );
        }
    }

    /// The pipeline has to be a fixed point: a document that already arrived
    /// packed must not come back bigger for having been packed again.
    ///
    /// Before T-192 this failed, and the cause was one dead object per round
    /// trip — see [`crate::prune`]'s header for which one and why `lopdf`
    /// leaves it behind. The never-grow guarantee hid it: it caught the
    /// inflated candidate and handed the original back, so no user saw a
    /// bigger file and nobody saw the bug either.
    #[test]
    fn a_document_that_arrives_packed_does_not_come_back_bigger() {
        let once = lossless(&loose_document(8)).bytes().to_vec();

        let twice = lossless(&once);

        assert!(
            twice.bytes().len() <= once.len(),
            "repacking a packed document grew it from {} to {} bytes",
            once.len(),
            twice.bytes().len()
        );
    }

    /// The strong form of the same statement, and the one worth holding on
    /// to: the pipeline is not merely non-inflating on a packed document, it
    /// is a *fixed point*. Running it twice produces the same bytes as
    /// running it once, so there is nothing left over for a third run to find.
    #[test]
    fn repacking_a_packed_document_produces_the_very_same_bytes() {
        let once = lossless(&loose_document(8)).bytes().to_vec();

        let twice = lossless(&once);

        assert_eq!(
            twice.bytes(),
            once.as_slice(),
            "a second repack changed {} bytes into {}",
            once.len(),
            twice.bytes().len()
        );
    }

    /// The same failure named by its cause rather than by its symptom, so
    /// that a fix which merely happens to win the bytes back cannot satisfy
    /// it. A document has exactly one cross-reference stream; every other
    /// `/Type /XRef` object in it is a corpse.
    #[test]
    fn only_one_cross_reference_object_survives_a_repack() {
        let once = lossless(&loose_document(8)).bytes().to_vec();

        let twice = lossless(&once);

        let corpses = reload(twice.bytes())
            .objects
            .values()
            .filter(|object| object.type_name().ok() == Some(b"XRef"))
            .count();
        assert_eq!(
            corpses, 1,
            "every repack left the previous generation's xref stream behind"
        );
    }

    /// The document that keeps its page and loses what made it worth
    /// anything. Measured on the real fixtures before the check existed:
    /// `tests/fixtures/signed/rsa2048_sha256.pdf` came back 92% smaller with
    /// every page intact, because the 92% *was* the signature.
    #[test]
    fn a_signed_document_is_handed_back_untouched() {
        let input = signed_document();

        let candidate = lossless(&input);

        assert_eq!(
            candidate.bytes(),
            input.as_slice(),
            "a signed document must come back byte for byte, not repacked"
        );
        assert_eq!(
            candidate.refusals(),
            &[Refusal::SignaturesWouldBeInvalidated]
        );
        assert_eq!(candidate.work(), Work::default());
    }

    /// The counterpart, so the check above cannot be a blanket refusal that
    /// happens to pass its own test.
    #[test]
    fn an_unsigned_document_is_repacked_as_usual() {
        let candidate = lossless(&loose_document(4));

        assert!(candidate.refusals().is_empty());
        assert!(candidate.bytes().len() < loose_document(4).len());
    }

    /// The one that matters. An encrypted document loads as an *empty*
    /// document, so repacking it would hand back a small, valid, page-less
    /// file — and the never-grow guarantee would happily accept it.
    #[test]
    fn an_encrypted_document_is_handed_back_untouched() {
        let input = encrypted_document();

        let candidate = lossless(&input);

        assert_eq!(
            candidate.bytes(),
            input.as_slice(),
            "an encrypted document must come back byte for byte, not repacked"
        );
        assert_eq!(
            candidate.refusals(),
            &[Refusal::EncryptedDocumentNotRewritable]
        );
        assert_eq!(candidate.work(), Work::default());
    }

    /// The premise the refusal rests on, pinned so it cannot rot silently: if
    /// a future `lopdf` starts populating an encrypted document's objects
    /// without a password, this test goes red and the refusal is worth
    /// revisiting. Until then, repacking that handle writes out an empty file.
    #[test]
    fn an_encrypted_document_really_does_load_as_an_empty_one() {
        let document = reload(&encrypted_document());

        assert!(document.is_encrypted());
        assert_eq!(document.get_pages().len(), 0);
    }

    #[test]
    fn a_document_that_cannot_be_read_stops_the_pipeline() {
        let result = run(b"not a document", CompressPreset::Lossless);

        assert!(matches!(result, Err(CompressError::Lopdf(_))));
    }
}
