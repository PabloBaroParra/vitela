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
//! - [`guarantee`] — the never-grow rule, and [`Compressed`], the only way
//!   out.
//! - [`report`] — what was done and what was refused, measured against the
//!   bytes the caller actually got.
//! - [`pipeline`] — the optimisation itself. Empty at T-190, on purpose.
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

pub mod error;
pub mod guarantee;
mod pipeline;
pub mod preset;
pub mod report;

pub use error::CompressError;
pub use guarantee::Compressed;
pub use preset::{CompressPreset, ImagePolicy};
pub use report::{CompressReport, Outcome, Refusal, Work};

/// Compresses `input` as far as `preset` allows, or hands it straight back.
///
/// The only entry point. Never returns bytes larger than `input`; see the
/// crate header.
///
/// # Errors
///
/// [`CompressError::EmptyInput`] when there is no document to work on.
pub fn compress(input: &[u8], preset: CompressPreset) -> Result<Compressed, CompressError> {
    guarantee::compress_with(input, preset, pipeline::run)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENT: &[u8] = b"%PDF-1.7\n% a document of a perfectly ordinary size\n%%EOF\n";

    /// T-190's acceptance criterion, stated the way the checklist states it:
    /// with an implementation that does nothing, this test already passes.
    /// It keeps passing as each stage lands, which is the entire reason it
    /// was written before them.
    #[test]
    fn no_preset_ever_grows_a_document() {
        for preset in CompressPreset::all() {
            let result = compress(DOCUMENT, preset).expect("a real document is not an error");

            assert!(
                result.bytes().len() <= DOCUMENT.len(),
                "{preset:?} returned {} bytes for a {}-byte document",
                result.bytes().len(),
                DOCUMENT.len()
            );
            assert_eq!(result.report().after(), result.bytes().len() as u64);
        }
    }

    #[test]
    fn a_crate_that_cannot_yet_compress_says_no_gain_rather_than_pretending() {
        let result = compress(DOCUMENT, CompressPreset::Small).expect("not an error");

        assert_eq!(result.report().outcome(), Outcome::NoGain);
        assert_eq!(result.bytes(), DOCUMENT);
        assert_eq!(result.report().work(), Work::default());
        assert_eq!(result.report().saved_bytes(), 0);
    }

    #[test]
    fn there_is_nothing_to_compress_in_nothing() {
        let result = compress(&[], CompressPreset::Lossless);

        assert!(matches!(result, Err(CompressError::EmptyInput)));
    }
}
