//! Shapes core values into the flat buffers JavaScript reads cheaply.
//!
//! Geometry crosses as one `Float32Array` per value rather than an array of
//! objects: a page can hold thousands of characters, and one typed array is
//! one copy instead of thousands of JS allocations.

use pdf_render::{Bitmap, TextRect};

/// Floats per rect in a flattened buffer: `x, y, width, height` in PDF points
/// with a bottom-left origin, the order `TextRect` declares them.
pub const RECT_STRIDE: usize = 4;

/// `rects` as `[x0, y0, w0, h0, x1, y1, w1, h1, ...]`.
pub fn flatten_rects(rects: &[TextRect]) -> Vec<f32> {
    rects
        .iter()
        .flat_map(|rect| [rect.x_pt, rect.y_pt, rect.width_pt, rect.height_pt])
        .collect()
}

/// The bitmap's pixels with no row padding (`width * height * 4` bytes), the
/// layout `ImageData` requires. pdf-render already hands back tight rows;
/// a padded bitmap is repacked rather than trusted.
pub fn tight_rgba(bitmap: Bitmap) -> Vec<u8> {
    let row = bitmap.width as usize * 4;
    let stride = bitmap.stride as usize;
    if stride == row {
        return bitmap.pixels;
    }
    bitmap
        .pixels
        .chunks(stride)
        .take(bitmap.height as usize)
        .flat_map(|line| &line[..row])
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_flatten_in_declaration_order() {
        let rects = [
            TextRect {
                x_pt: 1.0,
                y_pt: 2.0,
                width_pt: 3.0,
                height_pt: 4.0,
            },
            TextRect {
                x_pt: 5.0,
                y_pt: 6.0,
                width_pt: 7.0,
                height_pt: 8.0,
            },
        ];
        assert_eq!(
            flatten_rects(&rects),
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
        );
        assert_eq!(flatten_rects(&rects).len(), rects.len() * RECT_STRIDE);
    }

    #[test]
    fn tight_rows_pass_through_untouched() {
        let pixels: Vec<u8> = (0..16).collect();
        let bitmap = Bitmap {
            width: 2,
            height: 2,
            stride: 8,
            pixels: pixels.clone(),
        };
        assert_eq!(tight_rgba(bitmap), pixels);
    }

    #[test]
    fn padded_rows_are_repacked() {
        // 1x2 RGBA with 4 bytes of padding per row.
        let bitmap = Bitmap {
            width: 1,
            height: 2,
            stride: 8,
            pixels: vec![1, 2, 3, 4, 0, 0, 0, 0, 5, 6, 7, 8, 0, 0, 0, 0],
        };
        assert_eq!(tight_rgba(bitmap), vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }
}
