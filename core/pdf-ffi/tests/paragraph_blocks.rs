//! Text blocks through the FFI stay one line each: the Windows, Android and
//! Apple shells edit a block as a single line (one-line editor, CID-font
//! retype, per-line move and delete), so a real quote's stacked spec
//! paragraph reads as one block per line. Paragraphs are a core-level
//! grouping (`pdf_edit::group_text_runs`) only Linux uses.

use pdf_ffi::{open_from_bytes, read_page_text_blocks};

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

#[test]
fn a_stacked_paragraph_reads_as_one_block_per_line() {
    let handle = open_from_bytes(fixture(), None).expect("open");

    let blocks = read_page_text_blocks(&handle, 0).expect("blocks");

    for line in SPEC {
        let block = blocks
            .iter()
            .find(|block| block.text == line)
            .unwrap_or_else(|| panic!("{line:?} as its own block"));
        assert_eq!(block.lines.len(), 1);
        assert_eq!(block.leading, None);
    }
    assert!(blocks.iter().all(|block| block.lines.len() == 1));
}
