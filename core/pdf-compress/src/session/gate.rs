//! The two documents a compression never touches, and the one way past one
//! of them.
//!
//! ## The rule this module exists to keep
//!
//! *A document this crate must not rewrite never reaches a stage.*
//!
//! Asked once, at the door, before [`super::Session`] exists — because both
//! answers are properties of the *document* rather than of any stage, and a
//! stage that had to re-ask would be a stage that could forget. What the
//! envelope owns is the parse, the tally and the write; what this owns is
//! whether there is anything to open at all.
//!
//! ## Why an encrypted document is handed straight back
//!
//! This is the trap this crate exists to not fall into, and it is worth
//! stating plainly because the never-grow guarantee does **not** catch it.
//!
//! `lopdf::Document::load_mem` on an encrypted file *succeeds*. It returns a
//! `Document` whose `objects` map holds nothing but the `/Encrypt` dictionary
//! — the reader gives up as soon as it finds no password, before unpacking a
//! single object (`pdf_manip::open`'s header documents the same gotcha for
//! the load path). Re-serialising that handle produces a small, well-formed,
//! *empty* PDF. It is smaller than the input, so [`crate::guarantee`] would
//! accept it and hand the user a file with no pages in it. The guarantee
//! protects against growth, not against annihilation.
//!
//! So the encryption check is not a policy decision made here. It is a
//! correctness check: a document this crate cannot read is a document it must
//! not rewrite. It is reported as
//! [`Refusal::EncryptedDocumentNotRewritable`] rather than thrown, because
//! being unable to repack a protected file is a fact about that file and the
//! explanation the user is owed for a disappointing result.
//!
//! The same conclusion arrives from the writer's side: an encrypted document
//! is written with every object loose no matter what is asked of it
//! (`lopdf`'s `save_with_object_streams` returns early for one), and
//! `gen-fixtures` documents that an encrypted document written with a
//! cross-reference stream cannot be decrypted on the way back in. There is no
//! version of this pass that helps an encrypted file — which is why T-195
//! could not give this refusal a yes-path either, and `pdf-save`'s `compress`
//! module records what `docs/batch-compress.md` decision 7 expected instead.
//!
//! ## Why a signed document is handed straight back too
//!
//! This one the never-grow guarantee misses in the other direction, and the
//! page-count check misses as well. It was found by *running the
//! measurement*, not by reasoning about it: repacking
//! `tests/fixtures/signed/rsa2048_sha256.pdf` produced a file 92% smaller,
//! with its single page perfectly intact. The 92% was the signature. A signed
//! PDF is a base revision plus an incremental update; loading and
//! re-serialising collapses that into one revision and leaves the
//! `/ByteRange` the signature covers describing bytes that no longer exist.
//!
//! So a signed document is handed back untouched, carrying
//! [`Refusal::SignaturesWouldBeInvalidated`] — which is exactly what that
//! variant was defined to mean at T-190: *the caller has not said it knows,
//! so the file is left alone until it does.* The saying-so arrived at T-195
//! as [`SignedDocuments::CompressAnyway`] (`docs/batch-compress.md`
//! decision 7, carried across the boundary by
//! `pdf_save::SignatureAcknowledgement`, the same channel `pdf-save`'s
//! `content.rs` already uses to warn before a full rewrite). The default is
//! still the safe one, because the alternative is a user shown "92% smaller!"
//! over a document whose signature is gone.
//!
//! The two refusals are therefore **not** symmetrical, and no value the
//! caller can pass makes them so. A signature is information the user owns
//! and may choose to spend; an encrypted document is one this crate cannot
//! read, so consenting to compress it would consent to receiving an empty
//! file. See [`crate::consent`].
//!
//! Detection is `pdf_manip::document_has_signatures`, not a second opinion
//! written here — the same reason `pdf-save` borrows it rather than asking
//! the object graph itself.

use lopdf::Document;
use pdf_manip::{document_has_signatures, LopdfDocument};

use crate::consent::SignedDocuments;
use crate::error::CompressError;
use crate::report::Refusal;

use super::Session;

/// What [`open`] found: a document to work on, or a reason not to.
pub(crate) enum Opened {
    /// The document may be rewritten. The stages run.
    Ready(Box<Session>),
    /// The document must be handed back exactly as it arrived, for the reason
    /// carried here. See this module's header — one reason is a correctness
    /// stop, the other a decision the caller declined to make.
    Refused(Refusal),
}

