//! Export pages as PNG/JPEG at selectable DPI (T-037, spec "Export Pages as
//! Images"). Reuses `pdf-render`'s existing actor-backed render pipeline
//! rather than a separate export-specific rasterizer — the same principle
//! `design.md`'s Printing section applies (one renderer, many DPIs).

use image::{DynamicImage, ImageFormat, RgbaImage};

use crate::error::SaveError;

/// Output image format for [`export_page_as_image`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    /// JPEG has no alpha channel — the rendered RGBA bitmap is flattened
    /// onto an implicit opaque background before encoding.
    Jpeg,
}

/// Renders `page_index` of `doc` (already opened via
/// `pdf_render::PdfiumRenderer::open_document`) at `dpi` and encodes it as
/// `format`.
pub fn export_page_as_image(
    renderer: &pdf_render::PdfiumRenderer,
    doc: pdf_render::DocumentHandle,
    page_index: u32,
    dpi: u32,
    format: ExportFormat,
) -> Result<Vec<u8>, SaveError> {
    let bitmap = renderer
        .render_page(
            doc,
            page_index,
            dpi,
            None,
            pdf_render::RenderOptions::default(),
            pdf_render::Priority::Visible,
        )
        .wait()?;

    let width = bitmap.width()?;
    let height = bitmap.height()?;
    let pixels = bitmap.get_pixels()?;

    let image_buffer =
        RgbaImage::from_raw(width, height, pixels).ok_or(SaveError::InvalidSaveRequest(
            "rendered bitmap dimensions do not match its pixel buffer length",
        ))?;

    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    match format {
        ExportFormat::Png => {
            DynamicImage::ImageRgba8(image_buffer).write_to(&mut cursor, ImageFormat::Png)?;
        }
        ExportFormat::Jpeg => {
            let rgb = DynamicImage::ImageRgba8(image_buffer).to_rgb8();
            DynamicImage::ImageRgb8(rgb).write_to(&mut cursor, ImageFormat::Jpeg)?;
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("fixtures")
            .join("encrypted")
            .join("rc4_128_user_and_owner.pdf")
    }

    #[test]
    fn exports_a_page_as_a_decodable_png_at_the_requested_dpi() {
        let renderer = pdf_render::PdfiumRenderer::new();
        let doc = renderer
            .open_document(fixture_path(), Some("user-rc4-pass"))
            .expect("fixture should open");

        let bytes = export_page_as_image(&renderer, doc, 0, 150, ExportFormat::Png)
            .expect("export should succeed");

        assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n", "must be a valid PNG");
        let decoded = image::load_from_memory(&bytes).expect("must decode");
        // Fixture page is 612x792pt (US Letter) at 150 DPI: 612/72*150 = 1275.
        assert_eq!(decoded.width(), 1275);
        assert_eq!(decoded.height(), 1650);
    }

    #[test]
    fn exports_a_page_as_a_decodable_jpeg() {
        let renderer = pdf_render::PdfiumRenderer::new();
        let doc = renderer
            .open_document(fixture_path(), Some("user-rc4-pass"))
            .expect("fixture should open");

        let bytes = export_page_as_image(&renderer, doc, 0, 72, ExportFormat::Jpeg)
            .expect("export should succeed");

        assert_eq!(
            &bytes[0..2],
            &[0xFF, 0xD8],
            "must be a valid JPEG (SOI marker)"
        );
        let decoded = image::load_from_memory(&bytes).expect("must decode");
        assert_eq!(decoded.width(), 612);
        assert_eq!(decoded.height(), 792);
    }
}
