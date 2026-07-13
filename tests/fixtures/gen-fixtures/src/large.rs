//! Large-fixture generator for the perf harness (Batch 3, T-021, `pdf-render`).
//!
//! Extends `gen-fixtures` additively — see this crate's existing
//! `encrypted/` corpus for the sibling small-fixture pattern. This fixture
//! is deliberately **not** committed (see the root `.gitignore`'s
//! `tests/fixtures/large/` entry): at ~50MB it would bloat the repository,
//! and `spec.md`'s "Large-File Performance" requirement only needs it to
//! exist locally when the perf test actually runs.

use std::io;
use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::xref::XrefType;
use lopdf::{dictionary, Document, Object, Stream};

/// Shape of the perf-harness fixture: page count and the side length (in
/// pixels) of the square RGB image XObject embedded on every page.
#[derive(Debug, Clone, Copy)]
pub struct LargeFixtureSpec {
    pub pages: u32,
    pub image_side_px: u32,
}

/// `spec.md` "Large-File Performance": "a 50MB, 200-page PDF (FlateDecode
/// streams, linearized or not) ... on reference hardware". `image_side_px`
/// was tuned empirically against this generator (gradient + per-pixel noise,
/// see [`page_image_rgb`]) so the generated, FlateDecode-compressed fixture
/// lands close to 50MB in total — see `tests/fixtures/gen-fixtures/tests/large_fixture.rs`
/// for the assertion, and the Batch 3 apply-progress record for the actual
/// measured size.
pub const PERF_LARGE_SPEC: LargeFixtureSpec = LargeFixtureSpec {
    pages: 200,
    image_side_px: 316,
};

/// Deterministic xorshift32 PRNG — avoids pulling in an external RNG crate
/// just to generate test-fixture noise.
fn xorshift32(seed: u32) -> impl FnMut() -> u32 {
    let mut state = if seed == 0 { 1 } else { seed };
    move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    }
}

/// Builds one page's raw RGB8 pixel buffer: a smooth diagonal gradient (very
/// compressible — long runs of near-identical bytes both along rows and
/// between adjacent rows) with small per-pixel pseudo-random noise added (so
/// the buffer isn't *perfectly* uniform, requiring pdfium to actually decode
/// real varying pixel data rather than a degenerate all-one-color image).
/// This combination reliably compresses under `FlateDecode` — matching
/// `spec.md`'s fixture description — while still landing at a realistic,
/// non-trivial per-page byte count once the page count and image size are
/// tuned (see [`PERF_LARGE_SPEC`]).
fn page_image_rgb(side_px: u32, seed: u32) -> Vec<u8> {
    let mut rng = xorshift32(seed);
    let mut pixels = Vec::with_capacity((side_px as usize) * (side_px as usize) * 3);
    let denom = 2 * side_px.max(1);

    for y in 0..side_px {
        for x in 0..side_px {
            let base_r = ((x * 255) / side_px.max(1)) as u8;
            let base_g = ((y * 255) / side_px.max(1)) as u8;
            let base_b = (((x + y) * 255) / denom) as u8;
            let noise = (rng() & 0x1F) as u8; // small amplitude: 0..=31
            pixels.push(base_r.wrapping_add(noise));
            pixels.push(base_g.wrapping_add(noise));
            pixels.push(base_b.wrapping_add(noise));
        }
    }

    pixels
}

/// Builds the full multi-page document in memory (no I/O). Each page is a
/// single full-bleed image XObject — no text — since the perf target is
/// raster decode/rasterization throughput, not layout.
pub fn build_large_document(spec: &LargeFixtureSpec) -> lopdf::Result<Document> {
    let mut doc = Document::with_version("1.5");
    // Classic xref table — see `build_base_document`'s comment in `lib.rs`
    // for why: keeps the fixture simple to inspect/debug; not required for
    // correctness here (this fixture isn't encrypted), but keeps generator
    // behavior consistent across this crate's fixtures.
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

    let pages_id = doc.new_object_id();
    let mut kids = Vec::with_capacity(spec.pages as usize);

    let (page_width_pt, page_height_pt) = (612.0_f64, 792.0_f64); // US Letter

    for page_num in 0..spec.pages {
        let seed = page_num.wrapping_mul(2_654_435_761).wrapping_add(1);
        let image_bytes = page_image_rgb(spec.image_side_px, seed);

        let mut image_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(spec.image_side_px),
                "Height" => i64::from(spec.image_side_px),
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            image_bytes,
        );
        image_stream.compress()?;
        let image_id = doc.add_object(image_stream);

        let resources_id = doc.add_object(dictionary! {
            "XObject" => dictionary! { "Im0" => image_id },
        });

        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        page_width_pt.into(),
                        0.into(),
                        0.into(),
                        page_height_pt.into(),
                        0.into(),
                        0.into(),
                    ],
                ),
                Operation::new("Do", vec!["Im0".into()]),
                Operation::new("Q", vec![]),
            ],
        };
        let mut content_stream = Stream::new(dictionary! {}, content.encode()?);
        content_stream.compress()?;
        let content_id = doc.add_object(content_stream);

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), page_width_pt.into(), page_height_pt.into()],
        });
        kids.push(page_id.into());
    }

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => kids,
        "Count" => i64::from(spec.pages),
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    Ok(doc)
}

/// Generates the perf-harness fixture to `out_path`, creating parent
/// directories as needed. Not idempotent-cached: callers that only need the
/// fixture if it's missing should check `out_path.exists()` first (the perf
/// test does this to avoid regenerating a ~50MB file on every run).
pub fn generate_large_fixture(out_path: &Path, spec: &LargeFixtureSpec) -> io::Result<PathBuf> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut doc = build_large_document(spec).map_err(|e| io::Error::other(e.to_string()))?;
    doc.save(out_path)
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(out_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_document_with_expected_page_count() {
        let spec = LargeFixtureSpec {
            pages: 3,
            image_side_px: 16,
        };
        let doc = build_large_document(&spec).unwrap();
        assert_eq!(doc.get_pages().len(), 3);
    }

    #[test]
    fn page_image_bytes_are_deterministic_for_same_seed() {
        let a = page_image_rgb(32, 42);
        let b = page_image_rgb(32, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn page_image_bytes_differ_across_seeds() {
        let a = page_image_rgb(32, 1);
        let b = page_image_rgb(32, 2);
        assert_ne!(a, b);
    }
}
