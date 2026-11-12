//! Where text geometry lands on a page as the viewer draws it.
//!
//! Every rect the engine reports (text runs, search matches, selections) is
//! in the page's **unrotated** PDF space with a bottom-left origin. A viewer
//! draws the page turned by its `/Rotate` and from the top left. Reconciling
//! the two is `pdf_render::selection`'s `place_rect` / `point_to_pdf`, the
//! same transform every native shell uses; this module only applies it to
//! the flattened buffers that cross into JavaScript, at a scale of one: the
//! result is in drawn points, and the web shell multiplies by its zoom.
//!
//! Plain Rust, so the workspace's native `cargo test` covers it; the
//! wasm-bindgen exports that wrap it live in `geometry.rs`.

use pdf_render::{place_rect, point_to_pdf, PagePlacement, PageRotation, TextRect};

use crate::convert::RECT_STRIDE;

/// One page as it is drawn: its size with the turn applied, and the turn.
/// The values `Engine::pageDimensions` reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnPage {
    pub width_pt: f32,
    pub height_pt: f32,
    pub rotation_degrees: u32,
}

impl DrawnPage {
    fn placement(self) -> PagePlacement {
        PagePlacement {
            width_pt: self.width_pt,
            height_pt: self.height_pt,
            rotation: rotation(self.rotation_degrees),
            scale: 1.0,
        }
    }
}

/// The turn for `degrees` clockwise. The engine only ever reports 0, 90, 180
/// or 270; anything else is read modulo a full turn, and a value that is not
/// a quarter turn at all is treated as upright rather than guessed at.
fn rotation(degrees: u32) -> PageRotation {
    match degrees % 360 {
        90 => PageRotation::Clockwise90,
        180 => PageRotation::Clockwise180,
        270 => PageRotation::Clockwise270,
        _ => PageRotation::None,
    }
}

/// Places flattened PDF-space rects (`x, y, width, height`, bottom-left
/// origin, unrotated) on the drawn page: `left, top, width, height` in drawn
/// points, top-left origin. A trailing partial rect is ignored.
pub fn place_flat_rects(flat: &[f32], page: DrawnPage) -> Vec<f32> {
    let placement = page.placement();
    flat.chunks_exact(RECT_STRIDE)
        .flat_map(|chunk| {
            let rect = TextRect {
                x_pt: chunk[0],
                y_pt: chunk[1],
                width_pt: chunk[2],
                height_pt: chunk[3],
            };
            let placed = place_rect(rect, placement);
            [placed.left, placed.top, placed.width, placed.height].map(|value| value as f32)
        })
        .collect()
}

/// A point on the drawn page (drawn points, top-left origin) back in the
/// unrotated PDF space the page's characters are hit-tested in.
pub fn drawn_point_to_pdf(x: f64, y: f64, page: DrawnPage) -> (f32, f32) {
    point_to_pdf(x, y, page.placement())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: DrawnPage = DrawnPage {
        width_pt: 612.0,
        height_pt: 792.0,
        rotation_degrees: 0,
    };

    /// A Letter page turned a quarter: drawn landscape.
    const TURNED: DrawnPage = DrawnPage {
        width_pt: 792.0,
        height_pt: 612.0,
        rotation_degrees: 90,
    };

    #[test]
    fn an_upright_page_only_flips_the_y_axis() {
        let placed = place_flat_rects(&[72.0, 700.0, 50.0, 20.0], LETTER);
        assert_eq!(placed, vec![72.0, 792.0 - 720.0, 50.0, 20.0]);
    }

    #[test]
    fn a_quarter_turn_moves_the_bottom_edge_to_the_left() {
        // Clockwise 90: the unrotated bottom-left corner ends up top-left,
        // so PDF y becomes the drawn left and PDF x the drawn top.
        let placed = place_flat_rects(&[72.0, 700.0, 50.0, 20.0], TURNED);
        assert_eq!(placed, vec![700.0, 72.0, 20.0, 50.0]);
    }

    #[test]
    fn every_rect_is_placed_and_a_partial_one_is_dropped() {
        let placed = place_flat_rects(&[0.0, 0.0, 1.0, 1.0, 10.0, 10.0, 2.0, 2.0, 5.0], LETTER);
        assert_eq!(placed.len(), 2 * RECT_STRIDE);
    }

    #[test]
    fn a_drawn_point_inverts_the_placement_at_every_turn() {
        for degrees in [0, 90, 180, 270] {
            let quarter = degrees % 180 == 90;
            let page = DrawnPage {
                width_pt: if quarter { 792.0 } else { 612.0 },
                height_pt: if quarter { 612.0 } else { 792.0 },
                rotation_degrees: degrees,
            };
            // A zero-size rect at a PDF point lands where that point is drawn.
            let placed = place_flat_rects(&[100.0, 200.0, 0.0, 0.0], page);
            let back = drawn_point_to_pdf(f64::from(placed[0]), f64::from(placed[1]), page);
            assert_eq!(back, (100.0, 200.0), "at {degrees} degrees");
        }
    }

    #[test]
    fn an_unknown_turn_is_drawn_upright() {
        let odd = DrawnPage {
            rotation_degrees: 45,
            ..LETTER
        };
        assert_eq!(
            place_flat_rects(&[72.0, 700.0, 50.0, 20.0], odd),
            place_flat_rects(&[72.0, 700.0, 50.0, 20.0], LETTER),
        );
        let full = DrawnPage {
            rotation_degrees: 450,
            ..TURNED
        };
        assert_eq!(
            place_flat_rects(&[72.0, 700.0, 50.0, 20.0], full),
            place_flat_rects(&[72.0, 700.0, 50.0, 20.0], TURNED),
        );
    }
}
