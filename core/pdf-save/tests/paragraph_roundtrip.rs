//! Paragraph text blocks through the whole save: a real file's multi-line
//! paragraph retyped to more and to fewer lines, recorded the way every
//! shell records a block edit, saved, reopened — and still one paragraph in
//! the same font, leading and colour.
//!
//! The fixture is `tests/fixtures/content-edit/reportlab_quote_paragraph.pdf`
//! (see its generator script): a quote with a four-line specification
//! paragraph in an embedded subset TrueType font, dark blue with `0.3 Tc`,
//! and a small table in the same font. Its accented letters have codes only
//! the font's /ToUnicode CMap explains, so these tests also pin that text
//! reads, retypes and saves in the font's own subset.

use pdf_document::{Command, Document, PageId};
use pdf_edit::{group_text_runs, TextBlock};
use pdf_save::{save_document, ImportedSources, SaveInput, SaveIntent, SignatureAcknowledgement};

const SPEC: &str = "COMPOSICIÓN: Aluminio lacado con rotura de puente térmico\n\
                    COLOR: Blanco RAL 9016 mate\n\
                    APERTURA: Corredera de dos hojas\n\
                    GARANTÍA: Total en perfilería y herrajes";

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
        .find(|block| block.text.starts_with("COMPOSICI") || block.lines.len() > 1)
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
    try_retype_spec(text).expect("retyped and saved")
}

/// [`retype_spec`], reporting the first refusal on the way as its message
/// instead of panicking.
fn try_retype_spec(text: &str) -> Result<lopdf::Document, String> {
    try_retype_spec_in(&fixture_bytes(), text)
}

/// [`try_retype_spec`] on any saved state of the fixture, not only the
/// committed file.
fn try_retype_spec_in(bytes: &[u8], text: &str) -> Result<lopdf::Document, String> {
    let bytes = bytes.to_vec();
    let (base, security) = pdf_manip::open_document_from_bytes(&bytes, None).unwrap();
    let mut document: Document = pdf_save::document_from_lopdf(&base, security).unwrap();
    retype_pending(&mut document, &base, text)?;
    save_reopened(&document, &base, &bytes)
}

/// Retypes the spec paragraph as it reads with every queued edit applied,
/// recording the batch on `document`.
fn retype_pending(
    document: &mut Document,
    base: &pdf_manip::LopdfDocument,
    text: &str,
) -> Result<(), String> {
    let runs =
        pdf_save::read_pending_page_content_of(document, PageId(0), base, ImportedSources::none())
            .unwrap()
            .text_runs;
    let block = spec_block(&group_text_runs(&runs)).clone();
    let block_runs: Vec<_> = block
        .runs
        .iter()
        .map(|id| runs.iter().find(|run| run.id == *id).unwrap().clone())
        .collect();

    let command = pdf_save::retype_text_block_of(
        document,
        base,
        ImportedSources::none(),
        &block_runs,
        text,
        false,
    )
    .map_err(|error| error.to_string())?
    .expect("a batch");
    assert!(matches!(command, Command::Batch(_)));
    pdf_save::record_content_batch(document, base, ImportedSources::none(), command)
        .map_err(|error| error.to_string())
}

