//! What the files a document is turned into are called.
//!
//! Beside [`selection`](super::selection) and pure for the same reason: no
//! document, no renderer, nothing that can only run on one platform. Every
//! shell that writes files *derived* from an open PDF — one image per page,
//! one PDF per part of a split — has to name them, and four independent
//! answers would be four chances to disagree about what `"report.pdf.backup"`
//! is called.
//!
//! The reason this is shared rather than merely convenient is the separator.
//! A stem comes from an opened document's own file name, which no shell
//! controls, so every name produced here is reduced to a **single path
//! component**: a caller joining one onto a folder the user chose cannot land
//! anywhere else.

use super::ExportFormat;

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

/// The part of a document's own file name that files derived from it are
/// named after.
///
/// A `.pdf` extension is dropped and everything else is kept — including a
/// name that merely *contains* `".pdf"`, which is a name and not an
/// extension. Shared rather than shell-local for the same reason the rest of
/// this module is: every shell that writes files *beside* a document — page
/// images, split parts — asks this same question, and four answers would be
/// four chances to disagree about what `"report.pdf.backup"` is called.
///
/// What comes back is a **name**, not yet a safe path component.
/// [`page_image_file_name`] and [`split_part_file_name`] are what reduce it to
/// one.
pub fn document_file_stem(base_name: &str) -> &str {
    base_name
        .strip_suffix(".pdf")
        .or_else(|| base_name.strip_suffix(".PDF"))
        .unwrap_or(base_name)
}

/// The file name one part of a split document is written under, inside a
/// folder the user chose: `"report-part1.pdf"`.
///
/// `part_index` is zero-based (it indexes the list of parts) and the name is
/// one-based (it is read by a person), padded to the width of `total_parts`
/// so the folder sorts in document order in any file manager.
///
/// Beside [`page_image_file_name`] rather than in a shell, and for the same
/// two reasons: a split writes *many* files under names the user never types,
/// and `stem` comes from an opened document's file name, which no shell
/// controls. Reducing it to a single safe path component happens here, once,
/// so no caller can join a separator onto a folder the user chose.
pub fn split_part_file_name(stem: &str, part_index: u32, total_parts: u32) -> String {
    let width = decimal_width(total_parts.max(1));
    format!(
        "{stem}-part{number:0width$}.pdf",
        stem = safe_stem(stem),
        number = part_index.saturating_add(1),
        width = width,
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

    #[test]
    fn a_pdf_extension_is_dropped_from_the_derived_stem() {
        assert_eq!(document_file_stem("report.pdf"), "report");
        assert_eq!(document_file_stem("REPORT.PDF"), "REPORT");
    }

    /// The sample's stand-in name has no extension to drop, and a name that
    /// merely *contains* ".pdf" keeps it — it is a name, not an extension.
    #[test]
    fn a_name_without_a_pdf_extension_is_used_whole() {
        assert_eq!(document_file_stem("Vitela sample"), "Vitela sample");
        assert_eq!(document_file_stem("report.pdf.backup"), "report.pdf.backup");
    }

    #[test]
    fn a_split_part_is_named_one_based_after_the_document() {
        assert_eq!(split_part_file_name("report", 0, 2), "report-part1.pdf");
        assert_eq!(split_part_file_name("report", 1, 2), "report-part2.pdf");
    }

    /// Ten parts or more pad, so a folder listing sorts in document order
    /// rather than putting part 10 between parts 1 and 2.
    #[test]
    fn a_split_into_ten_or_more_parts_pads_the_number() {
        assert_eq!(split_part_file_name("report", 0, 10), "report-part01.pdf");
        assert_eq!(split_part_file_name("report", 9, 10), "report-part10.pdf");
    }

    /// The same flattening `page_image_file_name` performs, for the same
    /// reason: the stem comes from a file name no shell controls, and a
    /// separator in it must never redirect the write out of the chosen folder.
    #[test]
    fn a_split_part_stem_that_would_escape_the_chosen_folder_is_flattened() {
        assert_eq!(
            split_part_file_name("../etc/passwd", 0, 1),
            "etc-passwd-part1.pdf"
        );
        assert_eq!(split_part_file_name("   ", 0, 1), "page-part1.pdf");
    }
}
