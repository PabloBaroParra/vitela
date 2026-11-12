//! The colour the page shows behind a block, read off the rendered page so
//! the editor's box can cover the original text in the page's own colour
//! rather than in a box of the theme's.

use gtk::{gdk, prelude::*};
use pdf_document::Rect;
use pdf_render::{place_rect, PagePlacement, TextRect};

/// White, for a page with no render yet to read: what nearly every page is.
pub(crate) const PAPER: [f64; 3] = [1.0, 1.0, 1.0];

/// The page's colour under `bbox`, read from `texture` — the page as
/// rendered, `page` its placement on screen. `None` when the box falls
/// outside the texture.
pub(crate) fn sample(texture: &gdk::Texture, bbox: Rect, page: PagePlacement) -> Option<[f64; 3]> {
    let (width, height) = (texture.width(), texture.height());
    if width <= 0 || height <= 0 || page.width_pt <= 0.0 {
        return None;
    }
    // The texture spans the page as drawn, at its own resolution.
    let scaled = PagePlacement {
        scale: f64::from(width) / f64::from(page.width_pt),
        ..page
    };
    let placed = place_rect(
        TextRect {
            x_pt: bbox.x as f32,
            y_pt: bbox.y as f32,
            width_pt: bbox.width as f32,
            height_pt: bbox.height as f32,
        },
        scaled,
    );
    let stride = width as usize * 4;
    let mut pixels = vec![0; stride * height as usize];
    // Native-endian premultiplied ARGB: B, G, R, A in memory here.
    texture.download(&mut pixels, stride);
    let area = PixelArea {
        left: placed.left.floor().max(0.0) as usize,
        top: placed.top.floor().max(0.0) as usize,
        right: (placed.left + placed.width).ceil().max(0.0) as usize,
        bottom: (placed.top + placed.height).ceil().max(0.0) as usize,
    };
    dominant_colour(&pixels, stride, width as usize, height as usize, area)
        .map(|[b, g, r]| [r, g, b].map(|channel| f64::from(channel) / 255.0))
}

/// A box of pixels, right and bottom exclusive.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PixelArea {
    pub(crate) left: usize,
    pub(crate) top: usize,
    pub(crate) right: usize,
    pub(crate) bottom: usize,
}

/// The most common colour in `area` of a 4-byte-per-pixel buffer, as its
/// first three bytes. Text covers a minority of any box it sits in, so the
/// commonest colour is the paper's. Colours are bucketed to 5 bits a
/// channel first, so antialiasing and JPEG noise do not split the paper
/// into many near-equal shades; the answer is the bucket's mean.
pub(crate) fn dominant_colour(
    pixels: &[u8],
    stride: usize,
    width: usize,
    height: usize,
    area: PixelArea,
) -> Option<[u8; 3]> {
    let (right, bottom) = (area.right.min(width), area.bottom.min(height));
    let mut buckets: std::collections::HashMap<[u8; 3], (u32, [u64; 3])> = Default::default();
    for y in area.top..bottom {
        for x in area.left..right {
            let at = y * stride + x * 4;
            let Some(pixel) = pixels.get(at..at + 3) else {
                continue;
            };
            let key = [pixel[0] >> 3, pixel[1] >> 3, pixel[2] >> 3];
            let entry = buckets.entry(key).or_insert((0, [0; 3]));
            entry.0 += 1;
            for (sum, channel) in entry.1.iter_mut().zip(pixel) {
                *sum += u64::from(*channel);
            }
        }
    }
    let (count, sums) = buckets.into_values().max_by_key(|(count, _)| *count)?;
    Some(sums.map(|sum| (sum / u64::from(count)) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10x4 buffer of `paper` with a short run of `ink` across row 1.
    fn page(paper: [u8; 3], ink: [u8; 3]) -> Vec<u8> {
        let mut pixels = Vec::new();
        for y in 0..4 {
            for x in 0..10 {
                let colour = if y == 1 && x < 4 { ink } else { paper };
                pixels.extend_from_slice(&colour);
                pixels.push(255);
            }
        }
        pixels
    }

    #[test]
    fn the_paper_outnumbers_the_ink() {
        let pixels = page([240, 230, 200], [10, 10, 10]);
        let area = PixelArea {
            left: 0,
            top: 0,
            right: 10,
            bottom: 4,
        };
        assert_eq!(
            dominant_colour(&pixels, 40, 10, 4, area),
            Some([240, 230, 200])
        );
    }

    #[test]
    fn an_area_past_the_buffer_is_clipped_and_an_empty_one_has_no_colour() {
        let pixels = page([255, 255, 255], [0, 0, 0]);
        let past = PixelArea {
            left: 5,
            top: 2,
            right: 50,
            bottom: 50,
        };
        assert_eq!(
            dominant_colour(&pixels, 40, 10, 4, past),
            Some([255, 255, 255])
        );
        let empty = PixelArea {
            left: 20,
            top: 0,
            right: 30,
            bottom: 4,
        };
        assert_eq!(dominant_colour(&pixels, 40, 10, 4, empty), None);
    }
}
