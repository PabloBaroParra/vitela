//! The structural pass: the same document, packed the way PDF 1.5 allows.
//!
//! ## The rule this module exists to keep
//!
//! *Not one byte a reader can see changes. Only how those bytes are stored.*
//!
//! Three levers, all of them already in `lopdf` (`docs/batch-compress.md`
//! fact 3), none of them touching a pixel or a glyph:
//!
//! - **object streams** — the dictionaries that describe pages, fonts and
//!   resources stop being individually serialised ASCII and get packed into a
//!   flate-compressed container.
//! - **a cross-reference stream** — the offset table stops being a
//!   twenty-bytes-per-object ASCII listing and becomes a compressed binary
//!   one.
//! - **flate over streams that arrived unfiltered** — a content stream
//!   written in the clear is re-encoded. A stream that already carries a
//!   `/Filter` is never touched, because re-filtering compressed bytes grows
//!   them.
//!
//! This is what [`CompressPreset::Lossless`](crate::CompressPreset::Lossless)
//! is, and what the other two presets do before they touch an image.
//!
//! ## Why an encrypted document is handed straight back
//!
//! This is the trap this module exists to not fall into, and it is worth
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
//! So the encryption check is not a policy decision made here — the policy
//! gate is `pdf-save`'s `full_rewrite_blocker` at T-195
//! (`docs/batch-compress.md` decision 7). It is a correctness check: a
//! document this crate cannot read is a document it must not rewrite. It is
//! reported as [`Refusal::EncryptedDocumentNotRewritable`] rather than thrown,
//! because being unable to repack a protected file is a fact about that file
//! and the explanation the user is owed for a disappointing result.
//!
//! The same conclusion arrives from the writer's side: an encrypted document
//! is written with every object loose no matter what is asked of it
//! (`lopdf`'s `save_with_object_streams` returns early for one), and
//! `gen-fixtures` documents that an encrypted document written with a
//! cross-reference stream cannot be decrypted on the way back in. There is no
//! version of this pass that helps an encrypted file.
//!
//! ## Why a signed document is handed straight back too
//!
//! This one the never-grow guarantee misses in the other direction, and the
//! page-count check misses as well. It was found by *running the
//! measurement*, not by reasoning about it: repacking
//! `tests/fixtures/signed/rsa2048_sha256.pdf` produced a file 92% smaller,
//! with its single page perfectly intact. The 92% was the signature. A signed
//! PDF is a base revision plus an incremental update; loading and
//! re-serialising collapses that into one revision and leaves the `/ByteRange`
//! the signature covers describing bytes that no longer exist.
//!
//! So a signed document is handed back untouched, carrying
//! [`Refusal::SignaturesWouldBeInvalidated`] — which is exactly what that
//! variant was defined to mean at T-190: *the caller has not said it knows,
//! so the file is left alone until it does.* T-195 is what adds the saying-so
//! (`docs/batch-compress.md` decision 7, the same channel `pdf-save`'s
//! `content.rs` already uses to warn before a full rewrite). Until it lands,
//! the default is the safe one, because the alternative is a user shown "92%
//! smaller!" over a document whose signature is gone.
//!
//! Detection is `pdf_manip::document_has_signatures`, not a second opinion
//! written here — the same reason `pdf-save` borrows it rather than asking
//! the object graph itself.
//!
//! ## Why the candidate is re-read before it is offered
//!
//! [`crate::guarantee`] compares sizes, and the smallest possible PDF is a
//! broken one. A repack that loses pages would win that comparison. So this
//! pass reloads what it just wrote and counts the pages before offering it;
//! if the count moved, the candidate is dropped and the input is offered
//! unchanged. One extra parse per compression, in exchange for the class of
//! bug where a user's document comes back lighter because part of it is gone.

use lopdf::{Document, Object, SaveOptions};
use pdf_manip::{document_has_signatures, LopdfDocument};

use crate::error::CompressError;
use crate::guarantee::Candidate;
use crate::report::{Refusal, Work};

