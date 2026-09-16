//! `pdf-compress`: make a PDF smaller without making it worse.
//!
//! Batch 24 (`docs/batch-compress.md`), T-190..T-197. This crate owns the
//! *policy* of compression — how hard to try, what may be touched, and the
//! one promise that makes the feature usable at all. `pdf-save` owns when it
//! runs and what it is allowed to run on; a shell owns the preset the user
//! picked and where the result is written.
//!
//! ## The promise
//!
//! **Compressing a file never makes it bigger.** A compression that cannot
//! beat the document it was given returns that document, byte for byte, and
//! reports [`Outcome::NoGain`]. This is structural rather than aspirational:
//! [`guarantee::compress_with`] is the only way bytes leave this crate. See
//! [`guarantee`] for why it is enforced there and not by a check at the end.
//!
//! ## Module map
//!
//! - [`preset`] — the closed set of three, and the only copy of the numbers
//!   that decide image quality.
//! - [`consent`] — the one thing the caller, not this crate, decides: what
//!   happens to a document that already carries a signature.
//! - [`guarantee`] — the never-grow rule, and [`Compressed`], the only way
//!   out.
//! - [`report`] — what was done and what was refused, measured against the
//!   bytes the caller actually got.
//! - `pipeline` — the order the stages run in, and nothing else.
//! - `session` — one parse and one serialise per compression; its `gate` is
//!   the two documents that get neither, encrypted and signed.
//! - `images` — T-193's inventory: every image the document actually draws,
//!   measured against the size the page draws it at.
//! - `structural` — T-191's repack: flate over streams that arrived
//!   unfiltered. (Object streams and the cross-reference stream are the write
//!   format, and live in `session`.)
//! - `prune` — T-192's sweep: duplicate objects merged, unreachable ones
//!   deleted.
//! - [`error`] — [`CompressError`].
//!
//! ## What this crate does not do
//!
//! It does not undo. Compression is a whole-file transform at save time, not
//! an edit to the page model, so it creates no `Command` and touches no
//! `EditLog` — the inverse of "re-encoded four hundred images" is a second
//! copy of the file. Undoing a compression is declining to save it.
//! `docs/batch-compress.md` decision 2, and the same treatment the protection
//! strip already gets.

pub mod consent;
pub mod error;
pub mod guarantee;
mod images;
mod pipeline;
pub mod preset;
mod prune;
pub mod report;
mod session;
mod structural;
#[cfg(test)]
mod test_fixtures;

pub use consent::SignedDocuments;
pub use error::CompressError;
pub use guarantee::Compressed;
pub use preset::{CompressPreset, ImagePolicy};
pub use report::{CompressReport, Outcome, Refusal, Work};

/// Compresses `input` as far as `preset` allows, or hands it straight back.
///
/// The only entry point. Never returns bytes larger than `input`; see the
/// crate header.
///
/// `signed` is the caller's position on a document that already carries a
/// signature, and it is an argument rather than a default so that every call
/// site has to take one — see [`consent`]. A caller with nothing to say about
/// signatures writes [`SignedDocuments::LeaveAlone`], which is also
/// `SignedDocuments::default()`.
///
/// # Errors
///
/// [`CompressError::EmptyInput`] when there is no document to work on,
/// [`CompressError::Lopdf`] when `input` cannot be read as a PDF, and
/// [`CompressError::Io`] when the compressed document cannot be serialised.
pub fn compress(
    input: &[u8],
    preset: CompressPreset,
    signed: SignedDocuments,
) -> Result<Compressed, CompressError> {
    guarantee::compress_with(input, preset, |bytes, preset| {
        pipeline::run(bytes, preset, signed)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::loose_document;

    /// T-190's acceptance criterion, still stated the way the checklist
    /// states it. It passed against a pipeline that did nothing; it passes
    /// now that one stage does something; it is here to keep passing as the
    /// rest land, which is the entire reason it was written before them.
    #[test]
    fn no_preset_ever_grows_a_document() {
        let document = loose_document(8);

        for preset in CompressPreset::all() {
            let result = compress(&document, preset, SignedDocuments::LeaveAlone)
                .expect("a real document is not an error");

            assert!(
                result.bytes().len() <= document.len(),
                "{preset:?} returned {} bytes for a {}-byte document",
                result.bytes().len(),
                document.len()
            );
            assert_eq!(result.report().after(), result.bytes().len() as u64);
        }
    }

    /// The structural pass is real now, so the end-to-end call has to show a
    /// real saving — not just decline to make things worse.
    #[test]
    fn a_loosely_written_document_actually_comes_back_smaller() {
        let document = loose_document(8);

        let result = compress(
            &document,
            CompressPreset::Lossless,
            SignedDocuments::LeaveAlone,
        )
        .expect("not an error");

        assert_eq!(result.report().outcome(), Outcome::Reduced);
        assert!(result.report().saved_bytes() > 0);
        assert_eq!(result.report().work().streams_recompressed, 8);
        assert_eq!(
            lopdf::Document::load_mem(result.bytes())
                .expect("the result is a document")
                .get_pages()
                .len(),
            8
        );
    }

    /// The other half of the promise, and the one a user meets more often:
    /// compressing an already-compressed file is not a small win, it is no
    /// win, and it must cost them nothing — not even a re-serialisation.
    #[test]
    fn compressing_an_already_packed_document_changes_nothing_at_all() {
        let once = compress(
            &loose_document(8),
            CompressPreset::Lossless,
            SignedDocuments::LeaveAlone,
        )
        .expect("not an error")
        .into_bytes();

        let twice = compress(&once, CompressPreset::Lossless, SignedDocuments::LeaveAlone)
            .expect("not an error");

        assert_eq!(twice.report().outcome(), Outcome::NoGain);
        assert_eq!(twice.bytes(), once.as_slice());
        assert_eq!(twice.report().work(), Work::default());
        assert_eq!(twice.report().saved_bytes(), 0);
    }

    #[test]
    fn there_is_nothing_to_compress_in_nothing() {
        let result = compress(&[], CompressPreset::Lossless, SignedDocuments::LeaveAlone);

        assert!(matches!(result, Err(CompressError::EmptyInput)));
    }

    #[test]
    fn bytes_that_are_not_a_document_are_an_error_not_a_no_gain() {
        let result = compress(
            b"%PDF-1.7\n% looks like one, is not one\n%%EOF\n",
            CompressPreset::Small,
            SignedDocuments::LeaveAlone,
        );

        assert!(matches!(result, Err(CompressError::Lopdf(_))));
    }
}
