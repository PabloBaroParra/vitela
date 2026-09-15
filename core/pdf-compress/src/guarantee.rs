//! The one rule this crate is judged by.
//!
//! ## The rule this module exists to keep
//!
//! *No compression ever hands back a file bigger than the one it was given.*
//!
//! This is not a check a later stage is supposed to remember to run. It is
//! the only way out of the crate: [`compress_with`] is the sole constructor
//! of [`Compressed`], and [`Compressed`] is the only thing that carries bytes
//! across the public boundary. A stage that produces a worse file cannot ship
//! it by forgetting something — it has nowhere to put it.
//!
//! The rule is needed twice, at two scales: per image, where a JPEG
//! re-encoded at quality 75 can come out larger than the one it replaced, and
//! per file, where a whole repack can lose to the document it started from.
//! Both ask the same question, so both ask it in the same place —
//! [`choose`] — rather than in two comparisons that can drift apart. T-194
//! calls it per image; [`compress_with`] calls it once for the file.
//!
//! ## Why the counters are zeroed on rejection
//!
//! A rejected candidate is not a smaller compression, it is *no* compression.
//! The file the caller holds has had nothing done to it, so the report says
//! nothing was done — see [`crate::report`]. Refusals are the exception and
//! survive: they are facts about the input, and usually the explanation for
//! why there was no gain at all.

use crate::error::CompressError;
use crate::preset::CompressPreset;
use crate::report::{CompressReport, Refusal, Work};

/// A compression the pipeline is *offering*. Not yet the caller's bytes.
///
/// Crate-private on purpose: a `Candidate` that could be built from outside
/// would be a `Compressed` that never passed [`choose`].
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    bytes: Vec<u8>,
    work: Work,
    refusals: Vec<Refusal>,
}

impl Candidate {
    /// A candidate that claims no work — the shape [`crate::pipeline`]
    /// returns until it learns to compress anything.
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Candidate {
            bytes,
            work: Work::default(),
            refusals: Vec::new(),
        }
    }

    /// What this candidate is offering, for a later stage to build on.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Records what producing these bytes took. Only honoured if the
    /// candidate wins.
    //
    // Unused by the no-op pipeline, and exercised only by this module's tests
    // until the stages that count something arrive: T-191 (streams), T-192
    // (objects), T-194 (images).
    #[allow(dead_code)]
    pub(crate) fn with_work(mut self, work: Work) -> Self {
        self.work = work;
        self
    }

    /// Records something that could not be done. Survives rejection.
    //
    // Unused until the save-side gates land in T-195 (see `Refusal`).
    #[allow(dead_code)]
    pub(crate) fn refusing(mut self, refusal: Refusal) -> Self {
        self.refusals.push(refusal);
        self
    }
}

/// A finished compression: the bytes the caller gets, and what produced them.
///
/// The report travels *with* the bytes rather than beside them, so a caller
/// cannot take one without the other — same reason `pdf_manip::GraftOutcome`
/// is shaped this way.
#[derive(Debug, Clone)]
pub struct Compressed {
    bytes: Vec<u8>,
    report: CompressReport,
}

impl Compressed {
    /// The document to write out.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The document to write out, taking ownership.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// What was done to produce [`Compressed::bytes`], and what was not.
    pub fn report(&self) -> &CompressReport {
        &self.report
    }
}

/// Which of two encodings of the same thing to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    /// The new bytes are strictly smaller. Take them.
    Candidate,
    /// The new bytes are no smaller. Keep what we had — unchanged, not
    /// re-serialised.
    Original,
}

/// The rule, in one place, for both scales (see this module's header).
///
/// Strictly smaller wins. Equal size loses on purpose: rewriting a file to
/// the same size spends the user's time, invalidates any signature over it
/// and changes bytes that had no reason to change, all to save nothing.
pub(crate) fn choose(original: &[u8], candidate: &[u8]) -> Choice {
    if candidate.len() < original.len() {
        Choice::Candidate
    } else {
        Choice::Original
    }
}

