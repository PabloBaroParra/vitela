//! Opening one PDF as the source of an import, before anything is added:
//! the first of the two phases every boundary that imports runs (the second
//! is [`crate::import_sources`]).
//!
//! This used to live only in `pdf-ffi`'s `prepare_import`, so `pdf-wasm`
//! would have had to copy it (B26 T-227). A boundary calls it once per picked
//! file, so it can ask a locked file for its own password between files, and
//! abandons an import by dropping what it prepared.
//!
//! The rules are `pdf_manip`'s: [`pdf_manip::open_import_source_from_bytes`]
//! decrypts and asks the source's own copy permission before a page is read,
//! and [`pdf_manip::graft_report`] refuses what a graft cannot carry (a
//! signature, an XFA form, optional content, inline form fields) and lists
//! what it would lose. Asked over every page: an import takes them all.

use pdf_manip::{LopdfDocument, ManipError};

/// One source PDF, opened and checked, waiting to be added by
/// [`crate::import_sources`]. Holds the decrypted document.
pub struct PreparedSource {
    source: LopdfDocument,
    page_count: u32,
    warnings: Vec<String>,
}

impl std::fmt::Debug for PreparedSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedSource")
            .field("page_count", &self.page_count)
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

impl PreparedSource {
    /// How many pages importing this source adds — every page it has. Never
    /// zero.
    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    /// What this source's pages will not bring across exactly as they are
    /// (a form field renamed on arrival, a link that pointed outside the
    /// imported pages, …), one reader-facing line each — known before
    /// anything is added, so a boundary can ask before importing.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The decrypted document, for the slot [`crate::import_sources`] takes.
    pub fn into_source(self) -> LopdfDocument {
        self.source
    }
}

/// Why [`prepare_source`] refused a source. Its `Display` is the
/// reader-facing message.
#[derive(Debug)]
pub enum PrepareError {
    /// The open, the copy permission or the graft check refused it — a
    /// password one included (`PasswordRequired` / `WrongPassword`, which a
    /// boundary answers by asking for the source's password and calling
    /// again).
    Open(ManipError),
    /// The PDF has no pages to import.
    NoPages,
    /// The PDF has more pages than a page count can name.
    TooManyPages,
}

impl std::fmt::Display for PrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrepareError::Open(error) => error.fmt(f),
            PrepareError::NoPages => f.write_str("the PDF has no pages"),
            PrepareError::TooManyPages => f.write_str("the PDF has too many pages"),
        }
    }
}

impl std::error::Error for PrepareError {}

impl From<ManipError> for PrepareError {
    fn from(error: ManipError) -> Self {
        PrepareError::Open(error)
    }
}

/// Opens the PDF in `bytes` as an import source under `password` (none for
/// an unencrypted one): decrypts it, checks its own copy permission, and
/// checks every page can be grafted. Changes nothing anywhere.
///
/// A missing password may come back as either `PasswordRequired` or
/// `WrongPassword` (see [`pdf_manip::open_import_source_from_bytes`]); both
/// mean "ask for it and call again".
pub fn prepare_source(
    bytes: &[u8],
    password: Option<&str>,
) -> Result<PreparedSource, PrepareError> {
    let (source, _) = pdf_manip::open_import_source_from_bytes(bytes, password)?;
    let every_page: Vec<usize> = (0..source.page_count()).collect();
    if every_page.is_empty() {
        return Err(PrepareError::NoPages);
    }
    let warnings = pdf_manip::graft_report(&source, &every_page)?
        .warnings()
        .iter()
        .map(ToString::to_string)
        .collect();
    let page_count = u32::try_from(every_page.len()).map_err(|_| PrepareError::TooManyPages)?;
    Ok(PreparedSource {
        source,
        page_count,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Orientation, PageSize};

    fn pdf_bytes(pages: usize) -> Vec<u8> {
        let mut document = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
        for index in 0..pages {
            document =
                pdf_manip::insert_blank_page(&document, index, PageSize::A4, Orientation::Portrait)
                    .expect("blank page");
        }
        let mut bytes = Vec::new();
        document.into_lopdf().save_to(&mut bytes).expect("saves");
        bytes
    }

    #[test]
    fn a_plain_pdf_is_prepared_with_every_page() {
        let prepared = prepare_source(&pdf_bytes(3), None).expect("prepares");
        assert_eq!(prepared.page_count(), 3);
        assert!(prepared.warnings().is_empty());
        assert_eq!(prepared.into_source().page_count(), 3);
    }

    #[test]
    fn a_pdf_without_pages_is_refused_in_the_readers_words() {
        let err = prepare_source(&pdf_bytes(0), None).expect_err("no pages");
        assert!(matches!(err, PrepareError::NoPages));
        assert_eq!(err.to_string(), "the PDF has no pages");
    }

    #[test]
    fn bytes_that_are_not_a_pdf_are_an_open_failure() {
        let err = prepare_source(b"not a pdf", None).expect_err("not a PDF");
        assert!(matches!(err, PrepareError::Open(_)), "{err:?}");
    }
}
