//! Split planning for shells that use the UniFFI boundary.

use crate::error::FfiError;

/// A file to create for an inclusive range of zero-based page positions.
#[derive(uniffi::Record)]
pub struct FfiSplitPart {
    pub first: u32,
    pub last: u32,
    pub file_name: String,
}

/// Plans all parts and safe file names using the same rules as the Linux shell.
#[uniffi::export]
pub fn plan_split(
    typed: String,
    total_pages: u32,
    document_name: String,
) -> Result<Vec<FfiSplitPart>, FfiError> {
    let cuts = pdf_save::resolve_split_cuts(&typed, total_pages)
        .map_err(|detail| FfiError::InvalidPageSelection { detail })?;
    let ranges = pdf_save::split_parts(&cuts, total_pages);
    let total = ranges.len() as u32;
    let stem = pdf_save::document_file_stem(&document_name);
    Ok(ranges
        .into_iter()
        .enumerate()
        .map(|(index, (first, last))| FfiSplitPart {
            first,
            last,
            file_name: pdf_save::split_part_file_name(stem, index as u32, total),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_names_the_same_parts_as_linux() {
        let parts = plan_split("3,7".into(), 10, "report.pdf".into()).unwrap();
        assert_eq!(
            parts
                .into_iter()
                .map(|part| (part.first, part.last, part.file_name))
                .collect::<Vec<_>>(),
            vec![
                (0, 2, "report-part1.pdf".into()),
                (3, 6, "report-part2.pdf".into()),
                (7, 9, "report-part3.pdf".into())
            ]
        );
    }
}