/// Runs `pipeline` over `input` and keeps its result only if it is smaller.
///
/// The entry point every public compression goes through. `pipeline` is a
/// parameter rather than a hard-coded call so that the guarantee can be
/// tested against pipelines that misbehave — one that inflates, one that
/// changes nothing, one that claims work it did not keep — none of which a
/// real optimiser would volunteer to be.
pub(crate) fn compress_with<F>(
    input: &[u8],
    preset: CompressPreset,
    pipeline: F,
) -> Result<Compressed, CompressError>
where
    F: FnOnce(&[u8], CompressPreset) -> Result<Candidate, CompressError>,
{
    if input.is_empty() {
        return Err(CompressError::EmptyInput);
    }

    let candidate = pipeline(input, preset)?;
    let before = input.len() as u64;

    match choose(input, candidate.bytes()) {
        Choice::Candidate => {
            let after = candidate.bytes.len() as u64;
            Ok(Compressed {
                report: CompressReport::new(before, after, candidate.work, candidate.refusals),
                bytes: candidate.bytes,
            })
        }
        // The original, not a re-serialisation of it: a file nobody improved
        // is a file nobody should have touched. `after` is `before` by
        // construction, which is what makes the report read `NoGain` without
        // anyone having to remember to set it.
        Choice::Original => Ok(Compressed {
            bytes: input.to_vec(),
            report: CompressReport::new(before, before, Work::default(), candidate.refusals),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Outcome;

    const ORIGINAL: &[u8] = b"%PDF-1.7\n% a document of a perfectly ordinary size\n%%EOF\n";

    fn work_claiming_everything() -> Work {
        Work {
            images_resampled: 142,
            images_skipped: 7,
            streams_recompressed: 31,
            objects_dropped: 9,
        }
    }

    #[test]
    fn a_smaller_candidate_is_the_one_handed_back() {
        let smaller = b"%PDF-1.7\n%%EOF\n".to_vec();
        let expected = smaller.clone();

        let result = compress_with(ORIGINAL, CompressPreset::Balanced, |_, _| {
            Ok(Candidate::new(smaller.clone()).with_work(work_claiming_everything()))
        })
        .expect("a smaller candidate is not an error");

        assert_eq!(result.bytes(), expected.as_slice());
        assert_eq!(result.report().outcome(), Outcome::Reduced);
        assert_eq!(result.report().work(), work_claiming_everything());
    }

    /// The whole point of the module. A pipeline that inflates is not an
    /// error and is not a warning — it simply does not get to win.
    #[test]
    fn a_bigger_candidate_is_thrown_away() {
        let bloated = vec![b'x'; ORIGINAL.len() * 4];

        let result = compress_with(ORIGINAL, CompressPreset::Small, |_, _| {
            Ok(Candidate::new(bloated.clone()))
        })
        .expect("an inflating pipeline is not a failure");

        assert_eq!(result.bytes(), ORIGINAL);
        assert_eq!(result.report().outcome(), Outcome::NoGain);
    }

    /// Equal size loses. Rewriting to the same size costs the user their
    /// signature and buys them nothing.
    #[test]
    fn a_candidate_of_exactly_the_same_size_is_thrown_away() {
        let same_size = vec![b'y'; ORIGINAL.len()];

        let result = compress_with(ORIGINAL, CompressPreset::Lossless, |_, _| {
            Ok(Candidate::new(same_size.clone()))
        })
        .expect("an equal-size candidate is not a failure");

        assert_eq!(
            result.bytes(),
            ORIGINAL,
            "the original bytes must come back untouched, not a same-size re-serialisation"
        );
        assert_eq!(result.report().outcome(), Outcome::NoGain);
    }

    /// The subtle one. A thrown-away candidate did nothing to the file the
    /// caller is holding, so it may not claim it did.
    #[test]
    fn a_thrown_away_candidate_claims_no_work() {
        let bloated = vec![b'x'; ORIGINAL.len() * 4];

        let result = compress_with(ORIGINAL, CompressPreset::Balanced, |_, _| {
            Ok(Candidate::new(bloated.clone()).with_work(work_claiming_everything()))
        })
        .expect("an inflating pipeline is not a failure");

        assert_eq!(
            result.report().work(),
            Work::default(),
            "nothing was applied to these bytes, so nothing may be reported as applied"
        );
    }

    /// Refusals are facts about the *input*, so they outlive the candidate
    /// that carried them — and they are usually why there was no gain.
    #[test]
    fn a_thrown_away_candidate_keeps_its_refusals() {
        let bloated = vec![b'x'; ORIGINAL.len() * 4];

        let result = compress_with(ORIGINAL, CompressPreset::Balanced, |_, _| {
            Ok(Candidate::new(bloated.clone()).refusing(Refusal::SignaturesWouldBeInvalidated))
        })
        .expect("an inflating pipeline is not a failure");

        assert_eq!(
            result.report().refusals(),
            &[Refusal::SignaturesWouldBeInvalidated]
        );
    }

    /// The invariant that stops the report from describing some other file.
    #[test]
    fn the_report_always_measures_the_bytes_that_came_back() {
        let cases: Vec<Vec<u8>> = vec![
            b"%PDF-1.7\n%%EOF\n".to_vec(),
            vec![b'x'; ORIGINAL.len() * 4],
            vec![b'y'; ORIGINAL.len()],
        ];

        for candidate in cases {
            let result = compress_with(ORIGINAL, CompressPreset::Balanced, |_, _| {
                Ok(Candidate::new(candidate.clone()))
            })
            .expect("no case here is an error");

            assert_eq!(
                result.report().after(),
                result.bytes().len() as u64,
                "report.after() must describe the bytes actually returned"
            );
            assert_eq!(result.report().before(), ORIGINAL.len() as u64);
        }
    }

    #[test]
    fn a_failing_pipeline_fails_the_call() {
        let result = compress_with(ORIGINAL, CompressPreset::Balanced, |_, _| {
            Err(CompressError::EmptyInput)
        });

        assert!(matches!(result, Err(CompressError::EmptyInput)));
    }

    #[test]
    fn the_pipeline_is_handed_the_preset_it_was_called_with() {
        let result = compress_with(ORIGINAL, CompressPreset::Small, |bytes, preset| {
            assert_eq!(preset, CompressPreset::Small);
            assert_eq!(bytes, ORIGINAL);
            Ok(Candidate::new(b"tiny".to_vec()))
        })
        .expect("not an error");

        assert_eq!(result.bytes(), b"tiny");
    }

    #[test]
    fn choose_keeps_only_what_is_strictly_smaller() {
        assert_eq!(choose(b"abcdef", b"abc"), Choice::Candidate);
        assert_eq!(choose(b"abcdef", b"abcdef"), Choice::Original);
        assert_eq!(choose(b"abc", b"abcdef"), Choice::Original);
    }
}
