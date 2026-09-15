//! The order the stages run in, and nothing else.
//!
//! ## The rule this module exists to keep
//!
//! *Each stage decides what it does; this module decides only when it runs.*
//!
//! Kept thin on purpose. A stage that knows about the stage after it is a
//! stage that cannot be tested on its own, and this is the file every future
//! task edits — so the less it holds, the less each of those tasks can break.
//!
//! What runs, and in what order:
//!
//! - **T-191**, [`structural`] — the repack: object streams, a
//!   cross-reference stream, flate over streams that arrived unfiltered. Runs
//!   for every preset, because it is what
//!   [`CompressPreset::Lossless`](crate::CompressPreset::Lossless) *is* and
//!   the other two are it plus images.
//!
//! Still to land here:
//!
//! - **T-192** — the prune: objects unreachable from the catalog, and
//!   byte-identical duplicate resources.
//! - **T-194** — the image stage, under
//!   [`CompressPreset::image_policy`](crate::CompressPreset::image_policy),
//!   which is where the `preset` argument stops being ignored.

use crate::error::CompressError;
use crate::guarantee::Candidate;
use crate::preset::CompressPreset;
use crate::structural;

/// The compression pipeline, as far as it has been built.
///
/// Returns an *offer*. [`crate::guarantee`] decides whether it ships.
//
// `preset` is unused while the only stage is structural: every preset repacks
// the same way, and the numbers that separate them are image numbers. T-194 is
// where it starts being read.
pub(crate) fn run(input: &[u8], _preset: CompressPreset) -> Result<Candidate, CompressError> {
    structural::pass(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pipeline's own contract, independent of what any stage achieves:
    /// whatever comes out is still a readable document with the same pages in
    /// it. The structural stage checks this for its own candidate; this checks
    /// it for the chain, so a later stage cannot quietly lose a page between
    /// two passes that each kept them.
    #[test]
    fn every_preset_returns_a_document_with_the_same_pages() {
        let input = crate::structural::tests::loose_document(4);

        for preset in CompressPreset::all() {
            let candidate = run(&input, preset).expect("a plain document runs the pipeline");
            let reloaded =
                lopdf::Document::load_mem(candidate.bytes()).expect("the offer must be readable");

            assert_eq!(
                reloaded.get_pages().len(),
                4,
                "{preset:?} lost a page somewhere in the pipeline"
            );
        }
    }

    #[test]
    fn a_document_that_cannot_be_read_stops_the_pipeline() {
        let result = run(b"not a document", CompressPreset::Lossless);

        assert!(matches!(result, Err(CompressError::Lopdf(_))));
    }
}
