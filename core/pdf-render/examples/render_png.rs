//! Demo: renders a real PDF's first few pages to PNG files using only
//! `pdf-render`'s public API (`PdfiumRenderer`), proving the render engine
//! works end to end against an arbitrary user document.
//!
//! Usage:
//!
//! ```sh
//! cargo run -p pdf-render --example render_png -- path/to/document.pdf
//! ```
//!
//! Renders page 1, and up to the first 3 pages if the document has more,
//! at `DPI` and writes each to `target/demo/page-{n}.png`. Per-page render
//! time (actor round trip, including rasterization) and output paths are
//! printed to stdout.
//!
//! ## Why probing for page count instead of querying it
//!
//! `PdfiumRenderer`'s public API has no `page_count`/`num_pages` accessor
//! (see `src/renderer.rs`, `src/state.rs`) — page count is an internal
//! `pdfium_render::PdfDocument` detail the crate doesn't surface, and this
//! demo intentionally sticks to the crate's real public surface rather than
//! reaching around it with a raw `pdfium-render` call. Instead, this probes
//! by rendering pages in order and treating
//! `RenderError::PageIndexOutOfBounds` as "no more pages" rather than a
//! failure — a legitimate use of the documented error variant
//! (`src/error.rs`), not a workaround.
//!
//! ## Pixel format note
//!
//! `PdfiumRenderer::render_page`'s `BitmapHandle::get_pixels()` already
//! returns RGBA8 bytes — `src/renderer.rs`'s `render_page_job` converts from
//! pdfium's native BGRA via `PdfBitmap::as_rgba_bytes()` before the pixels
//! ever reach the bitmap registry. No further channel-swap is needed here.

use std::path::{Path, PathBuf};
use std::time::Instant;

use pdf_render::{PdfiumRenderer, Priority, RenderError, RenderOptions};

/// Render resolution. 150 DPI is a reasonable "on-screen preview" quality
/// that keeps PNG files small while remaining clearly legible.
const DPI: u32 = 150;

/// Render at most this many pages, even if the document has more.
const MAX_PAGES: u32 = 3;

fn main() {
    let pdf_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: render_png <path-to-pdf>");
        std::process::exit(2);
    });

    let output_dir = demo_output_dir();
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|e| panic!("failed to create output directory {output_dir:?}: {e}"));

    let renderer = PdfiumRenderer::new();

    let doc = renderer
        .open_document(&pdf_path, None)
        .unwrap_or_else(|e| panic!("failed to open PDF at {pdf_path:?}: {e}"));

    println!("Opened {pdf_path}");

    let mut pages_rendered = 0u32;
    for page_index in 0..MAX_PAGES {
        let start = Instant::now();
        let result = renderer
            .render_page(
                doc,
                page_index,
                DPI,
                None,
                RenderOptions::default(),
                Priority::Visible,
            )
            .wait();
        let elapsed = start.elapsed();

        let bitmap = match result {
            Ok(bitmap) => bitmap,
            Err(RenderError::PageIndexOutOfBounds(_)) => {
                // Reached the end of the document — not an error.
                break;
            }
            Err(e) => panic!("failed to render page {page_index}: {e}"),
        };

        let width = bitmap
            .width()
            .unwrap_or_else(|e| panic!("failed to read width for page {page_index}: {e}"));
        let height = bitmap
            .height()
            .unwrap_or_else(|e| panic!("failed to read height for page {page_index}: {e}"));
        let pixels = bitmap
            .get_pixels()
            .unwrap_or_else(|e| panic!("failed to read pixels for page {page_index}: {e}"));

        let output_path = output_dir.join(format!("page-{}.png", page_index + 1));
        write_png(&output_path, width, height, &pixels);

        println!(
            "page {}: {}x{} px, rendered in {:.2} ms -> {}",
            page_index + 1,
            width,
            height,
            elapsed.as_secs_f64() * 1000.0,
            output_path.display(),
        );

        pages_rendered += 1;
    }

    if pages_rendered == 0 {
        panic!("document has no renderable pages");
    }

    println!(
        "Done: {pages_rendered} page(s) rendered to {}",
        output_dir.display()
    );
}

/// `target/demo`, resolved relative to this crate's manifest directory so
/// the example writes to the same place regardless of the working directory
/// `cargo run` was invoked from.
fn demo_output_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent() // core/
        .and_then(Path::parent) // workspace root
        .unwrap_or(&manifest_dir)
        .join("target")
        .join("demo")
}

/// Encodes RGBA8 pixels as a PNG file.
fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    let file =
        std::fs::File::create(path).unwrap_or_else(|e| panic!("failed to create {path:?}: {e}"));
    let writer = std::io::BufWriter::new(file);

    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder
        .write_header()
        .unwrap_or_else(|e| panic!("failed to write PNG header for {path:?}: {e}"));
    writer
        .write_image_data(rgba)
        .unwrap_or_else(|e| panic!("failed to write PNG data for {path:?}: {e}"));
}
