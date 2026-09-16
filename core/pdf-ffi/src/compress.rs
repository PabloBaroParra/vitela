//! Compression, as a shell sees it (Batch 24, T-196).
//!
//! ## The rule this module exists to keep
//!
//! *A shell picks a preset and reads a report; it never learns how either
//! one is made.*
//!
//! `pdf-compress` owns how hard to squeeze, `pdf-save` owns whether it may
//! run at all, and neither of those is a decision five platform shells should
//! each be making. What crosses here is the vocabulary: the closed set of
//! three presets, the call that runs one, and the numbers a user is shown
//! afterwards.
//!
//! ## Why the report travels with the bytes
//!
//! [`FfiCompressedSave`] is one record rather than two return values for the
//! same reason `pdf_compress::Compressed` is: a caller must not be able to
//! take the bytes without seeing what was done to produce them. A refused
//! compression returns the ordinary save's own output and is *not* an error
//! — the only way a shell can tell the difference, and tell its user why the
//! file is the same size, is the report.
//!
//! ## What a shell has to ask before it offers the button
//!
//! Both gates are answerable ahead of the save, and both questions are here:
//! [`compression_refusal`] for the document that cannot be compressed at all,
//! and [`compressed_save_will_invalidate_signatures`] for the one where the
//! decision is the user's. The second is not the same question as
//! [`crate::will_invalidate_signatures`] — a compressed save is a full
//! rewrite even when nothing was edited, so a signed file answers `true` here
//! and `false` there, and a shell that asks the wrong one warns about the
//! wrong save.

use std::path::Path;

use pdf_compress::{CompressPreset, CompressReport, Outcome, Refusal, Work};

use crate::document::DocumentHandle;
use crate::error::FfiError;
use crate::types::{FfiSaveIntent, FfiSignatureAcknowledgement};

/// How hard a compression tries — the FFI shape of
/// `pdf_compress::CompressPreset`.
///
/// Three, not a slider: the numbers behind each row (target DPI, JPEG
/// quality) live in `pdf-compress` and deliberately do not cross this
/// boundary, so five shells cannot drift into five different ideas of what
/// "smaller" means (`docs/batch-compress.md` decision 3).
///
/// No `Default`, matching the core: a caller has to say how much quality it
/// is spending, because that is the user's decision and not a value this
/// crate may pick for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiCompressPreset {
    /// Structure only — renders pixel for pixel identically to the input.
    Lossless,
    /// Structure, plus images resampled to 150 effective DPI. The default
    /// offer: visibly unchanged on screen, materially smaller on disk.
    Balanced,
    /// Structure, plus 96 DPI — for when the file has to fit through
    /// something and the user has decided that matters more than the pixels.
    Small,
}

// Total in both directions, with no wildcard on purpose: `CompressPreset` is
// deliberately not `#[non_exhaustive]`, so a fourth preset is a decision to
// re-open here rather than a variant to absorb quietly.
impl From<FfiCompressPreset> for CompressPreset {
    fn from(preset: FfiCompressPreset) -> Self {
        match preset {
            FfiCompressPreset::Lossless => CompressPreset::Lossless,
            FfiCompressPreset::Balanced => CompressPreset::Balanced,
            FfiCompressPreset::Small => CompressPreset::Small,
        }
    }
}

impl From<CompressPreset> for FfiCompressPreset {
    fn from(preset: CompressPreset) -> Self {
        match preset {
            CompressPreset::Lossless => FfiCompressPreset::Lossless,
            CompressPreset::Balanced => FfiCompressPreset::Balanced,
            CompressPreset::Small => FfiCompressPreset::Small,
        }
    }
}

/// Whether the file got smaller — the FFI shape of `pdf_compress::Outcome`.
///
/// `NoGain` is a success, not a failure: the caller was handed their own
/// bytes back because nothing better was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiCompressOutcome {
    /// The returned bytes are strictly smaller than the ones compressed.
    Reduced,
    /// Nothing smaller was produced, so the save's own bytes came back.
    NoGain,
}

