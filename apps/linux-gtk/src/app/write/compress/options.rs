//! What the Compress dialog offers and what the status line says afterwards,
//! as rules rather than widgets.
//!
//! The cut `export::options` and `split::options` make, for the same reason:
//! every sentence a user reads is decided by a plain function of its
//! arguments, so a wording rule has a test that reaches it directly instead of
//! three widgets driven to provoke it.
//!
//! ## Why nothing here counts what was done
//!
//! [`pdf_compress::Work`] carries four tallies — images resampled, streams
//! recompressed, objects dropped — and none of them appears in any sentence
//! below. T-196 measured why: the gain on the 8-page generator document is
//! **entirely** write format (object streams and a cross-reference stream),
//! which `Work` does not count, so a real 38% reduction arrives with a tally
//! of zero. A dialog that reported the tally would tell the user nothing was
//! done to a file that had just lost a third of its size. What is read is the
//! outcome and the bytes, which are measured rather than attributed.
//!
//! ## Why `before` is spelled "uncompressed" everywhere
//!
//! [`pdf_compress::CompressReport::before`] is the size of the bytes **this
//! save produced**, not the size of the file the user opened — the report's
//! own documentation says so. With an unsaved edit in the document those two
//! are not the same number, and calling it "the original" would be a claim
//! about a file on disk that nothing here measured. Every sentence therefore
//! names it as the uncompressed save it is.

use std::path::Path;

use pdf_compress::{CompressPreset, CompressReport, Outcome, Refusal};

/// A preset as the dialog lists it: what it is called and what it costs.
///
/// A total `match` with no wildcard, deliberately, mirroring `pdf-ffi`'s two
/// preset conversions: [`CompressPreset`] is not `#[non_exhaustive]`, so a
/// fourth preset is a decision to re-open here — with words a user can choose
/// between — rather than a variant that quietly lands in the list unlabelled.
///
/// The numbers in the descriptions (150 DPI, 96 DPI) are the ones
/// `pdf_compress::preset` actually applies. They are repeated rather than read
/// from [`CompressPreset::image_policy`] because a person choosing between
/// three rows is reading prose, not a table of effective-DPI ceilings — but
/// [`descriptions_match_the_presets_they_describe`] pins them against the core
/// so the prose cannot drift away from what the preset does.
pub(super) fn preset_words(preset: CompressPreset) -> (&'static str, &'static str) {
    match preset {
        CompressPreset::Lossless => (
            "Lossless",
            "Repack the file only. Every page renders exactly as it does now.",
        ),
        CompressPreset::Balanced => (
            "Balanced",
            "Also bring images down to 150 DPI. Visibly unchanged on screen, \
             materially smaller on disk.",
        ),
        CompressPreset::Small => (
            "Small",
            "Also bring images down to 96 DPI. For when the file has to fit \
             through something and the pixels matter less than that.",
        ),
    }
}

/// The preset the dialog starts on.
///
/// `Balanced`, matching the core's own description of it as "the default
/// offer". It is also the only one of the three whose name does not
/// pre-commit the user to a trade: `Lossless` sounds like the safe answer and
/// `Small` like the useful one, and neither is the right default for a person
/// who has not yet seen what either produces.
pub(super) const DEFAULT_PRESET: CompressPreset = CompressPreset::Balanced;

/// Shown while the compressed bytes are going to disk.
///
/// Not [`super::COMPRESS`]'s `busy`, which says "Compressing PDF…": by this
/// point the compressing is over and the user has already been told what it
/// achieved. A second stretch of the same sentence would read as the work
/// starting again.
pub(super) const WRITING: &str = "Writing the compressed PDF...";

/// A byte count as a person reads it.
///
/// Powers of ten, not of two: this number sits next to an upload limit or a
/// mail attachment cap, and every one of those is quoted in MB meaning a
/// million bytes.
pub(super) fn human_size(bytes: u64) -> String {
    const KILO: f64 = 1_000.0;
    const MEGA: f64 = 1_000_000.0;
    let size = bytes as f64;
    if size >= MEGA {
        format!("{:.1} MB", size / MEGA)
    } else if size >= KILO {
        format!("{:.0} kB", size / KILO)
    } else if bytes == 1 {
        "1 byte".to_owned()
    } else {
        format!("{bytes} bytes")
    }
}

