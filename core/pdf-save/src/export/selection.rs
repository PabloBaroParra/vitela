//! Which pages an export covers, and what each file is called.
//!
//! Pure functions of what the user typed: no document, no renderer, nothing
//! that can only run on one platform. That is exactly why they live in
//! `pdf-save` rather than in a shell — every shell that grows an export screen
//! needs this same grammar, and four copies of it would be four chances to
//! disagree about what `"7-3"` means. The same reasoning put the
//! text-selection geometry in `pdf-render` rather than in the GTK4 shell
//! (batch B8, T-046).

use std::fmt;

use super::ExportFormat;

/// Why a typed page selection could not be read.
///
/// Its `Display` is the message a shell shows verbatim — every variant is a
/// sentence addressed to the person who typed the range, not a developer
/// string. Keeping it here rather than in [`crate::SaveError`] is deliberate:
/// reading "1-3,7" touches no document and can fail long before a save is
/// attempted, so a caller handling it should not have to match on a variant of
/// the save error type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PageSelectionError {
    /// Nothing but separators and whitespace was typed.
    Empty,
    /// An entry that is not a number, quoted as it was typed. The empty string
    /// means a range was missing one of its two sides (`"3-"`).
    NotANumber(String),
    /// A page number outside `1..=total_pages`. `page: 0` is the one-based
    /// input's own lower bound being broken, not an out-of-bounds index.
    PageOutOfRange { page: u32, total_pages: u32 },
    /// A range whose second number precedes its first (`"7-3"`).
    DescendingRange { from: u32, to: u32 },
}

impl fmt::Display for PageSelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageSelectionError::Empty => {
                write!(f, "Type which pages to export, for example 1-3,7.")
            }
            PageSelectionError::NotANumber(text) if text.is_empty() => write!(
                f,
                "A page range needs a number on both sides of the dash, like 1-3."
            ),
            PageSelectionError::NotANumber(text) => {
                write!(f, "\"{text}\" is not a page number.")
            }
            PageSelectionError::PageOutOfRange { page: 0, .. } => {
                write!(f, "Pages are numbered from 1.")
            }
            PageSelectionError::PageOutOfRange { page, total_pages } => write!(
                f,
                "This document has {total_pages} pages, so page {page} does not exist."
            ),
            PageSelectionError::DescendingRange { from, to } => write!(
                f,
                "The range {from}-{to} runs backwards; write it as {to}-{from}."
            ),
        }
    }
}

impl std::error::Error for PageSelectionError {}

/// Reads a one-based page selection — `"1-3,7"` — into ascending, deduplicated
/// **zero-based** page indices.
///
/// Shared rather than shell-local on purpose: every shell that grows an export
/// screen needs exactly this grammar, and four copies of it would be four
/// chances to disagree about what `"7-3"` means (the same reasoning that put
/// the text-selection geometry in `pdf-render` rather than in the GTK4 shell).
///
/// Nothing here is silently repaired. A backwards range is refused rather than
/// reversed and a page past the end is refused rather than clamped, because
/// both "repairs" would export a set of pages the user did not ask for and
/// give no sign of having done so.
pub fn parse_page_selection(input: &str, total_pages: u32) -> Result<Vec<u32>, PageSelectionError> {
    let mut pages = Vec::new();
    for entry in input.split(',') {
        let entry = entry.trim();
        // A stray separator ("1,,2", or the trailing comma of a half-typed
        // list) is not worth refusing: it says nothing about which pages were
        // meant.
        if entry.is_empty() {
            continue;
        }
        let (from, to) = match entry.split_once('-') {
            Some((from, to)) => (one_based_page(from)?, one_based_page(to)?),
            None => {
                let page = one_based_page(entry)?;
                (page, page)
            }
        };
        for page in [from, to] {
            if page == 0 || page > total_pages {
                return Err(PageSelectionError::PageOutOfRange { page, total_pages });
            }
        }
        if from > to {
            return Err(PageSelectionError::DescendingRange { from, to });
        }
        pages.extend(from - 1..=to - 1);
    }
    if pages.is_empty() {
        return Err(PageSelectionError::Empty);
    }
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

fn one_based_page(text: &str) -> Result<u32, PageSelectionError> {
    let text = text.trim();
    text.parse::<u32>()
        .map_err(|_| PageSelectionError::NotANumber(text.to_owned()))
}

/// The file name one exported page is written under, inside a folder the user
/// chose: `"report-03.png"`.
///
/// `page_index` is zero-based (it indexes the document) and the name is
/// one-based (it is read by a person), padded to the width of `total_pages` so
/// the folder sorts in page order in any file manager.
///
/// `stem` comes from the opened document's file name, which the shell does not
/// control, so it is reduced to a single safe path component here rather than
/// at each call site: every run of characters that could redirect the write — a
/// separator above all — collapses to one `-`, and a stem left with nothing
/// usable becomes `"page"`.
pub fn page_image_file_name(
    stem: &str,
    page_index: u32,
    total_pages: u32,
    format: ExportFormat,
) -> String {
    let width = decimal_width(total_pages.max(1));
    format!(
        "{stem}-{number:0width$}.{extension}",
        stem = safe_stem(stem),
        number = page_index.saturating_add(1),
        width = width,
        extension = format.extension(),
    )
}

fn safe_stem(stem: &str) -> String {
    let mut safe = String::with_capacity(stem.len());
    for character in stem.chars() {
        if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
            safe.push(character);
        } else if !safe.ends_with('-') {
            safe.push('-');
        }
    }
    let safe = safe.trim_matches([' ', '-'].as_slice());
    if safe.is_empty() {
        "page".to_owned()
    } else {
        safe.to_owned()
    }
}