/// Repacks `input` without changing what it draws.
///
/// Returns a [`Candidate`] — an *offer*. Whether these bytes reach the caller
/// is [`crate::guarantee`]'s decision, not this module's.
///
/// # Errors
///
/// [`CompressError::Lopdf`] when `input` is not a document that can be read,
/// and [`CompressError::Io`] when the repacked document cannot be serialised.
pub(crate) fn pass(input: &[u8]) -> Result<Candidate, CompressError> {
    let document = Document::load_mem(input)?;

    // See this module's header: neither of these is a policy, both are
    // correctness stops. A document this pass cannot repack without taking
    // something away is a document it hands back.
    if document.is_encrypted() {
        return Ok(Candidate::new(input.to_vec()).refusing(Refusal::EncryptedDocumentNotRewritable));
    }

    let document = LopdfDocument::from_lopdf(document);
    if document_has_signatures(&document) {
        return Ok(Candidate::new(input.to_vec()).refusing(Refusal::SignaturesWouldBeInvalidated));
    }
    let mut document = document.into_lopdf();

    let pages_before = document.get_pages().len();
    let streams_recompressed = flate_unfiltered_streams(&mut document);

    let mut bytes = Vec::with_capacity(input.len());
    document.save_with_options(&mut bytes, repacked())?;

    if page_count(&bytes) != Some(pages_before) {
        return Ok(Candidate::new(input.to_vec()));
    }

    Ok(Candidate::new(bytes).with_work(Work {
        streams_recompressed,
        ..Work::default()
    }))
}

/// The write format this pass exists to ask for.
///
/// Built as a struct literal rather than through `SaveOptions::builder()` on
/// purpose: the builder's `compression_level` defaults to `0` and `build()`
/// passes it straight through, so a builder nobody remembered to call
/// `.compression_level()` on packs the object streams *uncompressed* — the
/// opposite of the point. `ObjectStreamConfig::default()` is level 6.
///
/// `linearize` stays off. Fast-web-view interleaving is a layout for
/// streaming a file over a network, not a size reduction; it adds a hint
/// table and can make the file bigger.
fn repacked() -> SaveOptions {
    SaveOptions {
        use_object_streams: true,
        use_xref_streams: true,
        linearize: false,
        ..SaveOptions::default()
    }
}

/// Flate-encodes every stream that arrived without a `/Filter`, and says how
/// many actually changed.
///
/// `lopdf::Document::compress` does this walk already, but it returns `()`,
/// so a caller cannot tell one stream from four hundred and the report would
/// have to guess. The walk is three lines; the count is the reason it is
/// here.
///
/// Two guards, both `lopdf`'s and both kept:
///
/// - `allows_compression` is how a stream that must stay raw (a font
///   program's, classically) says so.
/// - `Stream::compress` only swaps its content in if the flated bytes are
///   actually shorter, so an incompressible stream is left alone rather than
///   re-encoded into something larger. That is the never-grow rule at a third
///   scale, and it is already written down inside `lopdf`.
fn flate_unfiltered_streams(document: &mut Document) -> usize {
    let mut recompressed = 0;

    for object in document.objects.values_mut() {
        let Object::Stream(stream) = object else {
            continue;
        };
        if !stream.allows_compression || stream.dict.get(b"Filter").is_ok() {
            continue;
        }

        // Errors are per-stream and not fatal: a stream that will not encode
        // is a stream that stays as it was, exactly as `Document::compress`
        // treats it.
        let _ = stream.compress();

        if stream.dict.get(b"Filter").is_ok() {
            recompressed += 1;
        }
    }

    recompressed
}

/// How many pages `bytes` has, or `None` if it is not a readable document.
///
/// The only question asked of the candidate before it is offered. Deliberately
/// not "does it render identically" — that is T-192's job, and it needs a
/// rasteriser this crate does not depend on. This one catches the failure that
/// the never-grow guarantee would otherwise reward.
fn page_count(bytes: &[u8]) -> Option<usize> {
    Document::load_mem(bytes)
        .ok()
        .map(|document| document.get_pages().len())
}

#[cfg(test)]
pub(crate) mod tests {
    use lopdf::content::{Content, Operation};
    use lopdf::xref::XrefType;
    use lopdf::{
        dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
    };

    use super::*;

