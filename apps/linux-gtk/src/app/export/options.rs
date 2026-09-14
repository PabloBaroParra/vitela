//! What the Export images dialog is asking, as rules rather than widgets.
//!
//! Everything a person can get wrong is decided here, before a folder is
//! chosen and long before a pixel is rendered — which is why none of it takes
//! a widget: each rule is a plain function of its arguments, and each has a
//! test that reaches it directly.
//!
//! The page grammar itself — `"1-3,7"` — is **not** here either. It lives in
//! `pdf_save::export::selection`, shared with every other shell that grows an
//! export screen; this module only routes the message it produces to the
//! dialog's error label.

use std::path::Path;

use pdf_save::ExportFormat;

use crate::app::render::raster_dimensions;

/// The resolutions the spin button allows, and the one it starts on.
///
/// 72 is "screen size, one pixel per point"; 150 is the readable default a
/// scanned page is usually filed at.
///
/// The ceiling is 400 rather than the 600 an archival scan would suggest, and
/// the reason is a number that lives in another module: `render`'s
/// `MAX_RASTER_PIXELS` is 32 Mpx, and US Letter at 600 DPI is 5100x6600 =
/// 33.7 Mpx — over the line by half a percent. A4 at 600 misses it too. A
/// slider whose top notch is refused for the two most common page sizes in
/// the world is a slider that lies, so the notch is not offered. 400 clears
/// Letter, A4 and A3 with room to spare, and the pages that are still too
/// large at 400 — posters, plans — are caught by [`oversized_page`] before a
/// single file is written.
pub(super) const MIN_DPI: f64 = 72.0;
pub(super) const MAX_DPI: f64 = 400.0;
pub(super) const DEFAULT_DPI: f64 = 150.0;

/// The format dropdown's entries, in the order it lists them.
///
/// Indexed by `DropDown::selected`, so the array order *is* the mapping —
/// [`format_at`] is the only place that knows it.
pub(super) const FORMATS: [(&str, ExportFormat); 2] =
    [("PNG", ExportFormat::Png), ("JPEG", ExportFormat::Jpeg)];

/// Which pages the export covers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PageChoice {
    All,
    Current,
    /// Whatever was typed in the range entry.
    Custom,
}

/// What the dialog hands the export chain once every answer is valid.
pub(super) struct ExportOptions {
    /// Zero-based page indices, ascending and deduplicated.
    pub(super) pages: Vec<u32>,
    pub(super) dpi: u32,
    pub(super) format: ExportFormat,
}

/// The pages `choice` names, or the message to show instead.
///
/// Zero-based on the way out, because that is what the renderer indexes by;
/// one-based on the way in, because that is what the user typed.
pub(super) fn resolve_pages(
    choice: PageChoice,
    custom: &str,
    current_page: u32,
    total_pages: u32,
) -> Result<Vec<u32>, String> {
    if total_pages == 0 {
        return Err("This document has no pages to export.".to_owned());
    }
    match choice {
        PageChoice::All => Ok((0..total_pages).collect()),
        // Clamped rather than trusted: `current_page` comes from the viewport
        // readout, which a page deletion can leave one past the end for as
        // long as it takes the next scroll tick to correct it.
        PageChoice::Current => Ok(vec![current_page.min(total_pages - 1)]),
        PageChoice::Custom => {
            pdf_save::parse_page_selection(custom, total_pages).map_err(|error| error.to_string())
        }
    }
}

/// The first selected page whose raster would be too large to allocate at
/// `dpi`, as a **one-based** page number, or `None` when every page fits.
///
/// Asked before the export starts rather than discovered on page 300 of 400:
/// the ceiling is a property of the page's size and the chosen DPI, both of
/// which are known up front, so there is no reason to write 299 files and
/// then fail.
pub(super) fn oversized_page(page_sizes: &[(f32, f32)], pages: &[u32], dpi: u32) -> Option<u32> {
    pages
        .iter()
        .copied()
        .find(|&page| {
            page_sizes
                .get(page as usize)
                .is_some_and(|&(width_pt, height_pt)| {
                    raster_dimensions(width_pt, height_pt, dpi).is_none()
                })
        })
        .map(|page| page + 1)
}

/// What the status line says when an export finishes.
///
/// Its own function so the singular/plural and the "where did it go" half are
/// pinned by a test rather than by reading the `format!` at the call site.
pub(super) fn export_summary(count: usize, folder: &Path) -> String {
    let pages = if count == 1 { "page" } else { "pages" };
    format!("Exported {count} {pages} to {}.", folder.display())
}

