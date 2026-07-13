//! Linear-fallback color inversion for dark-mode rendering (T-017).
//!
//! `pdfium-render` 0.9.2's safe `PdfRenderConfig` wrapper does not expose
//! pdfium's forced-color-scheme rendering entry point (`FPDF_RenderPageBitmapWithColorScheme_Start`),
//! only grayscale/reverse-byte-order flags — so this crate implements only
//! the **linear fallback** path described in `spec.md` "Dark-Mode Render
//! Option": a full-page RGBA byte inversion applied post-render, which may
//! also invert embedded images (an explicitly allowed outcome per the spec's
//! "Best-effort image preservation" scenario, since only the fallback path is
//! available). This is a deviation from `design.md`'s stated preference for
//! pdfium's native path when exposed — noted for `sdd-verify`.

/// Inverts RGB channels of an RGBA8 pixel buffer in place, leaving the alpha
/// channel untouched so transparency is preserved. `pixels.len()` need not be
/// a multiple of 4; any trailing partial pixel is left unmodified.
pub fn invert_rgba_in_place(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        chunk[0] = 255 - chunk[0];
        chunk[1] = 255 - chunk[1];
        chunk[2] = 255 - chunk[2];
        // chunk[3] (alpha) intentionally left untouched.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverts_rgb_preserves_alpha() {
        let mut pixels = vec![0u8, 0, 0, 255, 255, 255, 255, 128];
        invert_rgba_in_place(&mut pixels);
        assert_eq!(pixels, vec![255, 255, 255, 255, 0, 0, 0, 128]);
    }

    #[test]
    fn empty_buffer_is_noop() {
        let mut pixels: Vec<u8> = vec![];
        invert_rgba_in_place(&mut pixels);
        assert!(pixels.is_empty());
    }

    #[test]
    fn trailing_partial_pixel_left_untouched() {
        let mut pixels = vec![10u8, 20, 30, 40, 1, 2, 3];
        invert_rgba_in_place(&mut pixels);
        assert_eq!(pixels, vec![245, 235, 225, 40, 1, 2, 3]);
    }

    #[test]
    fn double_inversion_is_identity() {
        let original = vec![10u8, 20, 30, 40, 200, 150, 90, 255];
        let mut pixels = original.clone();
        invert_rgba_in_place(&mut pixels);
        invert_rgba_in_place(&mut pixels);
        assert_eq!(pixels, original);
    }
}
