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
    write!(
        receipt,
        "width={}\nheight={}\npixels={}\npixels_sha256={:x}\n",
        bitmap.width().map_err(|error| error.to_string())?,
        bitmap.height().map_err(|error| error.to_string())?,
        pixels.len(),
        Sha256::digest(pixels)
    )
    .map_err(|error| error.to_string())?;
    renderer
        .close_document(document)
        .map_err(|error| error.to_string())?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::render_embedded_sample;

    #[test]
    fn renders_the_embedded_sample_to_a_nonempty_receipt() {
        let receipt = render_embedded_sample().expect("the bundled PDFium renders page one");
        assert!(receipt.contains("width="));
        assert!(
            receipt.contains(
                "pixels_sha256=52409ebbba65a9244f7dc0d0aba4ebc529e47caf5c955da10e2a1843e9a804de"
            ),
            "unexpected receipt: {receipt}"
        );
    }

    #[test]
    fn rejects_an_unwritable_receipt_parent() {
        let missing_parent = std::path::Path::new("/definitely-missing-vitela-smoke/receipt");
        assert!(super::write_receipt(missing_parent).is_err());
    }
}
