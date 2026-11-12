//! The quiet box every editable item wears while content-edit mode is on,
//! the way Acrobat's "Edit PDF" marks what can be edited: a thin grey dashed
//! line, so a page full of items still reads as a page and not as a grid.
//!
//! Only the *resting* outline lives here. A selected or dragged item, the
//! multi-selection box, the hover outline ([`super::hover`]) and the resize
//! handles keep their own solid, coloured strokes, which is what lets them
//! stand out above these dashes.
//!
//! The context the page overlay draws into is already in display pixels
//! (`place_rect` scales a PDF rect to the size the page is shown at), so the
//! width and the dash pattern below are screen distances and stay the same
//! at any zoom with no division by the page scale.
//!
//! The page itself is always painted white, whatever the desktop theme, so
//! one grey serves both the light and the dark palette.

use gtk::cairo;
use pdf_render::PlacedRect;

/// Mid grey: visible on a white page without competing with its content.
const RESTING_RGB: (f64, f64, f64) = (0.45, 0.45, 0.45);
/// One display pixel.
const RESTING_WIDTH_PX: f64 = 1.0;
/// Three pixels on, two off — the short dash Acrobat draws.
const RESTING_DASH_PX: [f64; 2] = [3.0, 2.0];

/// Strokes the resting outline round `placed`. The context's own source,
/// width and dash are left as they were.
pub(crate) fn stroke_resting(context: &cairo::Context, placed: PlacedRect) {
    let _ = context.save();
    let (red, green, blue) = RESTING_RGB;
    context.set_source_rgb(red, green, blue);
    context.set_line_width(RESTING_WIDTH_PX);
    context.set_dash(&RESTING_DASH_PX, 0.0);
    context.rectangle(placed.left, placed.top, placed.width, placed.height);
    let _ = context.stroke();
    let _ = context.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alpha of every pixel in the surface's top row.
    fn top_row_alpha(surface: &mut cairo::ImageSurface) -> Vec<u8> {
        let width = surface.width() as usize;
        let data = surface.data().expect("surface data");
        // ARGB32 is one native-endian u32 per pixel, alpha in the top byte.
        let (pixels, _) = data.as_chunks::<4>();
        pixels
            .iter()
            .take(width)
            .map(|pixel| (u32::from_ne_bytes(*pixel) >> 24) as u8)
            .collect()
    }

    #[test]
    fn the_resting_outline_is_three_pixels_on_two_off_and_restores_the_context() {
        let mut surface =
            cairo::ImageSurface::create(cairo::Format::ARgb32, 24, 6).expect("surface");
        {
            let context = cairo::Context::new(&surface).expect("context");
            context.set_line_width(7.0);
            // The top edge's centre on y = 0.5, so the 1px line fills row 0
            // exactly; the stroke starts at x = 0 and runs right.
            stroke_resting(
                &context,
                PlacedRect {
                    left: 0.0,
                    top: 0.5,
                    width: 24.0,
                    height: 4.0,
                },
            );
            assert_eq!(context.line_width(), 7.0);
            assert!(context.dash().0.is_empty());
        }
        let row = top_row_alpha(&mut surface);
        let inked: Vec<bool> = row.iter().map(|alpha| *alpha > 0).collect();
        // A 5px period starting at the corner, whatever the page size or zoom
        // that placed the rect.
        let expected: Vec<bool> = (0..24).map(|x| x % 5 < 3).collect();
        assert_eq!(inked, expected);
        assert!(row.iter().all(|alpha| *alpha == 0 || *alpha == 255));
    }
}
