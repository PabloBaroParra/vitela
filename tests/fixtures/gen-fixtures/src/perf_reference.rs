//! The text-heavy reference PDF the shells' performance baselines are
//! recorded against (`docs/perf-baseline.md`).
//!
//! The sibling of [`crate::large`], which is all image and no text: that one
//! measures raster decode, this one gives search, text selection and content
//! editing something to do on every page. Like it, the output is **not**
//! committed — it is written under the already-ignored
//! `tests/fixtures/large/` and regenerated on demand, byte for byte the same
//! for the same page count.

use std::io;
use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::xref::XrefType;
use lopdf::{dictionary, Document, Object, Stream};

/// The page count the baselines in `docs/perf-baseline.md` are recorded at.
pub const DEFAULT_PAGES: u32 = 1600;

/// Body lines on every page, under its heading.
pub const LINES_PER_PAGE: usize = 44;

const WORDS_PER_LINE: usize = 10;
/// A blank line's worth of extra leading after every this many lines, so the
/// body reads as paragraphs rather than one block.
const LINES_PER_PARAGRAPH: usize = 6;

const WORDS: [&str; 32] = [
    "vellum", "margin", "ledger", "harbor", "signal", "quartz", "meadow", "lantern", "archive",
    "copper", "window", "garden", "thread", "figure", "column", "border", "station", "paper",
    "river", "stone", "cedar", "amber", "notch", "field", "ink", "seal", "fold", "page", "line",
    "mark", "note", "leaf",
];

// Layout, in points, on a 612 x 792 (US Letter) page: the last of the 44
// body lines still sits above the bottom margin.
const MARGIN_PT: i64 = 54;
const HEADING_BASELINE_PT: i64 = 738;
const HEADING_SIZE_PT: i64 = 16;
const BODY_SIZE_PT: i64 = 10;
const BODY_LEADING_PT: i64 = 13;
const PARAGRAPH_GAP_PT: i64 = 6;

/// Deterministic xorshift32, as in [`crate::large`]: no RNG crate for
/// fixture filler.
fn xorshift32(seed: u32) -> impl FnMut() -> u32 {
    let mut state = if seed == 0 { 1 } else { seed };
    move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    }
}

/// The heading of 1-based page `number` — a phrase that occurs exactly once
/// in the document, so a search for it has exactly one right answer.
pub fn page_heading(number: u32, pages: u32) -> String {
    format!("Reference page {number} of {pages}")
}

/// The body lines of 1-based page `number`: [`LINES_PER_PAGE`] lines of
/// dictionary words, the same for the same page every time.
pub fn page_lines(number: u32) -> Vec<String> {
    let mut next = xorshift32(number.wrapping_mul(2_654_435_761).wrapping_add(1));
    (0..LINES_PER_PAGE)
        .map(|_| {
            (0..WORDS_PER_LINE)
                .map(|_| WORDS[next() as usize % WORDS.len()])
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Builds the document in memory: `pages` US Letter pages sharing one
/// Helvetica font, each a heading and [`LINES_PER_PAGE`] lines of text in a
/// Flate-compressed content stream.
pub fn build_reference_document(pages: u32) -> lopdf::Result<Document> {
    let mut doc = Document::with_version("1.5");
    // Classic xref table, as every other fixture in this crate.
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut kids = Vec::with_capacity(pages as usize);
    for number in 1..=pages {
        let mut operations = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), HEADING_SIZE_PT.into()]),
            Operation::new("Td", vec![MARGIN_PT.into(), HEADING_BASELINE_PT.into()]),
            Operation::new(
                "Tj",
                vec![Object::string_literal(page_heading(number, pages))],
            ),
            Operation::new("Tf", vec!["F1".into(), BODY_SIZE_PT.into()]),
            Operation::new("Td", vec![0.into(), (-2 * BODY_LEADING_PT).into()]),
        ];
        for (index, line) in page_lines(number).into_iter().enumerate() {
            if index > 0 {
                let paragraph_break = index % LINES_PER_PARAGRAPH == 0;
                let leading = if paragraph_break {
                    BODY_LEADING_PT + PARAGRAPH_GAP_PT
                } else {
                    BODY_LEADING_PT
                };
                operations.push(Operation::new("Td", vec![0.into(), (-leading).into()]));
            }
            operations.push(Operation::new("Tj", vec![Object::string_literal(line)]));
        }
        operations.push(Operation::new("ET", vec![]));

        let mut content_stream = Stream::new(dictionary! {}, Content { operations }.encode()?);
        content_stream.compress()?;
        let content_id = doc.add_object(content_stream);

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        kids.push(page_id.into());
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => i64::from(pages),
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    Ok(doc)
}