/// How much smaller the compressed bytes are, as a whole percent.
///
/// Truncating rather than rounding, so the number shown is one the file
/// actually reached: a 49.7% reduction is reported as 49%, never as 50%.
fn percent_smaller(report: &CompressReport) -> u64 {
    if report.before() == 0 {
        return 0;
    }
    report.saved_bytes().saturating_mul(100) / report.before()
}

/// Whatever the compression had to refuse, as a clause to append — or the
/// empty string when it refused nothing.
///
/// Both refusals are gated ahead of the run (`super::begin_compress` turns an
/// encrypted document away, and the signature prompt settles the other), so in
/// practice this is empty. It is still written out rather than asserted away:
/// [`Refusal`] is `#[non_exhaustive]`, and a variant added upstream must reach
/// the user as a sentence rather than vanish because this shell only knew how
/// to ask about two.
fn refusal_clause(report: &CompressReport) -> String {
    if report.refusals().is_empty() {
        return String::new();
    }
    let reasons: Vec<String> = report
        .refusals()
        .iter()
        .map(|refusal| refusal.to_string())
        .collect();
    format!(" Not everything could be done: {}.", reasons.join("; "))
}

/// What the status line says once a compression has produced smaller bytes
/// and the destination is the only thing left to ask for.
pub(super) fn reduction_summary(report: &CompressReport) -> String {
    format!(
        "{} uncompressed → {} compressed ({}% smaller). Choose where to write it.{}",
        human_size(report.before()),
        human_size(report.after()),
        percent_smaller(report),
        refusal_clause(report)
    )
}

/// What it says when nothing smaller came out.
///
/// No destination is asked for in this case, and that is the point: the bytes
/// on offer are the ones an ordinary Save would have written, so a chooser
/// here would be inviting the user to file a second copy of their document
/// under a name that promises it is smaller. Saying so and stopping is the
/// honest end of the gesture.
pub(super) fn no_gain_summary(report: &CompressReport) -> String {
    format!(
        "Nothing to gain: this document compresses to the same {} an uncompressed save writes, \
         so no file was written.{}",
        human_size(report.before()),
        refusal_clause(report)
    )
}

/// What it says once the compressed bytes are on disk.
pub(super) fn written_summary(report: &CompressReport, destination: &Path) -> String {
    format!(
        "Compressed PDF written to {} ({} → {}, {}% smaller).",
        destination.display(),
        human_size(report.before()),
        human_size(report.after()),
        percent_smaller(report)
    )
}

/// The shell's words for a refusal met before anything ran.
///
/// The core's own sentence, capitalised and stopped — not a second, vaguer
/// copy of it, the same call `export::options` makes for a bad page range.
/// Nothing is appended about how to get past it, because in this shell there
/// is no way past it: `SaveIntent::StripProtection` is the yes-path
/// `pdf-save` documents, and no gesture in this shell asks for one.
pub(super) fn refusal_text(refusal: &Refusal) -> String {
    let sentence = refusal.to_string();
    let mut characters = sentence.chars();
    match characters.next() {
        Some(first) => format!("{}{}.", first.to_uppercase(), characters.as_str()),
        None => sentence,
    }
}

