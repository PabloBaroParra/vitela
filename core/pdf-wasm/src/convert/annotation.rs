//! Annotation geometry and colour as they cross into JavaScript.
//!
//! Annotation geometry is `f64` in the core (unlike text rects), so it
//! crosses as `Float64Array`s: a rect as `x, y, width, height` and ink as
//! `x0, y0, x1, y1, ...`, in the page's unrotated PDF space with a
//! bottom-left origin. A colour crosses as one number, `0xRRGGBB`.

use pdf_document::{Color, Rect};

/// `color` as `0xRRGGBB`.
pub fn pack_rgb(color: Color) -> u32 {
    (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}

/// The colour `0xRRGGBB` names; bits above the low 24 are ignored.
pub fn unpack_rgb(rgb: u32) -> Color {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color { r, g, b }
}

/// `rect` as `[x, y, width, height]`.
pub fn rect_values(rect: Rect) -> Vec<f64> {
    vec![rect.x, rect.y, rect.width, rect.height]
}

/// The rect at (`x`, `y`) of `width` x `height`.
pub fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// `[x0, y0, x1, y1, ...]` as points. A trailing lone value is ignored.
pub fn points_from_flat(flat: &[f64]) -> Vec<(f64, f64)> {
    let (pairs, _partial) = flat.as_chunks::<2>();
    pairs.iter().map(|&[x, y]| (x, y)).collect()
}

/// `points` as `[x0, y0, x1, y1, ...]`.
pub fn flatten_points(points: &[(f64, f64)]) -> Vec<f64> {
    points.iter().flat_map(|&(x, y)| [x, y]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_round_trips_through_one_number() {
        let teal = Color {
            r: 0x12,
            g: 0x80,
            b: 0xfe,
        };
        assert_eq!(pack_rgb(teal), 0x1280fe);
        assert_eq!(unpack_rgb(0x1280fe), teal);
        assert_eq!(unpack_rgb(0xff_1280fe), teal, "the high byte is ignored");
    }

    #[test]
    fn points_flatten_in_pairs_and_a_lone_value_is_dropped() {
        let points = vec![(1.0, 2.0), (3.5, 4.5)];
        let flat = flatten_points(&points);
        assert_eq!(flat, [1.0, 2.0, 3.5, 4.5]);
        assert_eq!(points_from_flat(&flat), points);
        assert_eq!(points_from_flat(&[1.0, 2.0, 9.0]), [(1.0, 2.0)]);
    }

    #[test]
    fn a_rect_crosses_in_declaration_order() {
        assert_eq!(rect_values(rect(1.0, 2.0, 3.0, 4.0)), [1.0, 2.0, 3.0, 4.0]);
    }
}
