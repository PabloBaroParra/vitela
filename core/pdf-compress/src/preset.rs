//! How hard to try, as a closed set of three.
//!
//! ## The rule this module exists to keep
//!
//! *The numbers that decide image quality live here, and only here.*
//!
//! The obvious UI for "compress" is a slider, and a slider puts those numbers
//! in the shell. Five shells would then each pick their own defaults, and
//! every bug report about a blurry scan would start by asking which slider
//! position produced it. Three presets instead, one table, five platforms
//! reading the same row — see `docs/batch-compress.md` decision 3.
//!
//! The enum is deliberately **not** `#[non_exhaustive]`: "three, not a
//! continuum" is the decision, so a caller matching all three should keep
//! compiling only for as long as that stays true. A fourth preset is a
//! decision to re-open on purpose, not a change to absorb quietly.

/// What a preset does to the images inside a page.
///
/// Absent ([`CompressPreset::image_policy`] returning `None`) means the
/// preset does not touch pixels at all — which is a stronger statement than
/// "very high quality", and the reason it is an `Option` rather than a row
/// with generous numbers in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePolicy {
    /// The *effective* DPI an image is resampled down to: its pixel count
    /// measured against the size the page's matrix actually draws it at, not
    /// against the raw dimensions of the XObject. An image already at or
    /// below this is left byte-identical — this is a ceiling, never a target
    /// to scale up to (`docs/batch-compress.md` decision 5).
    pub target_dpi: u32,
    /// Quality for re-encoding images that were **already** lossy. A lossless
    /// image is never converted into a JPEG to reach this number: JPEG has no
    /// alpha channel, and the `/SMask` would go with it.
    pub jpeg_quality: u8,
}

/// How hard [`crate::compress`] tries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressPreset {
    /// Structure only: object streams, a cross-reference stream, flate over
    /// streams that arrived unfiltered, and the prune of orphaned and
    /// duplicated resources. Renders pixel-for-pixel identically to the
    /// input, because it does not touch a single pixel.
    Lossless,
    /// [`CompressPreset::Lossless`], plus images resampled to 150 effective
    /// DPI and already-lossy ones re-encoded at quality 75. The default
    /// offer: visibly unchanged on screen, materially smaller on disk.
    Balanced,
    /// [`CompressPreset::Lossless`], plus 96 DPI at quality 55. For when the
    /// file has to fit through something — an upload limit, an attachment
    /// cap — and the user has decided that matters more than the pixels.
    Small,
}

impl CompressPreset {
    /// What this preset does to images, or `None` when it leaves them alone.
    pub fn image_policy(self) -> Option<ImagePolicy> {
        match self {
            CompressPreset::Lossless => None,
            CompressPreset::Balanced => Some(ImagePolicy {
                target_dpi: 150,
                jpeg_quality: 75,
            }),
            CompressPreset::Small => Some(ImagePolicy {
                target_dpi: 96,
                jpeg_quality: 55,
            }),
        }
    }

    /// Every preset, strongest-preserving first. Handed to shells so a
    /// preset dialog lists what exists rather than hard-coding three arms of
    /// its own — the same reason the numbers above are not in the UI either.
    pub fn all() -> [CompressPreset; 3] {
        [
            CompressPreset::Lossless,
            CompressPreset::Balanced,
            CompressPreset::Small,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decision 3's "no toca un solo píxel", as a test rather than a promise
    /// in a comment.
    #[test]
    fn lossless_has_no_image_policy_at_all() {
        assert_eq!(CompressPreset::Lossless.image_policy(), None);
    }

    /// Catches the typo that swaps the two rows of the table. Without this,
    /// a `Small` gentler than `Balanced` compiles, ships, and is only noticed
    /// by a user wondering why the smallest option produced the largest file.
    #[test]
    fn small_is_never_gentler_than_balanced() {
        let balanced = CompressPreset::Balanced
            .image_policy()
            .expect("Balanced resamples images");
        let small = CompressPreset::Small
            .image_policy()
            .expect("Small resamples images");

        assert!(
            small.target_dpi <= balanced.target_dpi,
            "Small ({} dpi) must not ask for more resolution than Balanced ({} dpi)",
            small.target_dpi,
            balanced.target_dpi
        );
        assert!(
            small.jpeg_quality <= balanced.jpeg_quality,
            "Small (q{}) must not ask for more quality than Balanced (q{})",
            small.jpeg_quality,
            balanced.jpeg_quality
        );
    }

    #[test]
    fn every_policy_stays_inside_the_jpeg_quality_range() {
        for preset in CompressPreset::all() {
            let Some(policy) = preset.image_policy() else {
                continue;
            };
            assert!(
                (1..=100).contains(&policy.jpeg_quality),
                "{preset:?} asks for quality {}, which is not a JPEG quality",
                policy.jpeg_quality
            );
            assert!(policy.target_dpi > 0, "{preset:?} asks for 0 dpi");
        }
    }

    #[test]
    fn all_lists_every_preset_once() {
        let presets = CompressPreset::all();
        assert_eq!(presets.len(), 3);
        for preset in presets {
            assert_eq!(
                presets.iter().filter(|&&other| other == preset).count(),
                1,
                "{preset:?} is listed more than once"
            );
        }
    }
}
