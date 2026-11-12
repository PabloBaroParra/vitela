//! A run's look read through the page's origin — what a shell asks before it
//! draws an editor over the run in the run's own font and colour.

use pdf_document::{Document, PageId};
use pdf_edit::FontProgramFormat;
use pdf_save::{text_run_font_program_of, text_run_style_of, ImportedSources};

fn opened_quote() -> (pdf_manip::LopdfDocument, Document) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/content-edit/reportlab_quote_paragraph.pdf");
    let bytes = std::fs::read(path).expect("committed fixture must be readable");
    let (base, security) = pdf_manip::open_document_from_bytes(&bytes, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();
    (base, document)
}

#[test]
fn the_quote_spec_run_reads_its_style_and_program_through_its_origin() {
    let (base, document) = opened_quote();
    let run = pdf_save::read_pending_page_content_of(
        &document,
        PageId(0),
        &base,
        ImportedSources::none(),
    )
    .unwrap()
    .text_runs
    .into_iter()
    .find(|run| run.text.starts_with("COMPOSICI"))
    .expect("the spec paragraph's first line");

    let style = text_run_style_of(&document, &base, ImportedSources::none(), &run)
        .unwrap()
        .expect("a page with content");
    let program = text_run_font_program_of(&document, &base, ImportedSources::none(), &run)
        .unwrap()
        .expect("an embedded subset");

    assert!((style.font_size - 9.0).abs() < 1e-6, "{}", style.font_size);
    assert!((style.fill_rgb[2] - 0.55).abs() < 1e-6);
    assert_eq!(program.format, FontProgramFormat::TrueType);
}