fn decimal_width(value: u32) -> usize {
    value.to_string().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_page_selects_that_page_zero_indexed() {
        assert_eq!(parse_page_selection("3", 10).unwrap(), vec![2]);
    }

    #[test]
    fn a_range_selects_every_page_it_spans_inclusive() {
        assert_eq!(parse_page_selection("2-4", 10).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn a_list_mixes_singles_and_ranges_and_tolerates_spaces() {
        assert_eq!(
            parse_page_selection(" 1, 3 - 4 ,7 ", 10).unwrap(),
            vec![0, 2, 3, 6]
        );
    }

    #[test]
    fn overlapping_entries_are_deduplicated_and_sorted() {
        assert_eq!(
            parse_page_selection("5,1-3,2", 10).unwrap(),
            vec![0, 1, 2, 4]
        );
    }

    #[test]
    fn an_empty_selection_is_refused_rather_than_read_as_every_page() {
        // "All pages" is a separate choice in the UI. An empty custom range is
        // a user who has not finished typing, not a request for the whole doc.
        assert!(matches!(
            parse_page_selection("   ", 10),
            Err(PageSelectionError::Empty)
        ));
    }

    #[test]
    fn page_zero_is_refused_because_the_input_is_one_based() {
        assert!(matches!(
            parse_page_selection("0", 10),
            Err(PageSelectionError::PageOutOfRange { page: 0, .. })
        ));
    }

    #[test]
    fn a_page_past_the_end_is_refused_naming_both_numbers() {
        assert!(matches!(
            parse_page_selection("4-11", 10),
            Err(PageSelectionError::PageOutOfRange {
                page: 11,
                total_pages: 10
            })
        ));
    }

    #[test]
    fn a_descending_range_is_refused_rather_than_silently_reversed() {
        assert!(matches!(
            parse_page_selection("7-3", 10),
            Err(PageSelectionError::DescendingRange { from: 7, to: 3 })
        ));
    }

    #[test]
    fn a_non_numeric_entry_is_refused_quoting_what_was_typed() {
        match parse_page_selection("1,two", 10) {
            Err(PageSelectionError::NotANumber(text)) => assert_eq!(text, "two"),
            other => panic!("expected NotANumber, got {other:?}"),
        }
    }

    #[test]
    fn a_range_with_a_missing_side_is_refused_and_not_read_as_open_ended() {
        assert!(matches!(
            parse_page_selection("3-", 10),
            Err(PageSelectionError::NotANumber(_))
        ));
    }

    #[test]
    fn every_refusal_says_something_a_person_can_act_on() {
        for input in ["", "0", "4-11", "7-3", "1,two"] {
            let message = parse_page_selection(input, 10).unwrap_err().to_string();
            assert!(
                !message.is_empty() && message.ends_with('.'),
                "{input:?} produced {message:?}"
            );
        }
    }

    // --- output file names -------------------------------------------------

    #[test]
    fn a_file_name_is_one_based_and_padded_to_the_width_of_the_last_page() {
        assert_eq!(
            page_image_file_name("report", 0, 9, ExportFormat::Png),
            "report-1.png"
        );
        assert_eq!(
            page_image_file_name("report", 0, 10, ExportFormat::Png),
            "report-01.png"
        );
        assert_eq!(
            page_image_file_name("report", 399, 400, ExportFormat::Jpeg),
            "report-400.jpg"
        );
    }

    #[test]
    fn a_stem_that_would_escape_the_chosen_folder_is_flattened() {
        // The stem comes from the opened file's name, which the shell does not
        // control. A separator in it must never redirect the write.
        assert_eq!(
            page_image_file_name("../etc/passwd", 0, 1, ExportFormat::Png),
            "etc-passwd-1.png"
        );
    }

    #[test]
    fn a_stem_with_nothing_usable_in_it_falls_back_to_a_name() {
        assert_eq!(
            page_image_file_name("", 0, 1, ExportFormat::Png),
            "page-1.png"
        );
        assert_eq!(
            page_image_file_name("   ", 0, 1, ExportFormat::Png),
            "page-1.png"
        );
        assert_eq!(
            page_image_file_name("///", 0, 1, ExportFormat::Png),
            "page-1.png"
        );
    }
}