/// The format at `index` in the dropdown, defaulting to PNG.
///
/// A `DropDown` with nothing selected reports `GTK_INVALID_LIST_POSITION`
/// rather than an error, so the fallback is a real branch and not defensive
/// noise.
pub(super) fn format_at(index: u32) -> ExportFormat {
    FORMATS
        .get(index as usize)
        .map_or(ExportFormat::Png, |&(_, format)| format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pages_is_every_index_in_the_document() {
        assert_eq!(
            resolve_pages(PageChoice::All, "", 0, 4).unwrap(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn the_current_page_is_the_only_one_exported() {
        assert_eq!(
            resolve_pages(PageChoice::Current, "", 2, 4).unwrap(),
            vec![2]
        );
    }

    /// The viewport readout can name a page a deletion has already removed;
    /// exporting the last page beats exporting nothing or panicking.
    #[test]
    fn a_current_page_past_the_end_lands_on_the_last_page() {
        assert_eq!(
            resolve_pages(PageChoice::Current, "", 9, 4).unwrap(),
            vec![3]
        );
    }

    #[test]
    fn a_custom_range_is_read_by_the_shared_grammar() {
        assert_eq!(
            resolve_pages(PageChoice::Custom, "1-2,4", 0, 4).unwrap(),
            vec![0, 1, 3]
        );
    }

    /// The dialog shows the core's sentence verbatim rather than a second,
    /// vaguer copy of it.
    #[test]
    fn a_bad_custom_range_surfaces_the_cores_own_message() {
        let message = resolve_pages(PageChoice::Custom, "9", 0, 4).unwrap_err();

        assert_eq!(
            message,
            pdf_save::PageSelectionError::PageOutOfRange {
                page: 9,
                total_pages: 4
            }
            .to_string()
        );
    }

    #[test]
    fn a_document_with_no_pages_is_refused_whatever_was_chosen() {
        for choice in [PageChoice::All, PageChoice::Current, PageChoice::Custom] {
            assert!(resolve_pages(choice, "1", 0, 0).is_err(), "{choice:?}");
        }
    }

    /// US Letter at 150 DPI is 1275x1650 — well within reach.
    #[test]
    fn ordinary_pages_at_an_ordinary_resolution_all_fit() {
        let sizes = vec![(612.0, 792.0); 3];

        assert_eq!(oversized_page(&sizes, &[0, 1, 2], 150), None);
    }

    /// The reason [`MAX_DPI`] is 400 and not 600, pinned as a test rather
    /// than left as a claim in a doc comment: Letter and A4 both clear the
    /// ceiling the spin button offers, and both are refused one notch past
    /// where it stops.
    #[test]
    fn letter_and_a4_both_fit_at_the_highest_resolution_the_dialog_offers() {
        let sizes = [(612.0, 792.0), (595.0, 842.0)];

        assert_eq!(oversized_page(&sizes, &[0, 1], MAX_DPI as u32), None);
        assert_eq!(oversized_page(&sizes, &[0, 1], 600), Some(1));
    }

    /// Reported before the first file is written, and named one-based so the
    /// number in the message is the number in the document.
    #[test]
    fn a_page_too_large_to_raster_is_named_before_any_file_is_written() {
        let sizes = vec![(612.0, 792.0), (200_000.0, 200_000.0)];

        assert_eq!(oversized_page(&sizes, &[0, 1], MAX_DPI as u32), Some(2));
    }

    #[test]
    fn a_page_the_document_does_not_have_is_not_called_oversized() {
        assert_eq!(oversized_page(&[(612.0, 792.0)], &[7], 150), None);
    }

    #[test]
    fn the_summary_counts_in_the_singular_when_one_page_was_written() {
        assert_eq!(
            export_summary(1, Path::new("/tmp/out")),
            "Exported 1 page to /tmp/out."
        );
        assert_eq!(
            export_summary(12, Path::new("/tmp/out")),
            "Exported 12 pages to /tmp/out."
        );
    }

    #[test]
    fn the_dropdown_order_is_the_format_mapping() {
        assert_eq!(format_at(0), ExportFormat::Png);
        assert_eq!(format_at(1), ExportFormat::Jpeg);
    }

    /// `GTK_INVALID_LIST_POSITION` is what an unselected `DropDown` reports.
    #[test]
    fn an_unselected_dropdown_falls_back_to_png() {
        assert_eq!(format_at(u32::MAX), ExportFormat::Png);
    }
}
