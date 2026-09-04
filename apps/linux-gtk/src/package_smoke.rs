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
    let (width, height, pixels) = render_embedded_sample_pixels()?;
    let mut receipt = String::new();
    write!(
        receipt,
        "width={}\nheight={}\npixels={}\npixels_sha256={:x}\n",
        width,
        height,
        pixels.len(),
        Sha256::digest(&pixels)
    )
    .map_err(|error| error.to_string())?;
    Ok(receipt)
}

fn render_embedded_sample_pixels() -> Result<(u32, u32, Vec<u8>), String> {
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
    let width = bitmap.width().map_err(|error| error.to_string())?;
    let height = bitmap.height().map_err(|error| error.to_string())?;
    let pixels = bitmap.get_pixels().map_err(|error| error.to_string())?;
    renderer
        .close_document(document)
        .map_err(|error| error.to_string())?;
    Ok((width, height, pixels))
}

#[cfg(test)]
mod tests {
    use super::render_embedded_sample_pixels;

    #[test]
    fn renders_the_embedded_sample_to_a_nonempty_receipt() {
        // vitela-sample.pdf's text uses the unembedded base font `Helvetica`,
        // so PDFium substitutes a system font at render time — the exact
        // rasterized pixels (and thus their hash) vary across machines with
        // different font packages installed, even for the same PDFium
        // version. Assert page geometry and that PDFium actually painted
        // something instead of pinning a pixel-exact hash.
        let (width, height, pixels) =
            render_embedded_sample_pixels().expect("the bundled PDFium renders page one");
        assert_eq!(width, 612, "unexpected rendered width");
        assert_eq!(height, 792, "unexpected rendered height");
        assert_eq!(pixels.len(), 1_938_816, "unexpected pixel buffer size");
        assert!(
            pixels.iter().any(|&byte| byte != pixels[0]),
            "rendered page looks blank/uniform"
        );
    }

    #[test]
    fn rejects_an_unwritable_receipt_parent() {
        let missing_parent = std::path::Path::new("/definitely-missing-vitela-smoke/receipt");
        assert!(super::write_receipt(missing_parent).is_err());
    }
}
