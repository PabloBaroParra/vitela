//! What the Add watermark dialog is asking, as rules rather than widgets —
//! for a text stamp and a picture alike.
//!
//! Same cut as `export::options`: everything a person can get wrong is
//! decided by a plain function here, so each rule has a test that reaches it
//! without driving a dialog. The page grammar — `"1-3,7"` — is not here
//! either; it is `pdf_save::parse_page_selection`, shared with Export images.

use pdf_document::{Watermark, WatermarkLayer};

/// The size spin button's range, in points. The floor keeps the stamp
/// legible; the ceiling is past what fits across a portrait A4 page at the
/// default angle, so the top of the range is never a value nobody could use.
pub(super) const MIN_FONT_SIZE: f64 = 8.0;
pub(super) const MAX_FONT_SIZE: f64 = 200.0;

/// Opacity is asked in percent, because that is how every image editor asks
/// it. The floor is 5 rather than 1: the core refuses only a fully
/// transparent stamp, but one at 1% is a watermark nobody will ever find.
pub(super) const MIN_OPACITY_PERCENT: f64 = 5.0;
pub(super) const MAX_OPACITY_PERCENT: f64 = 100.0;

/// Counter-clockwise, as the reader sees the page.
pub(super) const MIN_ANGLE: f64 = -180.0;
pub(super) const MAX_ANGLE: f64 = 180.0;

/// A picture's share of the page, asked in percent like opacity. The floor
/// is the same "nobody would find it" line; `100` is the core's ceiling —
/// the picture touches two opposite edges of the page.
pub(super) const MIN_SCALE_PERCENT: f64 = 5.0;
pub(super) const MAX_SCALE_PERCENT: f64 = 100.0;

/// What the text entry starts with — selected, so typing replaces it.
pub(super) const DEFAULT_TEXT: &str = "DRAFT";

/// Which pages the stamp goes on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PageChoice {
    All,
    Current,
    /// Whatever was typed in the range entry.
    Custom,
}

/// Which kind of mark the dialog is building.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MarkKind {
    Text,
    Picture,
}

/// The mark half of the answers — what is painted, without where or how.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MarkOptions {
    Text {
        text: String,
        font_size: f64,
    },
    Picture {
        /// The picked file's bytes, untouched: decoding is `pdf-edit`'s job.
        bytes: Vec<u8>,
        /// `0.0` exclusive to `1.0` inclusive — already converted from percent.
        scale: f64,
    },
}

/// What the dialog hands the recording step once every answer is usable.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WatermarkOptions {
    /// Zero-based page positions, ascending and deduplicated.
    pub(crate) pages: Vec<u32>,
    pub(crate) mark: MarkOptions,
    /// `0.0` exclusive to `1.0` inclusive — already converted from percent.
    pub(crate) opacity: f64,
    pub(crate) rotation_degrees: f64,
    pub(crate) layer: WatermarkLayer,
}

/// The angle and layer the dialog switches to when the user picks `kind`.
///
/// The core's own defaults, not this shell's: diagonal text over the content,
/// an upright picture behind it. Switching kind resets both on purpose — a
/// logo left at text's 45° reads as a mistake, and a background picture
/// painted over the text hides the page.
pub(super) fn look_defaults(kind: MarkKind) -> (f64, WatermarkLayer) {
    match kind {
        MarkKind::Text => (
            Watermark::DEFAULT_ROTATION_DEGREES,
            WatermarkLayer::OverContent,
        ),
        MarkKind::Picture => (
            Watermark::DEFAULT_IMAGE_ROTATION_DEGREES,
            WatermarkLayer::UnderContent,
        ),
    }
}

/// The pages `choice` names, or the message to show instead.
///
/// Zero-based on the way out, one-based on the way in — the same contract as
/// `export::options::resolve_pages`.
pub(super) fn resolve_pages(
    choice: PageChoice,
    custom: &str,
    current_page: u32,
    total_pages: u32,
) -> Result<Vec<u32>, String> {
    if total_pages == 0 {
        return Err("This document has no pages to stamp.".to_owned());
    }
    match choice {
        PageChoice::All => Ok((0..total_pages).collect()),
        // Clamped for the reason export gives: the viewport readout can sit
        // one past the end until the next scroll tick after a deletion.
        PageChoice::Current => Ok(vec![current_page.min(total_pages - 1)]),
        PageChoice::Custom => {
            pdf_save::parse_page_selection(custom, total_pages).map_err(|error| error.to_string())
        }
    }
}

/// The text to stamp, or the message to show instead.
///
/// Kept as typed — leading spaces included — because the stamp is centred
/// on its own width and the user may have meant them. Only an all-blank
/// entry is refused, here rather than by the core, so the dialog can say
/// so in words before anything is recorded.
pub(super) fn resolve_text(typed: &str) -> Result<String, String> {
    if typed.trim().is_empty() {
        Err("Type the text to stamp on the pages.".to_owned())
    } else {
        Ok(typed.to_owned())
    }
}