impl From<Outcome> for FfiCompressOutcome {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Reduced => FfiCompressOutcome::Reduced,
            Outcome::NoGain => FfiCompressOutcome::NoGain,
        }
    }
}

/// Something the compression could not do, and why — the FFI shape of
/// `pdf_compress::Refusal`.
///
/// Reported rather than thrown, because a refusal is usually the explanation
/// for a disappointing result. A shell shows this next to the size it did
/// not manage to change.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum FfiCompressRefusal {
    /// The document's protection would survive this save, and bytes that come
    /// out encrypted cannot be repacked by anyone. The way past it is a
    /// consented [`FfiSaveIntent::StripProtection`], not a second yes/no.
    EncryptedDocumentNotRewritable,
    /// The document carries a signature and the shell has not said the user
    /// was asked. See [`compressed_save_will_invalidate_signatures`].
    SignaturesWouldBeInvalidated,
    /// A refusal this build of the boundary does not model yet, carrying the
    /// core's own wording. `Refusal` is `#[non_exhaustive]`, so a variant
    /// added upstream reaches a shell as a sentence it can show rather than
    /// as a refusal that silently disappears.
    Other { detail: String },
}

impl From<Refusal> for FfiCompressRefusal {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::EncryptedDocumentNotRewritable => {
                FfiCompressRefusal::EncryptedDocumentNotRewritable
            }
            Refusal::SignaturesWouldBeInvalidated => {
                FfiCompressRefusal::SignaturesWouldBeInvalidated
            }
            other => FfiCompressRefusal::Other {
                detail: other.to_string(),
            },
        }
    }
}

/// How much was done, counted in things rather than bytes — the FFI shape of
/// `pdf_compress::Work`.
///
/// `u64` rather than the core's `usize`, which has no width across a foreign
/// function interface; every conversion from a count is therefore widening
/// and cannot lose a digit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, uniffi::Record)]
pub struct FfiCompressWork {
    /// Images resampled, re-encoded, and found to be smaller that way.
    pub images_resampled: u64,
    /// Images left byte-identical — already small enough, or carrying alpha
    /// a re-encode would drop.
    pub images_skipped: u64,
    /// Streams that arrived unfiltered or poorly filtered and were re-encoded.
    pub streams_recompressed: u64,
    /// Objects dropped as unreachable or as duplicates of one that stayed.
    pub objects_dropped: u64,
}

impl From<Work> for FfiCompressWork {
    fn from(work: Work) -> Self {
        FfiCompressWork {
            images_resampled: work.images_resampled as u64,
            images_skipped: work.images_skipped as u64,
            streams_recompressed: work.streams_recompressed as u64,
            objects_dropped: work.objects_dropped as u64,
        }
    }
}

/// What a compression did to one document — the FFI shape of
/// `pdf_compress::CompressReport`.
///
/// [`saved_bytes`](Self::saved_bytes) and [`outcome`](Self::outcome) are
/// derived in the core from the two counts, and are carried rather than left
/// for each shell to re-derive: five platforms subtracting two `u64`s is five
/// chances to subtract them the wrong way round.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FfiCompressReport {
    /// Size of the document compression was handed — the save's own output,
    /// not the file the user opened.
    pub before_bytes: u64,
    /// Size of the bytes the caller received, always.
    pub after_bytes: u64,
    /// `before_bytes - after_bytes`, or zero.
    pub saved_bytes: u64,
    /// Whether those two numbers add up to a smaller file.
    pub outcome: FfiCompressOutcome,
    /// What was applied to produce the bytes.
    pub work: FfiCompressWork,
    /// What could not be done, in the order it was discovered.
    pub refusals: Vec<FfiCompressRefusal>,
}

impl From<CompressReport> for FfiCompressReport {
    fn from(report: CompressReport) -> Self {
        FfiCompressReport {
            before_bytes: report.before(),
            after_bytes: report.after(),
            saved_bytes: report.saved_bytes(),
            outcome: report.outcome().into(),
            work: report.work().into(),
            refusals: report
                .refusals()
                .iter()
                .cloned()
                .map(FfiCompressRefusal::from)
                .collect(),
        }
    }
}

