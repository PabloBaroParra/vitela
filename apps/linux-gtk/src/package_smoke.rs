//! Deterministic, non-GUI package smoke path for the Linux distribution.

use std::fmt::Write as _;
use std::path::Path;

use pdf_render::{PdfiumRenderer, Priority, RenderOptions};
use sha2::{Digest, Sha256};

const SAMPLE_PDF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/sample/vitela-sample.pdf"
));

pub(crate) fn write_receipt(path: &Path) -> Result<(), String> {
    std::fs::write(path, render_embedded_sample()?).map_err(|error| error.to_string())
}

fn render_embedded_sample() -> Result<String, String> {
    let renderer = PdfiumRenderer::new();
    let document = renderer
        .open_document_from_bytes(SAMPLE_PDF.to_vec(), None)
        .map_err(|error| error.to_string())?;
    let bitmap = renderer
        .render_page(
            document,
            0,
            72,
            None,
            RenderOptions::default(),
            Priority::Visible,
        )
        .wait()
        .map_err(|error| error.to_string())?;
    let mut receipt = String::new();
    let pixels = bitmap.get_pixels().map_err(|error| error.to_string())?;
    let width = bitmap.width().map_err(|error| error.to_string())?;
    let height = bitmap.height().map_err(|error| error.to_string())?;
    let stride = bitmap.stride().map_err(|error| error.to_string())?;
    write!(
        receipt,
        "width={}\nheight={}\npixels={}\nink={}\npixels_sha256={:x}\n",
        width,
        height,
        pixels.len(),
        count_non_white_pixels(&pixels, width, height, stride),
        Sha256::digest(&pixels)
    )
    .map_err(|error| error.to_string())?;
    renderer
        .close_document(document)
        .map_err(|error| error.to_string())?;
    Ok(receipt)
}

// Rows are padded to the renderer's stride, so the trailing bytes of each row
// are not page content and are skipped rather than counted as ink.
fn count_non_white_pixels(pixels: &[u8], width: u32, height: u32, stride: u32) -> u64 {
    let mut ink = 0u64;
    for row in 0..height {
        let row_start = (row * stride) as usize;
        for column in 0..width {
            let pixel = row_start + (column * 4) as usize;
            if pixels[pixel] != 0xFF || pixels[pixel + 1] != 0xFF || pixels[pixel + 2] != 0xFF {
                ink += 1;
            }
        }
    }
    ink
}

#[cfg(test)]
mod tests {
    use super::render_embedded_sample;

    #[test]
    fn renders_the_embedded_sample_to_a_nonempty_receipt() {
        // The sample has no embedded FontFile (/BaseFont /Helvetica), so
        // PDFium substitutes a system font via fontconfig at render time —
        // different machines produce different pixel-exact hashes even with
        // the identical PDFium binary. Assert page geometry and non-blank
        // content instead of pinning pixels_sha256.
        let receipt = render_embedded_sample().expect("the bundled PDFium renders page one");
        assert!(
            receipt.contains("width=612"),
            "unexpected receipt: {receipt}"
        );
        assert!(
            receipt.contains("height=792"),
            "unexpected receipt: {receipt}"
        );
        assert!(
            receipt.contains("pixels=1938816"),
            "unexpected receipt: {receipt}"
        );
        assert!(
            receipt.contains("pixels_sha256="),
            "unexpected receipt: {receipt}"
        );

        let ink: u64 = receipt
            .lines()
            .find_map(|line| line.strip_prefix("ink="))
            .and_then(|value| value.parse().ok())
            .expect("receipt has a parseable ink field");
        assert!(ink > 0, "rendered page looks blank: {receipt}");
    }

    #[test]
    fn rejects_an_unwritable_receipt_parent() {
        let missing_parent = std::path::Path::new("/definitely-missing-vitela-smoke/receipt");
        assert!(super::write_receipt(missing_parent).is_err());
    }
}
