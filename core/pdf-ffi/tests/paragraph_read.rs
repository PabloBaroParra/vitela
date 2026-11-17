//! Paragraph blocks through the FFI: `read_page_paragraph_blocks` reads a
//! multi-line paragraph as one block, as the Linux shell does, for the
//! shells whose editor is multi-line (Windows). `read_page_text_blocks`
//! keeps reading one block per line (`paragraph_blocks.rs`).
//!
//! A paragraph in a composite (CID) font still reads one block per line:
//! those shells retype a composite run in an inserted standard font, which a
//! paragraph retype refuses (`pdf_edit::retype_paragraph`), so splitting it
//! keeps that line-by-line path working.

use pdf_ffi::{
    open_from_bytes, read_page_paragraph_blocks, retype_text_block, save_to_bytes, undo,
    FfiSaveIntent, FfiSignatureAcknowledgement, FfiTextBlock,
};

fn fixture() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/content-edit/reportlab_quote_paragraph.pdf");
    std::fs::read(path).expect("committed fixture must be readable")
}

const SPEC: [&str; 4] = [
    "COMPOSICIÓN: Aluminio lacado con rotura de puente térmico",
    "COLOR: Blanco RAL 9016 mate",
    "APERTURA: Corredera de dos hojas",
    "GARANTÍA: Total en perfilería y herrajes",
];

fn spec_block(blocks: &[FfiTextBlock]) -> &FfiTextBlock {
    blocks
        .iter()
        .find(|block| block.lines.len() > 1)
        .unwrap_or_else(|| panic!("the spec paragraph in {blocks:#?}"))
}

#[test]
fn a_stacked_paragraph_reads_as_one_block() {
    let handle = open_from_bytes(fixture(), None).expect("open");

    let blocks = read_page_paragraph_blocks(&handle, 0).expect("blocks");

    let block = spec_block(&blocks);
    assert_eq!(block.text, SPEC.join("\n"));
    assert_eq!(block.lines.len(), 4);
    assert!(block.leading.is_some());
}

/// A paragraph retyped again is built from the paragraph as it reads now,
/// pending edits included: once a retype has grown a line, a batch built
/// from the runs the file held is refused. Each such write is its own undo
/// step — a grown line does not fold into the batch that inserted it — which
/// is why the Windows shell writes a paragraph when its editor commits, as
/// Linux does, not on every pause in the typing.
#[test]
fn a_paragraph_retyped_again_from_re_read_blocks_records_every_write() {
    let handle = open_from_bytes(fixture(), None).expect("open");
    let original = SPEC.join("\n");
    let writes = [
        format!("{original}\nUno"),
        format!("{original}\nUno dos"),
        "COLOR: Negro\nUno dos tres".to_string(),
    ];

    for text in &writes {
        let blocks = read_page_paragraph_blocks(&handle, 0).expect("blocks");
        let block = spec_block(&blocks).clone();
        retype_text_block(&handle, block.runs, text.clone(), false)
            .unwrap_or_else(|error| panic!("writing {text:?}: {error:?}"));
    }

    let blocks = read_page_paragraph_blocks(&handle, 0).expect("blocks");
    assert_eq!(spec_block(&blocks).text, writes[2]);
    while undo(&handle) {}
    let blocks = read_page_paragraph_blocks(&handle, 0).expect("blocks");
    assert_eq!(
        spec_block(&blocks).text,
        original,
        "undo restores the paragraph"
    );
}

#[test]
fn a_paragraph_retyped_to_more_lines_saves_and_reopens_as_one_paragraph() {
    let handle = open_from_bytes(fixture(), None).expect("open");
    let text = format!("{}\nUna línea más", SPEC.join("\n"));
    let block = spec_block(&read_page_paragraph_blocks(&handle, 0).unwrap()).clone();

    retype_text_block(&handle, block.runs, text.clone(), false).expect("retyped");
    let saved = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("saved");
    let reopened = open_from_bytes(saved, None).expect("reopened");

    let blocks = read_page_paragraph_blocks(&reopened, 0).expect("blocks");
    assert_eq!(spec_block(&blocks).text, text);
}

/// Plain Helvetica lines 40pt apart at 24pt read as one paragraph; the same
/// lines in a composite font read one block per line.
#[test]
fn a_composite_paragraph_reads_one_block_per_line() {
    let lines = ["Hello world", "Second line", "Third line"];

    let plain = open_from_bytes(fixture_lines(&lines, false), None).expect("open");
    let blocks = read_page_paragraph_blocks(&plain, 0).expect("blocks");
    assert_eq!(blocks.len(), 1, "{blocks:#?}");
    assert_eq!(blocks[0].lines.len(), 3);

    let composite = open_from_bytes(fixture_lines(&lines, true), None).expect("open");
    let blocks = read_page_paragraph_blocks(&composite, 0).expect("blocks");
    assert_eq!(blocks.len(), 3, "{blocks:#?}");
    assert!(blocks
        .iter()
        .all(|block| block.lines.len() == 1 && block.leading.is_none()));
}

fn fixture_lines(lines: &[&str], composite: bool) -> Vec<u8> {
    let mut doc = gen_fixtures::build_multi_line_page_document(lines);
    if composite {
        let font = doc
            .objects
            .values_mut()
            .filter_map(|object| object.as_dict_mut().ok())
            .find(|dictionary| {
                dictionary
                    .get(b"BaseFont")
                    .ok()
                    .and_then(|object| object.as_name().ok())
                    == Some(b"Helvetica")
            })
            .expect("fixture font dictionary");
        font.set("Subtype", "Type0");
        font.set("BaseFont", "AAAAAA+NotoSans");
    }
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("serialize fixture");
    bytes
}
