//! Where a stamp lands when the *app* places it rather than the user tracing
//! it — a drag-and-dropped image, a clipboard paste (T-030's placement half).
//!
//! A traced rect is a statement of intent and is honoured as drawn; this
//! module is only for the case where there is no traced rect and the app has
//! to choose one. It lives in the core rather than in each shell because every
//! shell has to choose the *same* rect: the Linux GTK shell calls
//! [`stamp_placement`] directly, and WinUI/Compose reach it through
//! `pdf_ffi::stamp_placement`.

use pdf_document::Rect;
use pdf_edit::EditError;

use crate::error::AnnotateError;

/// An image the core's bounded decoder (`pdf_edit::decode_image`) refused,
/// as this crate says it.
pub(crate) fn invalid_image(error: EditError) -> AnnotateError {
    match error {
        EditError::InvalidImage(message) => AnnotateError::InvalidImage(message),
        other => AnnotateError::InvalidImage(other.to_string()),
    }
}

/// Longest side, in PDF points, of a stamp the app places for the user.
///
/// A square box rather than a wide one: the source is a logo, a screenshot or
/// a signature as often as it is a banner, and only the image's own
/// proportions can say which. 144pt is two inches — large enough to read at
/// 100% zoom, small enough to leave the page legible underneath.
pub const DEFAULT_STAMP_MAX_SIDE_PT: f64 = 144.0;

/// Rect for a stamp the app places at `anchor`, in PDF points.
///
/// The image keeps its own proportions, scaled so its longest side is
/// `max_side`, anchored by its top-left corner — which in PDF space (y grows
/// upward) puts the rect's origin at `anchor.1 - height`.
///
/// Reads only `image_bytes`' header (`pdf_edit::probe_image`), under the
/// same size limits [`crate::stamp_from_image_bytes`] decodes within: the
/// two are called with the same bytes and must agree about what those
/// bytes are, and one whose samples are broken is refused by the decode that
/// builds it.
pub fn stamp_placement(
    image_bytes: &[u8],
    anchor: (f64, f64),
    max_side: f64,
) -> Result<Rect, AnnotateError> {
    let header = pdf_edit::probe_image(image_bytes).map_err(invalid_image)?;
    let (pixel_width, pixel_height) = (header.width, header.height);
    if pixel_width == 0 || pixel_height == 0 {
        return Err(AnnotateError::InvalidImage(
            "image has no pixels to scale".to_string(),
        ));
    }

    let longest = f64::from(pixel_width.max(pixel_height));
    let scale = max_side / longest;
    let width = f64::from(pixel_width) * scale;
    let height = f64::from(pixel_height) * scale;

    Ok(Rect {
        x: anchor.0,
        y: anchor.1 - height,
        width,
        height,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        use image::{DynamicImage, ImageFormat, RgbImage};
        use std::io::Cursor;

        let dynamic = DynamicImage::ImageRgb8(RgbImage::from_pixel(
            width,
            height,
            image::Rgb([10, 20, 30]),
        ));
        let mut buf = Cursor::new(Vec::new());
        dynamic
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encoding a test png should succeed");
        buf.into_inner()
    }

    #[test]
    fn a_square_image_gets_a_square_rect() {
        let rect = stamp_placement(&png(64, 64), (0.0, 0.0), 144.0).expect("valid png");

        assert_eq!((rect.width, rect.height), (144.0, 144.0));
    }

    #[test]
    fn a_wide_image_is_bounded_by_its_width() {
        let rect = stamp_placement(&png(400, 100), (0.0, 0.0), 144.0).expect("valid png");

        assert_eq!((rect.width, rect.height), (144.0, 36.0));
    }

    #[test]
    fn a_tall_image_is_bounded_by_its_height() {
        let rect = stamp_placement(&png(100, 400), (0.0, 0.0), 144.0).expect("valid png");

        assert_eq!((rect.width, rect.height), (36.0, 144.0));
    }

    #[test]
    fn the_source_aspect_ratio_survives_placement() {
        let rect = stamp_placement(&png(300, 200), (0.0, 0.0), 144.0).expect("valid png");

        assert!((rect.width / rect.height - 1.5).abs() < 1e-9);
    }

    #[test]
    fn a_small_image_is_scaled_up_to_the_same_box() {
        // The box is a size, not a ceiling: a 16px icon and a 4000px photo
        // both land at a size the reader can see and grab.
        let rect = stamp_placement(&png(16, 16), (0.0, 0.0), 144.0).expect("valid png");

        assert_eq!((rect.width, rect.height), (144.0, 144.0));
    }

    #[test]
    fn the_rect_hangs_below_its_anchor_point() {
        let rect = stamp_placement(&png(400, 100), (200.0, 500.0), 144.0).expect("valid png");

        assert_eq!((rect.x, rect.y), (200.0, 500.0 - 36.0));
    }

    #[test]
    fn garbage_bytes_are_rejected_the_way_the_builder_rejects_them() {
        let result = stamp_placement(b"not an image", (0.0, 0.0), 144.0);

        assert!(matches!(result, Err(AnnotateError::InvalidImage(_))));
    }

    /// A complete 69-byte PNG whose header declares 11000 × 11000 RGBA
    /// pixels (about 484 MB once decoded).
    pub(crate) const OVERSIZED_PNG: [u8; 69] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x2a, 0xf8, 0x00, 0x00, 0x2a, 0xf8, 0x08, 0x06, 0x00, 0x00, 0x00, 0xce,
        0x2f, 0x9a, 0x8a, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0xa0, 0x0c, 0x00, 0x00, 0x00, 0x40, 0x00, 0x01, 0xb7, 0x34, 0x7c, 0xef, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    pub(crate) fn too_large<T: std::fmt::Debug>(result: Result<T, AnnotateError>) -> bool {
        matches!(result, Err(AnnotateError::InvalidImage(message)) if message.contains("too large"))
    }

    #[test]
    fn an_image_declaring_more_pixels_than_the_core_decodes_is_refused() {
        assert!(too_large(stamp_placement(
            &OVERSIZED_PNG,
            (0.0, 0.0),
            144.0
        )));
    }
}