/// Where [`generate_reference_pdf`] writes by default: the ignored
/// `tests/fixtures/large/` directory, named after the page count.
pub fn default_output_path(pages: u32) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("large")
        .join(format!("perf_reference_{pages}pg.pdf"))
}

/// Writes the `pages`-page reference document to `out_path`, creating parent
/// directories as needed.
pub fn generate_reference_pdf(out_path: &Path, pages: u32) -> io::Result<PathBuf> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut doc =
        build_reference_document(pages).map_err(|error| io::Error::other(error.to_string()))?;
    doc.save(out_path)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(out_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every string a page's content stream shows, in order.
    fn shown_text(doc: &Document, page_number: u32) -> Vec<String> {
        let page_id = doc.get_pages()[&page_number];
        let content = doc.get_page_content(page_id);
        Content::decode(&content)
            .expect("a decodable content stream")
            .operations
            .into_iter()
            .filter(|operation| operation.operator == "Tj")
            .map(|operation| {
                let bytes = operation.operands[0].as_str().expect("a string operand");
                String::from_utf8(bytes.to_vec()).expect("ASCII text")
            })
            .collect()
    }

    #[test]
    fn the_document_has_the_requested_number_of_pages() {
        let doc = build_reference_document(7).expect("the document builds");
        assert_eq!(doc.get_pages().len(), 7);
    }

    #[test]
    fn every_page_carries_its_heading_and_a_full_body_of_text() {
        let doc = build_reference_document(3).expect("the document builds");
        for number in 1..=3 {
            let shown = shown_text(&doc, number);
            assert_eq!(shown.len(), 1 + LINES_PER_PAGE);
            assert_eq!(shown[0], page_heading(number, 3));
            assert_eq!(shown[1..], page_lines(number));
        }
    }

    #[test]
    fn a_heading_names_its_page_and_the_page_count() {
        assert_eq!(page_heading(12, 1600), "Reference page 12 of 1600");
    }

    #[test]
    fn body_lines_are_the_same_every_time_and_differ_between_pages() {
        assert_eq!(page_lines(5), page_lines(5));
        assert_ne!(page_lines(5), page_lines(6));
        assert!(page_lines(5)
            .iter()
            .all(|line| line.split(' ').count() == WORDS_PER_LINE));
    }

    #[test]
    fn the_saved_file_reads_back_with_every_page() {
        let directory = std::env::temp_dir().join(format!(
            "gen-fixtures-perf-reference-{}",
            std::process::id()
        ));
        let path = directory.join("reference.pdf");
        generate_reference_pdf(&path, 4).expect("the file is written");
        let reread = Document::load(&path).expect("the file loads");
        assert_eq!(reread.get_pages().len(), 4);
        assert_eq!(shown_text(&reread, 4)[0], page_heading(4, 4));
        std::fs::remove_dir_all(&directory).expect("the scratch directory is removed");
    }

    #[test]
    fn the_default_output_is_the_ignored_large_fixture_directory() {
        let path = default_output_path(1600);
        assert!(path.ends_with("large/perf_reference_1600pg.pdf"));
    }
}
