//! Paragraph text blocks through the whole save: a real file's multi-line
//! paragraph retyped to more and to fewer lines, recorded the way every
//! shell records a block edit, saved, reopened — and still one paragraph in
//! the same font, leading and colour.
//!
//! The fixture is `tests/fixtures/content-edit/reportlab_quote_paragraph.pdf`
//! (see its generator script): a quote with a four-line specification
//! paragraph in an embedded subset TrueType font, dark blue with `0.3 Tc`,
//! and a small table in the same font.

use pdf_document::{Command, Document, PageId};
use pdf_edit::{group_text_runs, TextBlock};
use pdf_save::{save_document, ImportedSources, SaveInput, SaveIntent, SignatureAcknowledgement};

const SPEC: &str = "COMPOSICION: Aluminio lacado con rotura de puente termico\n\
                    COLOR: Blanco RAL 9016 mate\n\
                    APERTURA: Corredera de dos hojas\n\
                    VIDRIO: Doble 4/16/4 bajo emisivo";

/// The spec paragraph's fill colour and character spacing, as the fixture
/// writes them.
const SPEC_COLOUR: &str = ".1 .2 .55 rg";
const SPEC_SPACING: &str = ".3 Tc";

fn fixture_bytes() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/content-edit/reportlab_quote_paragraph.pdf");
    std::fs::read(path).expect("committed fixture must be readable")
}

fn blocks_of(document: &lopdf::Document) -> Vec<TextBlock> {
    let runs = pdf_edit::read_page_content(document, PageId(0))
        .expect("readable page")
        .text_runs;
    group_text_runs(&runs)
}

fn spec_block(blocks: &[TextBlock]) -> &TextBlock {
    blocks
        .iter()
        .find(|block| block.text.starts_with("COMPOSICION") || block.lines.len() > 1)
        .expect("the spec paragraph")
}

/// Every content stream of page one, decoded and concatenated.
fn page_content(document: &lopdf::Document) -> String {
    let page = pdf_edit::page_object_id(document, PageId(0)).unwrap();
    let mut content = String::new();
    for id in document.get_page_contents(page) {
        let stream = document.get_object(id).unwrap().as_stream().unwrap();
        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        content.push_str(&String::from_utf8_lossy(&bytes));
    }
    content
}

/// Retypes the spec paragraph to `text` through `record_content_batch`,
/// saves, and reopens the result.
fn retype_spec(text: &str) -> lopdf::Document {
    let bytes = fixture_bytes();
    let (base, security) = pdf_manip::open_document_from_bytes(&bytes, None).unwrap();
    let mut document: Document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let runs = pdf_save::read_pending_page_content_of(
        &document,
        PageId(0),
        &base,
        ImportedSources::none(),
    )
    .unwrap()
    .text_runs;
    let block = spec_block(&group_text_runs(&runs)).clone();
    let block_runs: Vec<_> = block
        .runs
        .iter()
        .map(|id| runs.iter().find(|run| run.id == *id).unwrap().clone())
        .collect();

    let command = pdf_save::retype_text_block_of(
        &document,
        &base,
        ImportedSources::none(),
        &block_runs,
        text,
        false,
    )
    .expect("retypable")
    .expect("a batch");
    assert!(matches!(command, Command::Batch(_)));
    pdf_save::record_content_batch(&mut document, &base, ImportedSources::none(), command)
        .expect("recorded");

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: ImportedSources::none(),
    })
    .expect("saved");
    lopdf::Document::load_mem(&saved).expect("the output reloads")
}

#[test]
fn the_fixture_reads_as_heading_paragraph_and_table_cells() {
    let document = lopdf::Document::load_mem(&fixture_bytes()).unwrap();
    let blocks = blocks_of(&document);

    let spec = spec_block(&blocks);
    assert_eq!(spec.text, SPEC);
    assert_eq!(spec.lines.len(), 4);
    assert!((spec.leading.unwrap() - 11.0).abs() < 1e-6);
    let texts: Vec<&str> = blocks.iter().map(|block| block.text.as_str()).collect();
    assert!(texts.contains(&"PRESUPUESTO 3.725/1"), "{texts:?}");
    // Nine table cells, each a block of one line.
    let cells = blocks.iter().filter(|block| block.bbox.y < 600.0).count();
    assert_eq!(cells, 9, "{texts:?}");
    assert!(blocks
        .iter()
        .filter(|block| block.bbox.y < 600.0)
        .all(|block| block.lines.len() == 1));
}

#[test]
fn a_paragraph_retyped_to_more_lines_saves_as_one_paragraph_in_its_own_look() {
    let text = format!("{SPEC}\nMEDIDAS: 1200 x 1000 mm\nHERRAJES: Acero inoxidable");
    let saved = retype_spec(&text);

    let blocks = blocks_of(&saved);
    let spec = spec_block(&blocks);
    assert_eq!(spec.text, text);
    assert_eq!(spec.lines.len(), 6);
    assert!((spec.leading.unwrap() - 11.0).abs() < 1e-6);

    let runs = pdf_edit::read_page_content(&saved, PageId(0))
        .unwrap()
        .text_runs;
    let fonts: Vec<&str> = spec
        .runs
        .iter()
        .map(|id| {
            runs.iter()
                .find(|run| run.id == *id)
                .unwrap()
                .resource_font_name
                .as_str()
        })
        .collect();
    assert!(fonts.iter().all(|font| *font == fonts[0]), "{fonts:?}");

    // The original line's colour and spacing, plus one copy for each
    // inserted line.
    let content = page_content(&saved);
    assert_eq!(content.matches(SPEC_COLOUR).count(), 3, "{content}");
    assert_eq!(content.matches(SPEC_SPACING).count(), 3, "{content}");
}

#[test]
fn a_paragraph_retyped_to_fewer_lines_saves_as_one_paragraph() {
    let saved = retype_spec("COMPOSICION: PVC\nCOLOR: Gris antracita");

    let blocks = blocks_of(&saved);
    let spec = spec_block(&blocks);
    assert_eq!(spec.text, "COMPOSICION: PVC\nCOLOR: Gris antracita");
    assert!((spec.leading.unwrap() - 11.0).abs() < 1e-6);
    // The table is untouched.
    let cells = blocks.iter().filter(|block| block.bbox.y < 600.0).count();
    assert_eq!(cells, 9);
}

/// A second retype built from the runs the file held — not the ones the
/// first retype queued — cannot name the line the first one inserted. It
/// is refused, never folded with that line kept (ghost text on save).
#[test]
fn a_second_retype_from_the_original_runs_is_refused_not_ghosted() {
    let bytes = fixture_bytes();
    let (base, security) = pdf_manip::open_document_from_bytes(&bytes, None).unwrap();
    let mut document: Document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let runs = pdf_edit::read_page_content(base.as_lopdf(), PageId(0))
        .unwrap()
        .text_runs;
    let block = spec_block(&group_text_runs(&runs)).clone();
    let block_runs: Vec<_> = block
        .runs
        .iter()
        .map(|id| runs.iter().find(|run| run.id == *id).unwrap().clone())
        .collect();
    let mut retype = |text: &str| {
        let none = ImportedSources::none();
        let command =
            pdf_save::retype_text_block_of(&document, &base, none, &block_runs, text, false)
                .expect("retypable")
                .expect("a batch");
        pdf_save::record_content_batch(&mut document, &base, none, command)
    };

    retype(&format!("{SPEC}\nUNA")).expect("the first retype records");
    let second = retype("A\nB\nC\nD");

    assert!(second.is_err(), "the stale inserted line would be kept");
}