/// A compressed save: the bytes to write, and what compression did to them.
///
/// One record rather than two returns — see this module's header for why the
/// report is not optional.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiCompressedSave {
    /// The document to write out. Never larger than the bytes the save
    /// produced; identical to them when the compression was refused or found
    /// nothing to gain.
    pub bytes: Vec<u8>,
    /// What happened, measured against the bytes the save produced.
    pub report: FfiCompressReport,
}

// `pdf_save::CompressedSave` also carries the graft warnings the save itself
// collected. They are dropped here for the same reason `crate::save_to_bytes`
// drops them: this boundary has no shape for a graft warning yet, and a
// compressed save is not the place to invent one. When assembly grows an FFI
// port, both entry points gain it together.
impl From<pdf_save::CompressedSave> for FfiCompressedSave {
    fn from(saved: pdf_save::CompressedSave) -> Self {
        FfiCompressedSave {
            bytes: saved.bytes,
            report: saved.report.into(),
        }
    }
}

/// Every preset a compress dialog may offer, strongest-preserving first.
///
/// Handed over as a list so a shell populates its dialog from what exists
/// rather than hard-coding three cases of its own — the same reason the
/// quality numbers behind each preset stay in `pdf-compress`.
#[uniffi::export]
pub fn compress_presets() -> Vec<FfiCompressPreset> {
    CompressPreset::all().into_iter().map(Into::into).collect()
}

/// Why this document cannot be compressed, or `None` when it can.
///
/// The question a shell asks *before* it offers the button, so it can
/// disable it with a reason instead of running a compression that was never
/// going to do anything. Cheap: it reads the document's protection, it does
/// not save anything.
#[uniffi::export]
pub fn compression_refusal(
    handle: &DocumentHandle,
    intent: FfiSaveIntent,
) -> Option<FfiCompressRefusal> {
    let state = handle.lock();

    // The acknowledgement says nothing about this question: a signature is
    // the other gate, and it has an answer rather than a fact.
    pdf_save::compression_blocker(
        state.save_input(intent, FfiSignatureAcknowledgement::Unacknowledged),
    )
    .map(Into::into)
}

/// Whether compressing this document breaks a signature it already carries.
///
/// The counterpart to [`crate::will_invalidate_signatures`], and the one a
/// shell must ask before offering a compression: compressing is a full
/// rewrite even when nothing was edited, so a signed file answers `true`
/// here where the ordinary query answers `false`.
///
/// `true` is exactly the condition under which [`save_compressed_to_bytes`]
/// refuses an unacknowledged save.
///
/// # Errors
///
/// Whatever the ordinary query returns when it has to walk the base document
/// to answer.
#[uniffi::export]
pub fn compressed_save_will_invalidate_signatures(
    handle: &DocumentHandle,
    intent: FfiSaveIntent,
) -> Result<bool, FfiError> {
    let state = handle.lock();

    pdf_save::compressed_save_will_invalidate_signatures(
        state.save_input(intent, FfiSignatureAcknowledgement::Unacknowledged),
    )
    .map_err(Into::into)
}

/// Saves `handle` and compresses the result as far as `preset` allows — the
/// canonical cross-platform entry point for "make this smaller", next to
/// [`crate::save_to_bytes`] for an ordinary one.
///
/// Compressing is not an edit: it creates no undo step and leaves the
/// document's pending edits exactly as it found them
/// (`docs/batch-compress.md` decision 2).
///
/// # Errors
///
/// Everything [`crate::save_to_bytes`] can return. In particular
/// [`FfiError::SignaturesWouldBeInvalidated`] when the file is signed and
/// `signatures` has not settled it — ask
/// [`compressed_save_will_invalidate_signatures`] first so the warning
/// arrives before the user commits. A document that simply *cannot* be
/// compressed is not an error: it comes back as the ordinary save's bytes
/// with the reason in the report.
#[uniffi::export]
pub fn save_compressed_to_bytes(
    handle: &DocumentHandle,
    intent: FfiSaveIntent,
    signatures: FfiSignatureAcknowledgement,
    preset: FfiCompressPreset,
) -> Result<FfiCompressedSave, FfiError> {
    let mut state = handle.lock();
    state.record_strip_consent(intent);

    let saved =
        pdf_save::save_document_compressed(state.save_input(intent, signatures), preset.into())?;

    Ok(saved.into())
}