/// Whether these bytes are worth offering a destination for.
pub(super) fn is_reduced(report: &CompressReport) -> bool {
    report.outcome() == Outcome::Reduced
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building a report from outside the crate is only possible in the
    /// refused shape, so the sentences are exercised through that one — which
    /// is also the shape that carries a refusal clause.
    fn refused_report(size: u64) -> CompressReport {
        CompressReport::refused(size, Refusal::SignaturesWouldBeInvalidated)
    }

    #[test]
    fn sizes_are_written_in_powers_of_ten() {
        assert_eq!(human_size(2_400_000), "2.4 MB");
        assert_eq!(human_size(840_000), "840 kB");
        assert_eq!(human_size(999), "999 bytes");
        assert_eq!(human_size(1), "1 byte");
        assert_eq!(human_size(0), "0 bytes");
    }

    /// The boundary each branch turns on, so a file of exactly a megabyte is
    /// not reported as "1000 kB".
    #[test]
    fn each_unit_starts_at_its_own_boundary() {
        assert_eq!(human_size(1_000), "1 kB");
        assert_eq!(human_size(1_000_000), "1.0 MB");
    }

    /// A report the caller cannot have gained from is never described as a
    /// reduction, whatever else it says.
    #[test]
    fn a_refused_report_is_not_a_reduction() {
        let report = refused_report(2_400_000);

        assert!(!is_reduced(&report));
        assert_eq!(percent_smaller(&report), 0);
    }

    /// A refusal is appended rather than swallowed — the property that
    /// matters when `Refusal` grows a variant this shell never asked about.
    #[test]
    fn a_refusal_reaches_the_user_as_a_sentence() {
        let summary = no_gain_summary(&refused_report(2_400_000));

        assert!(summary.starts_with("Nothing to gain: this document compresses to the same 2.4 MB"));
        assert!(
            summary.ends_with(&format!(
                "Not everything could be done: {}.",
                Refusal::SignaturesWouldBeInvalidated
            )),
            "{summary}"
        );
    }

    /// An empty document cannot be divided by, and reporting it as 0% smaller
    /// is the only answer that is not a lie.
    #[test]
    fn a_zero_byte_report_does_not_divide_by_zero() {
        assert_eq!(
            percent_smaller(&CompressReport::refused(
                0,
                Refusal::SignaturesWouldBeInvalidated
            )),
            0
        );
    }

    #[test]
    fn a_written_summary_names_the_destination_and_both_sizes() {
        let report = refused_report(2_400_000);

        assert_eq!(
            written_summary(&report, Path::new("/tmp/small.pdf")),
            "Compressed PDF written to /tmp/small.pdf (2.4 MB → 2.4 MB, 0% smaller)."
        );
    }

    /// The core's sentence, not a paraphrase of it.
    #[test]
    fn a_blocker_is_shown_in_the_cores_own_words() {
        assert_eq!(
            refusal_text(&Refusal::EncryptedDocumentNotRewritable),
            "This document's password protection does not allow it to be rewritten, \
             so it cannot be compressed."
        );
    }

    /// Every preset the core offers has words of its own, and no two share
    /// them.
    #[test]
    fn every_preset_is_named_and_described_distinctly() {
        let mut names = Vec::new();
        for preset in CompressPreset::all() {
            let (name, description) = preset_words(preset);
            assert!(!name.is_empty() && !description.is_empty(), "{preset:?}");
            names.push(name);
        }
        names.sort_unstable();
        let total = names.len();
        names.dedup();
        assert_eq!(names.len(), total, "two presets share a name");
    }

    /// The prose repeats numbers that live in `pdf_compress::preset`, so it
    /// is pinned against them: a preset that stops touching images, or that
    /// moves its ceiling, fails here rather than shipping a description of
    /// what it used to do.
    #[test]
    fn descriptions_match_the_presets_they_describe() {
        assert!(
            CompressPreset::Lossless.image_policy().is_none(),
            "Lossless is described as leaving every page as it renders now"
        );
        for preset in [CompressPreset::Balanced, CompressPreset::Small] {
            let policy = preset
                .image_policy()
                .unwrap_or_else(|| panic!("{preset:?} is described as resampling images"));
            let (_, description) = preset_words(preset);
            assert!(
                description.contains(&format!("{} DPI", policy.target_dpi)),
                "{preset:?} resamples to {} DPI, but is described as {description:?}",
                policy.target_dpi
            );
        }
    }

    /// `Balanced` is what the dialog opens on, and the core calls it the
    /// default offer — the two must not drift apart.
    #[test]
    fn the_dialog_opens_on_the_preset_the_core_calls_the_default_offer() {
        assert_eq!(DEFAULT_PRESET, CompressPreset::Balanced);
        assert!(CompressPreset::all().contains(&DEFAULT_PRESET));
    }
}
