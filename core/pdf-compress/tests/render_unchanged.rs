//! T-192's acceptance criterion, taken literally: *zero visual change —
//! verified by comparing renders, not by inspecting the tree.*
//!
//! Every other test in this crate asks the object graph whether it still
//! looks right. That is exactly the wrong witness for a stage whose job is to
//! **delete objects**: a prune that took one thing too many produces a graph
//! that still parses, still has the right number of pages, and draws a blank
//! square where a font used to be. The only check that cannot be fooled that
//! way is the raster.
//!
//! So this file rasterises every page of every corpus document twice — once
//! from the bytes the user handed in, once from what
//! [`pdf_compress::compress`] gave back — and compares the two bitmaps pixel
//! for pixel. Not "similar", not "within a threshold": identical. `Lossless`
//! changes how bytes are stored and nothing about what they draw, so anything
//! short of equality is a bug rather than a tolerance.
//!
//! The rasteriser is `pdf-render`, the same pdfium the app itself paints
//! with, as a dev-dependency — this crate does not depend on a rasteriser to
//! do its job, only to prove it did it. Same pattern as
//! `pdf-save/tests/preview_raster.rs`, which checks the consequence of a save
//! where the consequence was visible.

use std::path::{Path, PathBuf};

use pdf_compress::{compress, CompressPreset};
use pdf_render::{PdfiumRenderer, Priority, RenderOptions};

/// Rendering DPI. 72 means one bitmap pixel per PDF point — the page's own
/// units, no resampling of the rasteriser's own making between the two
/// renders being compared.
const DPI: u32 = 72;

/// How many pages of a document are checked.
///
/// The whole point is the pages, so the cap is high enough to cover every
/// committed fixture outright. It exists for the generated 200-page file,
/// where rendering every page would turn a correctness test into a benchmark
/// and the two-hundredth page proves nothing the twelfth did not.
const MAX_PAGES: usize = 12;

/// The corpus, as file paths. Encrypted and signed documents are left out on
/// purpose: this crate hands those back byte for byte, so comparing their
/// renders would compare a file with itself and pass no matter what the
/// prune did.
const RENDERABLE: &[(&str, bool)] = &[
    ("assets/sample/vitela-sample.pdf", false),
    (
        "tests/fixtures/content-edit/reportlab_embedded_subset.pdf",
        false,
    ),
    ("tests/fixtures/large/edit_reopen_10pg.pdf", true),
    ("tests/fixtures/large/edit_reopen_50pg.pdf", true),
    ("tests/fixtures/large/perf_200pg.pdf", true),
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the crate sits two directories below the repository root")
}

/// Reads a corpus entry, or `None` when it is a generated fixture that has
/// not been generated in this checkout.
fn read(path: &str, generated: bool) -> Option<Vec<u8>> {
    match std::fs::read(repository_root().join(path)) {
        Ok(bytes) => Some(bytes),
        Err(_) if generated => None,
        Err(err) => panic!("committed fixture {path} is missing: {err}"),
    }
}

/// One rendered page as `(width, height, pixels)`.
fn render(renderer: &PdfiumRenderer, bytes: Vec<u8>, page: u32) -> (u32, u32, Vec<u8>) {
    let document = renderer
        .open_document_from_bytes(bytes, None)
        .expect("the document must open in pdfium");
    let bitmap = renderer
        .render_page(
            document,
            page,
            DPI,
            None,
            RenderOptions::default(),
            Priority::Visible,
        )
        .wait()
        .unwrap_or_else(|err| panic!("page {page} must render: {err:?}"));

    (
        bitmap.width().expect("a rendered page has a width"),
        bitmap.height().expect("a rendered page has a height"),
        bitmap.get_pixels().expect("a rendered page has pixels"),
    )
}

fn page_count(bytes: &[u8]) -> usize {
    lopdf::Document::load_mem(bytes)
        .map(|document| document.get_pages().len())
        .unwrap_or(0)
}

/// The criterion. Every page of every compressible corpus document must paint
/// exactly the same pixels after `Lossless` as before it.
///
/// This is the test the prune is answerable to. Deleting an object that
/// turned out to be reachable after all, or merging two that only looked
/// alike, does not change the page count and does not stop the file parsing —
/// it changes what the page *draws*, and only here.
#[test]
fn lossless_compression_paints_the_same_pixels() {
    let renderer = PdfiumRenderer::new();

    for (path, generated) in RENDERABLE {
        let Some(input) = read(path, *generated) else {
            continue;
        };

        let compressed = compress(&input, CompressPreset::Lossless)
            .unwrap_or_else(|err| panic!("{path} could not be compressed: {err}"));

        // A document that came back untouched has nothing to compare; the
        // corpus guard in `corpus.rs` is what pins that it came back
        // untouched for a stated reason.
        if compressed.bytes() == input.as_slice() {
            continue;
        }

        let pages = page_count(&input).min(MAX_PAGES);
        assert!(pages > 0, "{path} has no pages to compare");

        for page in 0..pages {
            let page = u32::try_from(page).expect("MAX_PAGES fits in a u32");
            let before = render(&renderer, input.clone(), page);
            let after = render(&renderer, compressed.bytes().to_vec(), page);

            assert_eq!(
                (before.0, before.1),
                (after.0, after.1),
                "{path} page {page} changed size when compressed"
            );
            assert!(
                before.2 == after.2,
                "{path} page {page} paints different pixels after Lossless compression \
                 ({} of {} bytes differ)",
                before
                    .2
                    .iter()
                    .zip(&after.2)
                    .filter(|(left, right)| left != right)
                    .count(),
                before.2.len()
            );
        }
    }
}

/// The premise under the test above, and the reason it is worth running: this
/// comparison can actually fail. A renderer that returned the same bitmap for
/// two different documents would let any prune through.
#[test]
fn the_comparison_can_tell_two_documents_apart() {
    let renderer = PdfiumRenderer::new();
    let sample = read("assets/sample/vitela-sample.pdf", false).expect("committed");
    let other = read(
        "tests/fixtures/content-edit/reportlab_embedded_subset.pdf",
        false,
    )
    .expect("committed");

    let left = render(&renderer, sample, 0);
    let right = render(&renderer, other, 0);

    assert!(
        (left.0, left.1) != (right.0, right.1) || left.2 != right.2,
        "two different documents rendered identically; the comparison proves nothing"
    );
}