/// Compresses `handle` into `path` (the convenience twin of
/// [`save_compressed_to_bytes`], for a shell that already has a path).
///
/// Returns the report rather than nothing: unlike an ordinary save, a
/// compression has a result the user asked for and is owed.
///
/// # Errors
///
/// Whatever [`save_compressed_to_bytes`] returns, plus [`FfiError::Io`] if
/// the compressed document cannot be written to `path`.
#[uniffi::export]
pub fn save_compressed_to_path(
    handle: &DocumentHandle,
    path: String,
    intent: FfiSaveIntent,
    signatures: FfiSignatureAcknowledgement,
    preset: FfiCompressPreset,
) -> Result<FfiCompressReport, FfiError> {
    let saved = save_compressed_to_bytes(handle, intent, signatures, preset)?;
    std::fs::write(Path::new(&path), &saved.bytes)?;

    Ok(saved.report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both directions, over the whole set: a preset a shell picked has to
    /// arrive at `pdf-compress` as the one the user chose, and the list the
    /// dialog was built from has to be the core's own.
    #[test]
    fn every_preset_survives_the_round_trip_in_both_directions() {
        for preset in CompressPreset::all() {
            let crossed: FfiCompressPreset = preset.into();
            assert_eq!(CompressPreset::from(crossed), preset);
        }

        assert_eq!(
            compress_presets(),
            CompressPreset::all()
                .into_iter()
                .map(FfiCompressPreset::from)
                .collect::<Vec<_>>()
        );
    }

    /// The shape a gate produces, which is the one a shell meets most often:
    /// nothing done, nothing gained, and a reason to show.
    #[test]
    fn a_refused_report_crosses_with_its_reason_and_no_claimed_work() {
        let report: FfiCompressReport =
            CompressReport::refused(9_000, Refusal::EncryptedDocumentNotRewritable).into();

        assert_eq!(report.before_bytes, 9_000);
        assert_eq!(report.after_bytes, 9_000);
        assert_eq!(report.saved_bytes, 0);
        assert_eq!(report.outcome, FfiCompressOutcome::NoGain);
        assert_eq!(report.work, FfiCompressWork::default());
        assert_eq!(
            report.refusals,
            vec![FfiCompressRefusal::EncryptedDocumentNotRewritable]
        );
    }

    /// The other refusal, and the sentence behind it — a shell shows the
    /// variant, so the variant has to be the right one.
    #[test]
    fn the_signature_refusal_keeps_its_own_variant() {
        let report: FfiCompressReport =
            CompressReport::refused(120, Refusal::SignaturesWouldBeInvalidated).into();

        assert_eq!(
            report.refusals,
            vec![FfiCompressRefusal::SignaturesWouldBeInvalidated]
        );
    }

    /// Every field of the tally is carried. Without this, a count added by a
    /// later task reaches the shells as a permanent zero and nobody notices.
    #[test]
    fn the_tally_crosses_field_for_field() {
        let work: FfiCompressWork = Work {
            images_resampled: 1,
            images_skipped: 2,
            streams_recompressed: 3,
            objects_dropped: 4,
        }
        .into();

        assert_eq!(
            work,
            FfiCompressWork {
                images_resampled: 1,
                images_skipped: 2,
                streams_recompressed: 3,
                objects_dropped: 4,
            }
        );
    }

    #[test]
    fn both_outcomes_cross_unchanged() {
        assert_eq!(
            FfiCompressOutcome::from(Outcome::Reduced),
            FfiCompressOutcome::Reduced
        );
        assert_eq!(
            FfiCompressOutcome::from(Outcome::NoGain),
            FfiCompressOutcome::NoGain
        );
    }
}