/// Saves `document` over `bytes` and reopens the result.
fn save_reopened(
    document: &Document,
    base: &pdf_manip::LopdfDocument,
    bytes: &[u8],
) -> Result<lopdf::Document, String> {
    let saved = save_document(SaveInput {
        document,
        base,
        original_bytes: Some(bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: ImportedSources::none(),
    })
    .map_err(|error| error.to_string())?;
    Ok(lopdf::Document::load_mem(&saved).expect("the output reloads"))
}

/// The resource names of the fonts `block`'s runs paint in.
fn block_fonts(document: &lopdf::Document, block: &TextBlock) -> Vec<String> {
    let runs = pdf_edit::read_page_content(document, PageId(0))
        .unwrap()
        .text_runs;
    block
        .runs
        .iter()
        .map(|id| {
            runs.iter()
                .find(|run| run.id == *id)
                .unwrap()
                .resource_font_name
                .clone()
        })
        .collect()
}

/// The fixture's one font resource: the reportlab subset of Vera.
const SUBSET_FONT: &str = "F2+0";

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
    let saved = retype_spec("COMPOSICIÓN: PVC\nCOLOR: Gris antracita");

    let blocks = blocks_of(&saved);
    let spec = spec_block(&blocks);
    assert_eq!(spec.text, "COMPOSICIÓN: PVC\nCOLOR: Gris antracita");
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

/// The accented letters are codes only /ToUnicode explains; they must read
/// as themselves, never as U+FFFD.
#[test]
fn accented_text_reads_through_the_fonts_to_unicode_map() {
    let document = lopdf::Document::load_mem(&fixture_bytes()).unwrap();
    let blocks = blocks_of(&document);

    let texts: Vec<&str> = blocks.iter().map(|block| block.text.as_str()).collect();
    assert!(
        spec_block(&blocks).text.starts_with("COMPOSICIÓN"),
        "{texts:?}"
    );
    assert!(texts.contains(&"Descripción"), "{texts:?}");
    assert!(texts.contains(&"Instalación"), "{texts:?}");
    assert!(
        texts.iter().all(|text| !text.contains('\u{FFFD}')),
        "{texts:?}"
    );
}

#[test]
fn an_accented_paragraph_retyped_keeps_its_accents_and_its_own_subset_font() {
    let text = "COMPOSICIÓN: Aluminio térmico\nGARANTÍA: Instalación incluida";
    let saved = retype_spec(text);

    let blocks = blocks_of(&saved);
    let spec = spec_block(&blocks);
    assert_eq!(spec.text, text);
    let fonts = block_fonts(&saved, spec);
    assert!(fonts.iter().all(|font| font == SUBSET_FONT), "{fonts:?}");
}

/// One accented line retyped in place, through the single-run edit every
/// shell's one-line editor uses.
#[test]
fn an_accented_line_retyped_in_place_round_trips_in_the_subset_font() {
    let mut document = lopdf::Document::load_mem(&fixture_bytes()).unwrap();
    let page = pdf_edit::page_object_id(&document, PageId(0)).unwrap();
    let runs = pdf_edit::read_page_content(&document, PageId(0))
        .unwrap()
        .text_runs;
    let line = runs
        .iter()
        .find(|run| run.text.starts_with("GARANTÍA"))
        .expect("the guarantee line")
        .clone();

    pdf_edit::replace_text_run(&mut document, page, &line, "GARANTÍA: Instalación térmica")
        .expect("every character is in the subset");
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    let reopened = lopdf::Document::load_mem(&bytes).unwrap();

    let runs = pdf_edit::read_page_content(&reopened, PageId(0))
        .unwrap()
        .text_runs;
    let line = runs
        .iter()
        .find(|run| run.text.starts_with("GARANTÍA"))
        .expect("the retyped line");
    assert_eq!(line.text, "GARANTÍA: Instalación térmica");
    assert_eq!(line.resource_font_name, SUBSET_FONT);
}

/// The bytes `document` saves to.
fn bytes_of(document: &mut lopdf::Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

/// The page's text runs, as a reopened file reads them.
fn runs_of(document: &lopdf::Document) -> Vec<pdf_document::TextRun> {
    pdf_edit::read_page_content(document, PageId(0))
        .unwrap()
        .text_runs
}

/// The `/BaseFont` of the page font resource `name`.
fn base_font_of(document: &lopdf::Document, name: &str) -> String {
    let page = pdf_edit::page_object_id(document, PageId(0)).unwrap();
    let fonts = document.get_page_fonts(page).unwrap();
    let font = fonts
        .get(name.as_bytes())
        .expect("a page font by that name");
    String::from_utf8_lossy(font.get(b"BaseFont").unwrap().as_name().unwrap()).into_owned()
}

/// The subset carries no `ñ`. Typing one into a line draws that letter in
/// the closest standard-14 font — Helvetica, for a sans subset — and only
/// that letter: the rest of the line stays in the subset, and the line and
/// the paragraph still read as one.
#[test]
fn a_letter_the_subset_lacks_is_drawn_in_helvetica_and_only_that_letter() {
    let mut document = lopdf::Document::load_mem(&fixture_bytes()).unwrap();
    let page = pdf_edit::page_object_id(&document, PageId(0)).unwrap();
    let line = runs_of(&document)
        .into_iter()
        .find(|run| run.text.starts_with("APERTURA"))
        .expect("the opening line");

    pdf_edit::replace_text_run(&mut document, page, &line, "APERTURA: Dos hojas pequeñas")
        .expect("the ñ falls back to a standard font");
    let reopened = lopdf::Document::load_mem(&bytes_of(&mut document)).unwrap();

    let runs = runs_of(&reopened);
    let blocks = group_text_runs(&runs);
    let spec = spec_block(&blocks);
    assert_eq!(spec.lines.len(), 4, "{:?}", spec.text);
    assert_eq!(spec.lines[2].text, "APERTURA: Dos hojas pequeñas");
    let fonts: Vec<(String, String)> = spec.lines[2]
        .runs
        .iter()
        .map(|id| {
            let run = runs.iter().find(|run| run.id == *id).unwrap();
            (run.text.clone(), run.resource_font_name.clone())
        })
        .collect();
    let [(before, first), (letter, fallback), (after, last)] = fonts.as_slice() else {
        panic!("three runs: {fonts:?}");
    };
    assert_eq!(
        (before.as_str(), letter.as_str(), after.as_str()),
        ("APERTURA: Dos hojas peque", "ñ", "as")
    );
    assert_eq!((first.as_str(), last.as_str()), (SUBSET_FONT, SUBSET_FONT));
    assert!(fallback.starts_with("VFb"), "{fallback}");
    assert_eq!(base_font_of(&reopened, fallback), "Helvetica");
}

/// A paragraph holding a fallback letter retypes again after the save — the
/// mixed line is still one line of the paragraph, and the retype writes in
/// the subset again, falling back only where it has to. The second line
/// starts with its fallback letter, so its first run is the Helvetica one:
/// retyping it must go back to the subset, not turn the line Helvetica.
#[test]
fn a_paragraph_with_a_fallback_letter_retypes_again_after_saving() {
    let mut first = retype_spec("COMPOSICIÓN: Aluminio\nÑANDÚ: Moderno");
    let text = "AÑO: 2024\nCOLOR: Blanco";

    let saved = try_retype_spec_in(&bytes_of(&mut first), text).expect("retyped again");

    let runs = runs_of(&saved);
    let blocks = group_text_runs(&runs);
    let spec = blocks
        .iter()
        .find(|block| block.text.starts_with("AÑO"))
        .expect("the paragraph");
    assert_eq!(spec.text, text);
    let fonts = block_fonts(&saved, spec);
    assert_eq!(fonts[0], SUBSET_FONT, "{fonts:?}");
    assert_eq!(
        fonts.iter().filter(|font| font.starts_with("VFb")).count(),
        1,
        "{fonts:?}"
    );
}

/// A paragraph whose queued retype gave it a fallback letter retypes again
/// before any save: its pending text is measured split across the fonts.
#[test]
fn a_paragraph_with_a_queued_fallback_letter_retypes_again_before_saving() {
    let bytes = fixture_bytes();
    let (base, security) = pdf_manip::open_document_from_bytes(&bytes, None).unwrap();
    let mut document: Document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let text = "AÑO: 2024\nCOLOR: Blanco";

    retype_pending(&mut document, &base, "AÑO: 2023\nCOLOR: Blanco").unwrap();
    retype_pending(&mut document, &base, text).expect("retyped again");
    let saved = save_reopened(&document, &base, &bytes).unwrap();

    let blocks = blocks_of(&saved);
    assert!(blocks.iter().any(|block| block.text == text), "{blocks:?}");
}

/// A retype that wraps onto a new line, styled after the paragraph, still
/// falls back for the letters the subset lacks on that new line.
#[test]
fn a_new_wrapped_line_with_a_missing_letter_is_inserted_with_the_fallback() {
    let text = format!("{SPEC}\nDISEÑO: Moderno");
    let saved = retype_spec(&text);

    let blocks = blocks_of(&saved);
    let spec = spec_block(&blocks);
    assert_eq!(spec.text, text);
    assert_eq!(spec.lines.len(), 5);
    let fonts = block_fonts(&saved, spec);
    assert_eq!(
        fonts.iter().filter(|font| font.starts_with("VFb")).count(),
        1,
        "{fonts:?}"
    );
    // The inserted line keeps the paragraph's colour and spacing.
    let content = page_content(&saved);
    assert_eq!(content.matches(SPEC_COLOUR).count(), 2, "{content}");
}

/// A character no standard font has either is still refused, by name.
#[test]
fn a_character_no_standard_font_has_is_refused_by_name() {
    let error =
        try_retype_spec("COMPOSICIÓN: Aluminio\nDISEÑO: 漢").expect_err("WinAnsi has no 漢");

    assert!(error.contains("'漢'"), "{error}");
    assert!(!error.contains('\u{FFFD}'), "{error}");
}
