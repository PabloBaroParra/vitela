//! What a compression did, and what it declined to do.
//!
//! ## The rule this module exists to keep
//!
//! *The report describes the bytes the caller actually got.*
//!
//! A compression that was measured, attempted and then thrown away for being
//! no smaller is not a compression, and a report claiming "142 images
//! resampled" for a file where nothing was applied is a lie told in good
//! faith. [`crate::guarantee`] is what enforces that: it builds the report
//! from the branch it took, not from the work the pipeline attempted. This
//! module makes the lie hard to write in the first place — [`Outcome`] is
//! *derived* from the two byte counts rather than stored beside them, so it
//! cannot disagree with them.
//!
//! Same report channel as `pdf_manip::GraftReport` in the assembly batch: an
//! operation that can silently do less than asked says so in a value the
//! caller cannot take without seeing.

/// Whether the caller's file got smaller.
///
/// Never stored — always computed from `before` and `after`, so there is no
/// state to get out of step with the numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The compressed file is strictly smaller, and is what the caller got.
    Reduced,
    /// Nothing smaller was produced, so the caller got their original bytes
    /// back — byte for byte, not an equivalent re-serialisation of them.
    NoGain,
}

/// Something compression could not do to this document, and why.
///
/// Reported rather than thrown, because a refusal is usually the explanation
/// for a disappointing result: a user who is told "this file is signed, so
/// only its structure was repacked" has learned something, where a user shown
/// a 2% saving and nothing else has not. The save-side gates (T-195) are what
/// record these; T-190 only guarantees they survive the journey.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// The document is encrypted and its security handler does not permit the
    /// full rewrite that compression needs. Decided by `pdf-save`'s existing
    /// `full_rewrite_blocker`, not by a second policy invented here
    /// (`docs/batch-compress.md` decision 7).
    EncryptedDocumentNotRewritable,
    /// The document carries signatures, compressing rewrites every byte
    /// offset they cover, and the caller has not said it knows. The file is
    /// left alone until it does.
    SignaturesWouldBeInvalidated,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::EncryptedDocumentNotRewritable => write!(
                f,
                "this document's password protection does not allow it to be rewritten, so it cannot be compressed"
            ),
            Refusal::SignaturesWouldBeInvalidated => write!(
                f,
                "compressing rewrites the file and would stop its signature from verifying"
            ),
        }
    }
}

/// How much was done, counted in things rather than bytes.
///
/// Zeroed wholesale when the candidate is thrown away — see this module's
/// header. Grouped into one value instead of four fields on the report so
/// that zeroing it is a single, obvious assignment rather than four chances
/// to forget one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Work {
    /// Images resampled to the preset's effective DPI, re-encoded, and found
    /// to be smaller that way (T-194).
    pub images_resampled: usize,
    /// Images left byte-identical: already below the target DPI, carrying
    /// alpha that a re-encode would drop, or simply smaller as they were.
    pub images_skipped: usize,
    /// Streams that arrived unfiltered or poorly filtered and were re-encoded
    /// with flate (T-191).
    pub streams_recompressed: usize,
    /// Objects dropped as unreachable from the catalog, or as byte-identical
    /// duplicates of an object that stayed (T-192).
    pub objects_dropped: usize,
}

impl Work {
    /// This tally plus another stage's.
    ///
    /// Stages report only what they themselves did, and
    /// [`crate::session`] adds them up. Written as one field-wise sum rather
    /// than as four `+=` at the call site, so a field added by a later task
    /// is added to the total in one place instead of wherever someone
    /// remembers.
    pub(crate) fn plus(self, other: Work) -> Work {
        Work {
            images_resampled: self.images_resampled + other.images_resampled,
            images_skipped: self.images_skipped + other.images_skipped,
            streams_recompressed: self.streams_recompressed + other.streams_recompressed,
            objects_dropped: self.objects_dropped + other.objects_dropped,
        }
    }
}

/// What a compression did to one document.
///
/// Only [`crate::guarantee`] can build one, and only from the branch it
/// actually took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressReport {
    before: u64,
    after: u64,
    work: Work,
    refusals: Vec<Refusal>,
}

impl CompressReport {
    pub(crate) fn new(before: u64, after: u64, work: Work, refusals: Vec<Refusal>) -> Self {
        CompressReport {
            before,
            after,
            work,
            refusals,
        }
    }

    /// Size of the document handed in.
    pub fn before(&self) -> u64 {
        self.before
    }

    /// Size of the document handed back — always the length of the bytes the
    /// caller received, including when those are the originals.
    pub fn after(&self) -> u64 {
        self.after
    }

    /// What was applied to produce those bytes.
    pub fn work(&self) -> Work {
        self.work
    }

    /// What could not be done, in the order it was discovered.
    pub fn refusals(&self) -> &[Refusal] {
        &self.refusals
    }

    /// Whether the file got smaller, derived from the two counts above.
    pub fn outcome(&self) -> Outcome {
        if self.after < self.before {
            Outcome::Reduced
        } else {
            Outcome::NoGain
        }
    }

    /// Bytes saved, or zero. Saturating rather than signed: by the time a
    /// report exists the guarantee has already ruled out a larger result, so
    /// a negative saving is unrepresentable rather than merely unexpected.
    pub fn saved_bytes(&self) -> u64 {
        self.before.saturating_sub(self.after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(before: u64, after: u64) -> CompressReport {
        CompressReport::new(before, after, Work::default(), Vec::new())
    }

    #[test]
    fn a_smaller_result_is_reduced() {
        let report = report(1_000, 400);
        assert_eq!(report.outcome(), Outcome::Reduced);
        assert_eq!(report.saved_bytes(), 600);
    }

    #[test]
    fn a_result_of_the_same_size_is_no_gain() {
        let report = report(1_000, 1_000);
        assert_eq!(report.outcome(), Outcome::NoGain);
        assert_eq!(report.saved_bytes(), 0);
    }

    /// The guarantee makes this state unreachable; the report still refuses
    /// to describe it as a saving if it ever is reached.
    #[test]
    fn a_larger_result_is_never_reported_as_a_saving() {
        let report = report(1_000, 4_000);
        assert_eq!(report.outcome(), Outcome::NoGain);
        assert_eq!(report.saved_bytes(), 0);
    }

    #[test]
    fn a_refusal_explains_itself_in_words() {
        assert!(Refusal::SignaturesWouldBeInvalidated
            .to_string()
            .contains("signature"));
        assert!(Refusal::EncryptedDocumentNotRewritable
            .to_string()
            .contains("password"));
    }

    /// Every field of the tally is summed, so a stage's count cannot be lost
    /// by being the one nobody remembered to add.
    #[test]
    fn adding_two_tallies_adds_every_field() {
        let sum = Work {
            images_resampled: 1,
            images_skipped: 2,
            streams_recompressed: 3,
            objects_dropped: 4,
        }
        .plus(Work {
            images_resampled: 10,
            images_skipped: 20,
            streams_recompressed: 30,
            objects_dropped: 40,
        });

        assert_eq!(
            sum,
            Work {
                images_resampled: 11,
                images_skipped: 22,
                streams_recompressed: 33,
                objects_dropped: 44,
            }
        );
    }

    #[test]
    fn work_starts_at_nothing_done() {
        assert_eq!(
            Work::default(),
            Work {
                images_resampled: 0,
                images_skipped: 0,
                streams_recompressed: 0,
                objects_dropped: 0,
            }
        );
    }
}