/// Opens `input` for compression, or says why it cannot be.
///
/// # Errors
///
/// [`CompressError::Lopdf`] when `input` is not a document that can be read.
pub(crate) fn open(input: &[u8], signed: SignedDocuments) -> Result<Opened, CompressError> {
    let document = Document::load_mem(input)?;

    // Asked first, and asked of every caller: `signed` says nothing about
    // this one. See this module's header for why the two refusals are not
    // symmetrical.
    if document.is_encrypted() {
        return Ok(Opened::Refused(Refusal::EncryptedDocumentNotRewritable));
    }

    let document = LopdfDocument::from_lopdf(document);
    if signed == SignedDocuments::LeaveAlone && document_has_signatures(&document) {
        return Ok(Opened::Refused(Refusal::SignaturesWouldBeInvalidated));
    }
    let document = document.into_lopdf();

    Ok(Opened::Ready(Box::new(Session::new(document))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guarantee::Candidate;
    use crate::pipeline;
    use crate::preset::CompressPreset;
    use crate::report::Work;
    use crate::test_fixtures::{encrypted_document, loose_document, signed_document};

    fn lossless(input: &[u8], signed: SignedDocuments) -> Candidate {
        pipeline::run(input, CompressPreset::Lossless, signed).expect("a readable document")
    }

    fn reload(bytes: &[u8]) -> Document {
        Document::load_mem(bytes).expect("a candidate must be a readable document")
    }

    /// The document that keeps its page and loses what made it worth
    /// anything. Measured on the real fixtures before the check existed:
    /// `tests/fixtures/signed/rsa2048_sha256.pdf` came back 92% smaller with
    /// every page intact, because the 92% *was* the signature.
    #[test]
    fn a_signed_document_is_handed_back_untouched() {
        let input = signed_document();

        let candidate = lossless(&input, SignedDocuments::LeaveAlone);

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

    /// T-195, the path of yes. The refusal above is the default, not a
    /// policy: a caller who has told the user the signature will stop
    /// verifying is asking for a compression, and getting `NoGain` back
    /// instead would read as a bug in the shell that asked.
    #[test]
    fn a_signed_document_is_compressed_when_the_caller_says_it_knows() {
        let input = signed_document();

        let candidate = lossless(&input, SignedDocuments::CompressAnyway);

        assert!(
            candidate.bytes().len() < input.len(),
            "consented compression produced {} bytes from {}",
            candidate.bytes().len(),
            input.len()
        );
        assert_eq!(
            candidate.refusals(),
            &[],
            "nothing was refused: the caller asked for exactly this"
        );
        assert_eq!(reload(candidate.bytes()).get_pages().len(), 2);
    }

    /// The counterpart, so the check above cannot be a blanket refusal that
    /// happens to pass its own test.
    #[test]
    fn an_unsigned_document_is_repacked_as_usual() {
        let input = loose_document(4);

        let candidate = lossless(&input, SignedDocuments::LeaveAlone);

        assert!(candidate.refusals().is_empty());
        assert!(candidate.bytes().len() < input.len());
    }

    /// The one that matters. An encrypted document loads as an *empty*
    /// document, so repacking it would hand back a small, valid, page-less
    /// file — and the never-grow guarantee would happily accept it.
    #[test]
    fn an_encrypted_document_is_handed_back_untouched() {
        let input = encrypted_document();

        let candidate = lossless(&input, SignedDocuments::LeaveAlone);

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

    /// The consent is about signatures and about nothing else. An encrypted
    /// document loads with no objects in it, so "compress anyway" would write
    /// out an empty file — there is no value a caller can pass to reach that.
    #[test]
    fn consent_to_invalidate_a_signature_does_not_unlock_an_encrypted_document() {
        let input = encrypted_document();

        let candidate = lossless(&input, SignedDocuments::CompressAnyway);

        assert_eq!(candidate.bytes(), input.as_slice());
        assert_eq!(
            candidate.refusals(),
            &[Refusal::EncryptedDocumentNotRewritable]
        );
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
    fn bytes_that_are_not_a_document_are_an_error() {
        assert!(matches!(
            open(b"this has never been a PDF", SignedDocuments::LeaveAlone),
            Err(CompressError::Lopdf(_))
        ));
    }
}