    /// A document with `pages` pages, each carrying its own unfiltered
    /// content stream, written the way `pdf-save` writes one today: every
    /// object loose, classic cross-reference table (`docs/batch-compress.md`
    /// fact 2).
    pub(crate) fn loose_document(pages: usize) -> Vec<u8> {
        let mut document = Document::with_version("1.5");
        document.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = document.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let mut kids = Vec::with_capacity(pages);
        for page in 0..pages {
            // Several lines per page, not one. `lopdf`'s `Stream::compress`
            // only swaps flated bytes in when they actually win by more than
            // the nineteen bytes a `/FlateDecode` entry costs, so a one-line
            // content stream is correctly left raw — and a fixture built from
            // one would test nothing about recompression.
            let mut operations = vec![Operation::new("BT", vec![])];
            for line in 0..20 {
                operations.push(Operation::new("Tf", vec!["F1".into(), 12.into()]));
                operations.push(Operation::new(
                    "Td",
                    vec![72.into(), (720 - line * 14).into()],
                ));
                operations.push(Operation::new(
                    "Tj",
                    vec![Object::string_literal(format!(
                        "page {page}, line {line}: text repetitive enough that flate has something to say about it"
                    ))],
                ));
            }
            operations.push(Operation::new("ET", vec![]));
            let content = Content { operations };
            let content_id = document.add_object(Stream::new(
                dictionary! {},
                content.encode().expect("a fixture content stream encodes"),
            ));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            });
            kids.push(page_id.into());
        }

        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => pages as i64,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);

        serialise(&mut document)
    }

    fn serialise(document: &mut Document) -> Vec<u8> {
        let mut bytes = Vec::new();
        document
            .save_to(&mut bytes)
            .expect("a fixture document serialises");
        bytes
    }

    /// The same document, carrying a signature dictionary — the shape
    /// `pdf_manip::document_has_signatures` recognises, and the shape
    /// `tests/fixtures/signed/` holds. `tests/corpus.rs` runs the same
    /// assertion against those real files.
    fn signed_document() -> Vec<u8> {
        let mut document = Document::load_mem(&loose_document(2)).expect("the plain form loads");
        document.add_object(dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
            "SubFilter" => "adbe.pkcs7.detached",
        });

        serialise(&mut document)
    }

    /// The same one-page document, encrypted with RC4-128 and a real
    /// password — the shape `tests/fixtures/encrypted/` holds, built inline so
    /// this module's most important test does not reach two directories up
    /// for a file.
    fn encrypted_document() -> Vec<u8> {
        let mut document = Document::load_mem(&loose_document(1)).expect("the plain form loads");

        // Required by the standard security handler's key derivation
        // (PDF 32000-1:2008 §7.6.3.3); a fixed value is fine in a test.
        let file_id = Object::string_literal("structural-pass-fixture-id");
        document.trailer.set("ID", vec![file_id.clone(), file_id]);
        document.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

        let state = EncryptionState::try_from(EncryptionVersion::V2 {
            document: &document,
            owner_password: "owner-pass",
            user_password: "user-pass",
            key_length: 128,
            permissions: Permissions::all(),
        })
        .expect("a V2 encryption state is buildable");
        document.encrypt(&state).expect("the fixture encrypts");

        serialise(&mut document)
    }

    fn reload(bytes: &[u8]) -> Document {
        Document::load_mem(bytes).expect("a candidate must be a readable document")
    }

    #[test]
    fn a_loose_document_comes_back_packed_and_smaller() {
        let input = loose_document(12);

        let candidate = pass(&input).expect("a plain document is repackable");

        assert!(
            candidate.bytes().len() < input.len(),
            "packing {} loose objects produced {} bytes from {}",
            12,
            candidate.bytes().len(),
            input.len()
        );
    }

    #[test]
    fn the_repacked_document_uses_object_streams_and_an_xref_stream() {
        let candidate = pass(&loose_document(12)).expect("repackable");
        let repacked = reload(candidate.bytes());

        assert!(
            matches!(
                repacked.reference_table.cross_reference_type,
                XrefType::CrossReferenceStream
            ),
            "the candidate must carry a cross-reference stream, not a classic table"
        );
        assert!(
            candidate.bytes().windows(6).any(|w| w == b"ObjStm"),
            "the candidate must pack its dictionaries into object streams"
        );
    }

    #[test]
    fn every_page_survives_the_repack() {
        let candidate = pass(&loose_document(12)).expect("repackable");

        assert_eq!(reload(candidate.bytes()).get_pages().len(), 12);
    }

    #[test]
    fn a_stream_that_arrived_unfiltered_leaves_flated_and_counted() {
        let candidate = pass(&loose_document(3)).expect("repackable");

        assert_eq!(
            candidate.work().streams_recompressed,
            3,
            "three unfiltered content streams arrived; three should be flated"
        );
    }

    #[test]
    fn a_stream_that_already_carried_a_filter_is_left_alone() {
        let mut document = Document::load_mem(&loose_document(3)).expect("loads");
        let flated = flate_unfiltered_streams(&mut document);
        assert_eq!(flated, 3, "the first pass flates what arrived unfiltered");

        assert_eq!(
            flate_unfiltered_streams(&mut document),
            0,
            "a second pass must not re-filter already-compressed bytes"
        );
    }

    /// `lopdf`'s own never-grow rule, borrowed rather than re-implemented: a
    /// stream too short for flate to win keeps its raw bytes and is not
    /// counted as work.
    #[test]
    fn an_incompressible_stream_is_not_counted_as_recompressed() {
        let mut document = Document::with_version("1.5");
        document.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));

        assert_eq!(flate_unfiltered_streams(&mut document), 0);
    }

    /// The document that keeps its page and loses what made it worth
    /// anything. Measured on the real fixtures before this check existed:
    /// `tests/fixtures/signed/rsa2048_sha256.pdf` came back 92% smaller with
    /// every page intact, because the 92% *was* the signature.
    #[test]
    fn a_signed_document_is_handed_back_untouched() {
        let input = signed_document();

        let candidate = pass(&input).expect("a signed document is not an error here");

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
        let candidate = pass(&loose_document(4)).expect("repackable");

        assert!(candidate.refusals().is_empty());
        assert!(candidate.bytes().len() < loose_document(4).len());
    }

    /// The one that matters. An encrypted document loads as an *empty*
    /// document, so repacking it would hand back a small, valid, page-less
    /// file — and the never-grow guarantee would happily accept it.
    #[test]
    fn an_encrypted_document_is_handed_back_untouched() {
        let input = encrypted_document();

        let candidate = pass(&input).expect("an encrypted document is not an error here");

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

    #[test]
    fn bytes_that_are_not_a_document_are_an_error() {
        let result = pass(b"this has never been a PDF");

        assert!(matches!(result, Err(CompressError::Lopdf(_))));
    }

    /// The guard behind the re-read, exercised directly: unreadable bytes
    /// have no page count at all, and the guarantee would otherwise reward
    /// them for being small.
    #[test]
    fn only_a_readable_document_has_a_page_count() {
        assert_eq!(page_count(&loose_document(3)), Some(3));
        assert_eq!(page_count(b"not a document at all"), None);
        assert_eq!(page_count(b"%PDF-1.7\n%%EOF\n"), None);
    }

    /// `SaveOptions::builder()` leaves `compression_level` at zero, which
    /// writes *uncompressed* object streams. This pins that the pass does not
    /// use it.
    #[test]
    fn the_write_options_ask_for_real_compression() {
        let options = repacked();

        assert!(options.use_object_streams);
        assert!(options.use_xref_streams);
        assert!(!options.linearize);
        assert!(
            options.object_stream_config.compression_level > 0,
            "object streams packed at level 0 are stored, not compressed"
        );
    }

    /// The premise the refusal rests on, pinned so it cannot rot silently: if
    /// a future `lopdf` starts populating an encrypted document's objects
    /// without a password, this test goes red and the refusal above is worth
    /// revisiting. Until then, repacking that handle writes out an empty file.
    #[test]
    fn an_encrypted_document_really_does_load_as_an_empty_one() {
        let document =
            Document::load_mem(&encrypted_document()).expect("it loads — that is the trap");

        assert!(document.is_encrypted());
        assert_eq!(document.get_pages().len(), 0);
    }
}