/// The picked picture, or the message to show instead. Only "nothing
/// picked" is caught here; whether the bytes are a picture at all is
/// `pdf_edit::check_watermark`'s answer, given when the stamp is recorded.
pub(super) fn resolve_picture(picked: Option<&[u8]>) -> Result<Vec<u8>, String> {
    match picked {
        Some(bytes) if !bytes.is_empty() => Ok(bytes.to_vec()),
        _ => Err("Choose a PNG or JPEG picture to stamp on the pages.".to_owned()),
    }
}

/// Percent to the core's `(0, 1]` fraction — opacity and picture scale alike.
pub(super) fn fraction_from_percent(percent: f64) -> f64 {
    percent / 100.0
}

/// The opacity spin button's starting value: the core's default look.
pub(super) fn default_opacity_percent() -> f64 {
    Watermark::DEFAULT_OPACITY * 100.0
}

/// The scale spin button's starting value: the core's default picture size.
pub(super) fn default_scale_percent() -> f64 {
    Watermark::DEFAULT_IMAGE_SCALE * 100.0
}

/// What the status line says once the stamp is recorded.
pub(crate) fn summary(count: usize) -> String {
    let pages = if count == 1 { "page" } else { "pages" };
    format!("Watermark added to {count} {pages}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pages_names_every_page_in_order() {
        assert_eq!(resolve_pages(PageChoice::All, "", 0, 3), Ok(vec![0, 1, 2]));
    }

    #[test]
    fn the_current_page_is_clamped_to_the_document() {
        assert_eq!(resolve_pages(PageChoice::Current, "", 1, 3), Ok(vec![1]));
        assert_eq!(resolve_pages(PageChoice::Current, "", 7, 3), Ok(vec![2]));
    }

    #[test]
    fn a_custom_range_is_read_by_the_shared_grammar() {
        assert_eq!(
            resolve_pages(PageChoice::Custom, "1, 3-4", 0, 5),
            Ok(vec![0, 2, 3])
        );
    }

    #[test]
    fn a_bad_custom_range_surfaces_the_cores_own_message() {
        let expected = pdf_save::parse_page_selection("9", 3)
            .expect_err("page 9 of 3")
            .to_string();
        assert_eq!(resolve_pages(PageChoice::Custom, "9", 0, 3), Err(expected));
    }

    #[test]
    fn a_document_with_no_pages_is_refused_whatever_the_choice() {
        for choice in [PageChoice::All, PageChoice::Current, PageChoice::Custom] {
            assert!(resolve_pages(choice, "1", 0, 0).is_err());
        }
    }

    #[test]
    fn blank_text_is_refused_and_typed_text_is_kept_as_typed() {
        assert!(resolve_text("").is_err());
        assert!(resolve_text("   ").is_err());
        assert_eq!(resolve_text(" COPY").as_deref(), Ok(" COPY"));
    }

    #[test]
    fn opacity_and_scale_are_asked_in_percent_and_start_at_the_cores_defaults() {
        assert_eq!(fraction_from_percent(30.0), 0.3);
        assert_eq!(
            fraction_from_percent(default_opacity_percent()),
            Watermark::DEFAULT_OPACITY
        );
        assert_eq!(
            fraction_from_percent(default_scale_percent()),
            Watermark::DEFAULT_IMAGE_SCALE
        );
    }

    #[test]
    fn no_picture_picked_is_refused_and_a_picked_one_is_kept_whole() {
        assert!(resolve_picture(None).is_err());
        assert!(resolve_picture(Some(&[])).is_err());
        assert_eq!(resolve_picture(Some(&[1, 2, 3])), Ok(vec![1, 2, 3]));
    }

    #[test]
    fn each_kind_starts_from_the_cores_look() {
        assert_eq!(
            look_defaults(MarkKind::Text),
            (
                Watermark::DEFAULT_ROTATION_DEGREES,
                WatermarkLayer::OverContent
            )
        );
        assert_eq!(
            look_defaults(MarkKind::Picture),
            (
                Watermark::DEFAULT_IMAGE_ROTATION_DEGREES,
                WatermarkLayer::UnderContent
            )
        );
    }

    #[test]
    fn the_default_size_and_angle_sit_inside_the_dialogs_ranges() {
        assert!((MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&Watermark::DEFAULT_FONT_SIZE));
        assert!((MIN_ANGLE..=MAX_ANGLE).contains(&Watermark::DEFAULT_ROTATION_DEGREES));
        assert!((MIN_OPACITY_PERCENT..=MAX_OPACITY_PERCENT).contains(&default_opacity_percent()));
        assert!((MIN_SCALE_PERCENT..=MAX_SCALE_PERCENT).contains(&default_scale_percent()));
        assert!((MIN_ANGLE..=MAX_ANGLE).contains(&Watermark::DEFAULT_IMAGE_ROTATION_DEGREES));
    }

    #[test]
    fn the_summary_counts_pages_in_words() {
        assert_eq!(summary(1), "Watermark added to 1 page.");
        assert_eq!(summary(4), "Watermark added to 4 pages.");
    }
}
