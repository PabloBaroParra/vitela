//! Builds the sample document that ships **inside** every Vitela platform
//! shell, so a fresh install can render, scroll, search, and print something
//! real without the user first having to find a PDF of their own.
//!
//! This is a product asset, not a test fixture — which is why it lives here
//! and not in `tests/fixtures/gen-fixtures`. The two generators deliberately
//! stay separate: fixtures exist to exercise edge cases (encryption,
//! signatures, 50MB perf corpora) and may be regenerated freely, while this
//! file is committed, packaged into three shells, and shown to users.
//!
//! The output is **byte-reproducible**: no timestamps, no random file ID, and
//! a classic cross-reference table. Regenerating on a clean checkout must
//! leave the committed file unchanged, so a diff here always means the
//! content actually changed.

use std::io;
use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::xref::XrefType;
use lopdf::{dictionary, Document, Object, Stream};

/// File name of the shipped sample, identical across all platforms. Each
/// shell packages the very same file from `assets/sample/`, so a bug
/// reproduced on one platform is reproducible on the others.
pub const SAMPLE_FILE_NAME: &str = "vitela-sample.pdf";

/// US Letter, in PostScript points — matches the fixture generators.
const PAGE_WIDTH_PT: i64 = 612;
const PAGE_HEIGHT_PT: i64 = 792;

/// Baseline of the first line on every page, and the vertical step between
/// lines. Chosen so the longest page below stays well inside the media box.
const FIRST_BASELINE_PT: i64 = 700;
const LINE_HEIGHT_PT: i64 = 32;
const LEFT_MARGIN_PT: i64 = 72;

/// The first line of every page is its heading; the rest is body text.
const HEADING_SIZE_PT: i64 = 28;
const BODY_SIZE_PT: i64 = 14;

/// A word that appears on **every** page of the sample. The shells' search
/// boxes are exact and case-sensitive, so a term guaranteed to hit more than
/// once gives "next/previous match" something to step through out of the box.
pub const SAMPLE_SEARCH_TERM: &str = "vellum";

/// The sample's text, one entry per page. Deliberately plain Helvetica text:
/// pdfium's text extraction backs the search feature, and a scanned-image
/// sample would silently render search useless.
pub const SAMPLE_PAGES: &[&[&str]] = &[
    &[
        "Vitela",
        "A sample document",
        "",
        "This file ships inside the app. It is here so you",
        "can scroll, search, and print straight away, with",
        "no document of your own required.",
        "",
        "Vitela is Spanish for vellum.",
    ],
    &[
        "Page two of three",
        "",
        "Scrolling down rendered this page on demand.",
        "Pages are rasterised only as they come into view,",
        "so a long document opens as fast as a short one.",
        "",
        "This page also mentions vellum.",
    ],
    &[
        "Page three of three",
        "",
        "Try the search box: look for the word vellum and",
        "step through the matches. Search is exact and",
        "case-sensitive on every platform.",
        "",
        "Nothing here ever leaves your machine.",
    ],
];

/// Builds the sample document in memory. No I/O, so tests can assert on the
/// document structure without touching the filesystem.
pub fn build_sample_document() -> lopdf::Result<Document> {
    let mut doc = Document::with_version("1.5");
    // Classic xref table rather than a cross-reference stream: it keeps the
    // shipped file inspectable with a text editor, and matches the format the
    // fixture generators already emit.
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    // One shared resource dictionary for every page — the sample uses a
    // single font and no images.
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut kids = Vec::with_capacity(SAMPLE_PAGES.len());
    for lines in SAMPLE_PAGES {
        let content_id = doc.add_object(Stream::new(dictionary! {}, page_content(lines)?));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), PAGE_WIDTH_PT.into(), PAGE_HEIGHT_PT.into()],
        });
        kids.push(page_id.into());
    }

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => kids,
        "Count" => SAMPLE_PAGES.len() as i64,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    // A fixed, non-random file ID. `pdf-save`'s production writer injects a
    // real generator; here a constant is what makes regeneration
    // byte-reproducible, which is the property the committed file relies on.
    let file_id = Object::string_literal("vitela-sample-document");
    doc.trailer.set("ID", vec![file_id.clone(), file_id]);

    Ok(doc)
}

/// Encodes one page's content stream: each line is its own text object at a
/// descending baseline. Blank entries advance the baseline without drawing,
/// which is how the sample gets its paragraph spacing.
fn page_content(lines: &[&str]) -> lopdf::Result<Vec<u8>> {
    let mut operations = Vec::new();
    let mut baseline = FIRST_BASELINE_PT;
    for (index, line) in lines.iter().enumerate() {
        if !line.is_empty() {
            let size = if index == 0 {
                HEADING_SIZE_PT
            } else {
                BODY_SIZE_PT
            };
            operations.push(Operation::new("BT", vec![]));
            operations.push(Operation::new("Tf", vec!["F1".into(), size.into()]));
            operations.push(Operation::new(
                "Td",
                vec![LEFT_MARGIN_PT.into(), baseline.into()],
            ));
            operations.push(Operation::new("Tj", vec![Object::string_literal(*line)]));
            operations.push(Operation::new("ET", vec![]));
        }
        baseline -= LINE_HEIGHT_PT;
    }

    Content { operations }.encode()
}

/// Serialises the sample to bytes without touching the filesystem.
pub fn sample_bytes() -> lopdf::Result<Vec<u8>> {
    let mut doc = build_sample_document()?;
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    Ok(bytes)
}

/// Writes the sample to `out_path`, creating parent directories as needed.
/// Returns the path written.
pub fn generate_sample(out_path: &Path) -> io::Result<PathBuf> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = sample_bytes().map_err(|error| io::Error::other(error.to_string()))?;
    std::fs::write(out_path, bytes)?;
    Ok(out_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_has_one_page_per_entry() {
        let doc = build_sample_document().unwrap();
        assert_eq!(doc.get_pages().len(), SAMPLE_PAGES.len());
    }

    #[test]
    fn sample_has_more_than_one_page() {
        // Multi-page is the point: a single-page sample would exercise
        // neither scrolling nor page-range printing.
        assert!(SAMPLE_PAGES.len() > 1);
    }

    #[test]
    fn every_page_contains_the_search_term() {
        for (index, lines) in SAMPLE_PAGES.iter().enumerate() {
            assert!(
                lines.iter().any(|line| line.contains(SAMPLE_SEARCH_TERM)),
                "page {index} does not contain {SAMPLE_SEARCH_TERM:?}"
            );
        }
    }

    #[test]
    fn sample_is_not_encrypted() {
        // Shipping an encrypted sample would greet a first run with a
        // password prompt — the opposite of the point.
        let doc = build_sample_document().unwrap();
        assert!(doc.trailer.get(b"Encrypt").is_err());
    }

    #[test]
    fn sample_bytes_are_reproducible() {
        assert_eq!(sample_bytes().unwrap(), sample_bytes().unwrap());
    }

    #[test]
    fn sample_bytes_are_a_pdf() {
        assert!(sample_bytes().unwrap().starts_with(b"%PDF-"));
    }
}
