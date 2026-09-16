//! The compression corpus (Batch 24, T-197) — `tests/fixtures/compress/`.
//!
//! `core/pdf-compress/tests/` already had the harness: `corpus.rs` runs the
//! guardians, `measure.rs` prints the size table, `render_unchanged.rs`
//! compares rasters, and all three read one list. What it did not have was
//! anything to point them at. The repository's real PDFs are a sample
//! document, some producer output, four protected files and a perf fixture —
//! and T-193's measurement of them found that **not one image in the whole
//! repository reaches even the lowest preset's 96 effective dpi**. A
//! resampler run over that corpus finds no candidates, and its tests pass
//! without exercising anything.
//!
//! So this module builds the documents the repository was missing, one per
//! thing the compressor has to get right:
//!
//! | fixture | what it is there to catch |
//! |---|---|
//! | `scan_200dpi.pdf` | a lossy image far above every preset's ceiling |
//! | `reused_image_two_scales.pdf` | the *largest placement governs* rule |
//! | `transparency_smask.pdf` | a soft mask that must follow its parent down |
//! | `vector_only.pdf` | a page the image stage must not touch at all |
//! | `already_packed.pdf` | a file with no slack left, which must not grow |
//!
//! **Committed, not generated on demand.** The perf fixture under
//! `tests/fixtures/large/` is `.gitignore`d because it is 50MB, and the
//! compression corpus reads those rows as optional — a checkout that has not
//! run the generator simply skips them. That is the wrong trade here: CI
//! never runs `gen-fixtures`, so a fixture kept out of the repository is a
//! guardian that never fires in CI, which is precisely what T-197 exists to
//! create. These are therefore sized to be committed — every raster is a
//! smooth synthetic one ([`raster`]), which is both what a real page mostly
//! is and what keeps the whole corpus well under a megabyte.

mod documents;
mod images;
mod raster;

use std::io;
use std::path::{Path, PathBuf};

use lopdf::Document;

/// One entry of the committed compression corpus.
#[derive(Debug, Clone, Copy)]
pub struct CompressFixture {
    pub file_name: &'static str,
    /// What this row is answerable for — the same sentence that appears
    /// beside it in `core/pdf-compress/tests/common/mod.rs`.
    pub what: &'static str,
}

/// Every fixture this module writes, in the order it writes them.
pub const CORPUS: &[CompressFixture] = &[
    CompressFixture {
        file_name: "scan_200dpi.pdf",
        what: "a scan: one lossy image at 200 effective dpi",
    },
    CompressFixture {
        file_name: "reused_image_two_scales.pdf",
        what: "one image XObject painted large and small",
    },
    CompressFixture {
        file_name: "transparency_smask.pdf",
        what: "real transparency: an image with a soft mask",
    },
    CompressFixture {
        file_name: "vector_only.pdf",
        what: "pure vector: no image for the image stage to find",
    },
    CompressFixture {
        file_name: "already_packed.pdf",
        what: "object streams, xref stream, every stream flated",
    },
];

/// Builds the document behind `file_name`.
///
/// Public so the generator's own tests can assert on the graph without going
/// through a temporary directory, and so a failure there names the builder
/// rather than a path.
pub fn build(file_name: &str) -> lopdf::Result<Document> {
    match file_name {
        "scan_200dpi.pdf" => documents::scan(),
        "reused_image_two_scales.pdf" => documents::reused_image(),
        "transparency_smask.pdf" => documents::transparency(),
        "vector_only.pdf" => documents::vector_only(),
        "already_packed.pdf" => documents::already_packed(),
        other => panic!("{other} is not part of the compression corpus"),
    }
}

/// Writes the whole corpus into `out_dir`, creating it if needed. Returns the
/// paths written, in [`CORPUS`] order.
pub fn generate_compress_corpus(out_dir: &Path) -> io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out_dir)?;

    let mut written = Vec::with_capacity(CORPUS.len());
    for fixture in CORPUS {
        let document = build(fixture.file_name).map_err(|e| io::Error::other(e.to_string()))?;
        let out_path = out_dir.join(fixture.file_name);
        let bytes = serialise(document, fixture.file_name).map_err(io::Error::other)?;
        std::fs::write(&out_path, bytes)?;
        written.push(out_path);
    }

    Ok(written)
}

/// The bytes of one corpus fixture.
///
/// `already_packed.pdf` is the one row whose *write format* is the point of
/// the fixture, so it is the one row that does not take the default save.
/// Everything else is written the way today's `pdf-save` writes a file —
/// loose objects, classic cross-reference table — which is the shape the
/// compressor is actually handed in the field.
pub fn serialise(mut document: Document, file_name: &str) -> lopdf::Result<Vec<u8>> {
    let mut bytes = Vec::new();

    if file_name == "already_packed.pdf" {
        document.save_with_options(
            &mut bytes,
            lopdf::SaveOptions {
                use_object_streams: true,
                use_xref_streams: true,
                ..lopdf::SaveOptions::default()
            },
        )?;
    } else {
        document.save_to(&mut bytes)?;
    }

    Ok(bytes)
}
