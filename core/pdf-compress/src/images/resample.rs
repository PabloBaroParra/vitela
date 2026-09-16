//! Samples down to the size the preset asks for, and back into bytes.
//!
//! ## The rule this module exists to keep
//!
//! *The target DPI is a ceiling. Nothing here ever adds a sample.*
//!
//! [`shrunk_to`] is the whole policy, and it is four lines of arithmetic
//! because that is all it should be. An image already at or below the
//! preset's target comes back `None` — not "resized to itself", not "resized
//! by a factor of 1.0". The difference matters downstream: `None` is what
//! [`super::rewrite`] turns into *byte-identical*, and byte-identical is the
//! promise decision 5 of `docs/batch-compress.md` makes about an image the
//! preset has no business touching.
//!
//! Each axis is decided on its own, for the same reason
//! [`EffectiveDpi`](super::dpi::EffectiveDpi) measures them apart: a matrix
//! may stretch width and height by different factors, so an image can be
//! wastefully detailed across and exactly right down. Shrinking such an image
//! on one axis only leaves it with non-square pixels, which is not a defect —
//! a PDF image is mapped onto the unit square whatever its sample counts are,
//! so the placement stretches it back to the shape it was drawn as.
//!
//! The resize filter is a triangle (linear) kernel. Downscaling is an
//! averaging problem rather than an interpolation one, and `image`'s
//! implementation widens the kernel to the source's scale, so every source
//! sample contributes to the result. Lanczos would be sharper on photographs
//! and rings on the hard edges that a scanned page is mostly made of; it also
//! costs more per pixel, on a stage that runs once per image of a document
//! that may hold hundreds.

use image::{GrayImage, ImageBuffer, RgbImage};

use super::codec::Raster;
use super::dpi::EffectiveDpi;
use super::format::Channels;

/// The sample counts `pixels` must come down to for `target_dpi`, or `None`
/// when it is already there.
///
/// `None` also covers the case a preset can never justify: an image *below*
/// the target is not scaled up to meet it. The target buys a smaller file,
/// and inventing samples costs bytes to add detail that was never captured.
pub(crate) fn shrunk_to(
    pixels: (u32, u32),
    dpi: EffectiveDpi,
    target_dpi: u32,
) -> Option<(u32, u32)> {
    let target = f64::from(target_dpi);

    let axis = |samples: u32, measured: f64| {
        if !measured.is_finite() || measured <= target {
            return samples;
        }
        // Rounded rather than truncated, and floored at one: an image scaled
        // by a factor small enough to round an axis to zero still has to have
        // a row and a column, or it stops being an image.
        let scaled = (f64::from(samples) * target / measured).round();
        scaled.clamp(1.0, f64::from(samples)) as u32
    };

    let to = (axis(pixels.0, dpi.horizontal), axis(pixels.1, dpi.vertical));

    (to != pixels).then_some(to)
}

/// `raster`'s samples, resized to `to`.
pub(crate) fn resized(raster: &Raster, to: (u32, u32)) -> Vec<u8> {
    use image::imageops::{resize, FilterType};

    let (width, height) = (raster.width, raster.height);
    let samples = raster.samples.clone();

    match raster.channels {
        Channels::Grey => {
            let source: GrayImage = ImageBuffer::from_raw(width, height, samples)
                .expect("the raster holds its samples");
            resize(&source, to.0, to.1, FilterType::Triangle).into_raw()
        }
        Channels::Rgb => {
            let source: RgbImage = ImageBuffer::from_raw(width, height, samples)
                .expect("the raster holds its samples");
            resize(&source, to.0, to.1, FilterType::Triangle).into_raw()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dpi(horizontal: f64, vertical: f64) -> EffectiveDpi {
        EffectiveDpi {
            horizontal,
            vertical,
        }
    }

    #[test]
    fn an_image_above_the_target_comes_down_to_it() {
        assert_eq!(
            shrunk_to((600, 600), dpi(600.0, 600.0), 150),
            Some((150, 150))
        );
    }

    /// The ceiling, stated twice: at the target and below it, nothing
    /// happens. The second case is the one that matters — an image scaled up
    /// to meet the target would cost bytes to add detail nobody captured.
    #[test]
    fn an_image_at_or_below_the_target_is_left_where_it_is() {
        assert_eq!(shrunk_to((300, 300), dpi(150.0, 150.0), 150), None);
        assert_eq!(shrunk_to((300, 300), dpi(72.0, 72.0), 150), None);
    }

    /// A matrix may stretch one axis and not the other, so the axis that is
    /// already fine keeps every sample it has.
    #[test]
    fn only_the_axis_that_is_too_detailed_shrinks() {
        assert_eq!(
            shrunk_to((600, 600), dpi(600.0, 100.0), 150),
            Some((150, 600))
        );
    }

    /// An image scaled by a factor small enough to round an axis away still
    /// has to have a row and a column in it.
    #[test]
    fn a_shrink_never_rounds_an_axis_out_of_existence() {
        assert_eq!(
            shrunk_to((1_000, 4), dpi(96_000.0, 384.0), 96),
            Some((1, 1))
        );
    }

    /// A measurement that is not a number cannot ask for anything, so it asks
    /// for nothing. `governed_by` already refuses to build one of these, and
    /// this is the second door on the same room.
    #[test]
    fn an_unmeasurable_axis_asks_for_no_change() {
        assert_eq!(shrunk_to((600, 600), dpi(f64::NAN, f64::NAN), 150), None);
    }

    /// The resize is a real average, not a sample-and-hope: a black row and a
    /// white row halved vertically come out grey, and a nearest-neighbour
    /// implementation would return one or the other untouched.
    #[test]
    fn halving_two_rows_averages_them_rather_than_dropping_one() {
        let raster = Raster {
            width: 2,
            height: 2,
            channels: Channels::Grey,
            family: super::super::format::Family::Lossless,
            samples: vec![0, 0, 255, 255],
        };

        let halved = resized(&raster, (2, 1));

        assert_eq!(halved.len(), 2);
        for sample in halved {
            assert!(
                (100..=155).contains(&sample),
                "a halved black-and-white pair came out at {sample}, not near the middle"
            );
        }
    }

    #[test]
    fn a_resize_produces_exactly_the_samples_the_new_size_needs() {
        let raster = Raster {
            width: 8,
            height: 8,
            channels: Channels::Rgb,
            family: super::super::format::Family::Lossless,
            samples: vec![0x40; 8 * 8 * 3],
        };

        assert_eq!(resized(&raster, (3, 5)).len(), 3 * 5 * 3);
    }
}
