//! Where the optimisation will live. Empty on purpose.
//!
//! T-190 ships the guarantee, not the compression — `docs/batch-compress.md`
//! puts it first precisely so the net exists before anyone steps onto the
//! wire. Until the real stages land, [`run`] hands the input straight back,
//! [`crate::guarantee`] measures it as no smaller, and [`crate::compress`]
//! answers `NoGain`. That is the honest result for a crate that has not been
//! taught to compress anything yet, and it is already the correct one.
//!
//! What replaces this body, in order:
//!
//! - **T-191** — the structural pass: object streams and a cross-reference
//!   stream via lopdf's `save_with_options`, plus flate over streams that
//!   arrived unfiltered. Also the task that has to *measure* whether today's
//!   plain `save_to` inflates a document that came in with object streams.
//! - **T-192** — the prune: objects unreachable from the catalog, and
//!   byte-identical duplicate resources.
//! - **T-194** — the image stage, under [`CompressPreset::image_policy`].

use crate::error::CompressError;
use crate::guarantee::Candidate;
use crate::preset::CompressPreset;

/// The compression pipeline, as far as it has been built.
///
/// Returns the input unchanged. The guarantee decides what that means, and
/// its answer — "no smaller, so keep the original" — happens to be right
/// both now and for a file that genuinely cannot be improved.
pub(crate) fn run(input: &[u8], _preset: CompressPreset) -> Result<Candidate, CompressError> {
    Ok(Candidate::new(input.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unbuilt_pipeline_changes_nothing() {
        let input = b"%PDF-1.7\n%%EOF\n";

        for preset in CompressPreset::all() {
            let candidate = run(input, preset).expect("the no-op pipeline cannot fail");
            assert_eq!(
                candidate.bytes(),
                input,
                "{preset:?} must not alter bytes a stage that does not exist cannot have touched"
            );
        }
    }
}
