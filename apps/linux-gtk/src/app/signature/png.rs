//! Strokes to PNG: the signature as a transparent picture with black, round-
//! capped ink, cropped to what [`ink::frame`] says.
//!
//! Cairo draws it — the same library that paints the pad, so the PNG looks the
//! way the pad did — and the `image` crate encodes it, because Cairo's own PNG
//! writer is an optional feature this crate does not enable and Cairo's pixels
//! are premultiplied, which a PNG is not.
//!
//! Nothing here touches GTK: an `ImageSurface` needs no display, so this runs
//! on a worker thread and under a plain `#[test]`.

use gtk::cairo;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};

use super::ink::{self, Stroke};

/// The strokes as PNG bytes, or `None` when there is no line to draw or the
/// picture cannot be produced.
pub(crate) fn render_png(strokes: &[Stroke], stroke_width: f64) -> Option<Vec<u8>> {
    let frame = ink::frame(strokes, stroke_width, ink::MAX_SIDE_PX)?;
    let (width, height) = (frame.pixel_width(), frame.pixel_height());
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    {
        let context = cairo::Context::new(&surface).ok()?;
        context.scale(frame.scale, frame.scale);
        context.translate(-frame.left, -frame.top);
        context.set_source_rgba(0.0, 0.0, 0.0, 1.0);
        context.set_line_width(stroke_width);
        context.set_line_cap(cairo::LineCap::Round);
        context.set_line_join(cairo::LineJoin::Round);
        for stroke in strokes.iter().filter(|stroke| stroke.len() >= 2) {
            context.move_to(stroke[0].0, stroke[0].1);
            for &(x, y) in &stroke[1..] {
                context.line_to(x, y);
            }
            context.stroke().ok()?;
        }
    }
    let stride = usize::try_from(surface.stride()).ok()?;
    let (width, height) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    let mut rgba = Vec::new();
    surface
        .with_data(|data| rgba = straight_rgba(data, width as usize, height as usize, stride))
        .ok()?;
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&rgba, width, height, ExtendedColorType::Rgba8)
        .ok()?;
    Some(png)
}

/// Cairo's `ARgb32` (premultiplied, native-endian 32-bit words) as the straight
/// RGBA bytes a PNG stores. `stride` is Cairo's row length in bytes, which can
/// be wider than `width * 4`.
fn straight_rgba(data: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let row_start = row * stride;
        for column in 0..width {
            let at = row_start + column * 4;
            let word = u32::from_ne_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
            let alpha = (word >> 24) & 0xff;
            let unpremultiply = |channel: u32| match alpha {
                0 => 0,
                // Rounded, and clamped: premultiplied data can round a channel
                // a hair above its alpha.
                _ => ((channel * 255 + alpha / 2) / alpha).min(255) as u8,
            };
            rgba.extend_from_slice(&[
                unpremultiply((word >> 16) & 0xff),
                unpremultiply((word >> 8) & 0xff),
                unpremultiply(word & 0xff),
                alpha as u8,
            ]);
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(png: &[u8]) -> image::RgbaImage {
        image::load_from_memory(png)
            .expect("the PNG must decode")
            .to_rgba8()
    }

    #[test]
    fn no_line_means_no_picture() {
        assert_eq!(render_png(&[vec![(5.0, 5.0)]], 4.0), None);
    }

    #[test]
    fn the_picture_is_cropped_to_the_ink_plus_half_a_stroke() {
        let strokes = [vec![(100.0, 50.0), (300.0, 80.0)]];

        let picture = decoded(&render_png(&strokes, 8.0).expect("there is a line"));

        assert_eq!(picture.dimensions(), (208, 38));
    }

    #[test]
    fn a_long_signature_is_capped_at_the_long_side() {
        let strokes = [vec![(0.0, 0.0), (2396.0, 596.0)]];

        let picture = decoded(&render_png(&strokes, 4.0).expect("there is a line"));

        assert_eq!(picture.dimensions(), (1200, 300));
    }

    #[test]
    fn the_background_is_transparent_and_the_ink_is_opaque_black() {
        let strokes = [vec![(0.0, 0.0), (100.0, 100.0)]];

        let picture = decoded(&render_png(&strokes, 8.0).expect("there is a line"));

        // A corner far from the diagonal is untouched pad.
        assert_eq!(picture.get_pixel(picture.width() - 1, 0).0[3], 0);
        // The middle of the diagonal is ink: black, fully opaque.
        let (middle_x, middle_y) = (picture.width() / 2, picture.height() / 2);
        assert_eq!(picture.get_pixel(middle_x, middle_y).0, [0, 0, 0, 255]);
    }

    #[test]
    fn straight_rgba_undoes_premultiplication_and_skips_stride_padding() {
        // One pixel of two-wide rows with 4 bytes of padding: a half-opaque
        // premultiplied red (alpha 128, red 128) in the first slot.
        let word = (128u32 << 24) | (128u32 << 16);
        let mut data = vec![0u8; 12];
        data[..4].copy_from_slice(&word.to_ne_bytes());

        let rgba = straight_rgba(&data, 1, 1, 12);

        assert_eq!(rgba, [255, 0, 0, 128]);
    }
}
