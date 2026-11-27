//! Bounded decoding of the image files a user hands in (PNG or JPEG): a
//! stamp, an inserted image, a replacement picture.
//!
//! A file's header says how large its picture is, and a decoder allocates for
//! that before it reads a single sample: a 69-byte PNG can declare
//! 11000 × 11000 pixels and make it allocate half a gigabyte, which takes the
//! WebAssembly engine down. So every decode here checks the declared size
//! first, from the header alone, against the core's raster budget
//! ([`MAX_IMAGE_SIDE`], [`MAX_IMAGE_PIXELS`] — the ceilings `pdf-render`
//! rasterises a page within), and the decoder itself runs under the same
//! limits as a backstop.

use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

use crate::error::EditError;

/// Longest side, in pixels, of an image file the core decodes.
pub const MAX_IMAGE_SIDE: u32 = 16_384;

/// Most pixels an image file the core decodes may declare (32 Mpx: an RGBA
/// picture of 128 MiB).
pub const MAX_IMAGE_PIXELS: u64 = 32 * 1024 * 1024;

/// What the decoder may allocate: [`MAX_IMAGE_PIXELS`] at 16-bit RGBA.
const MAX_DECODE_BYTES: u64 = MAX_IMAGE_PIXELS * 8;

/// What an image file's header says, read without decoding its samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageHeader {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
}

/// Reads `bytes`' header: its size, checked against the core's limits, and
/// whether it has an alpha channel. A file whose samples are broken passes;
/// [`decode_image`] refuses it.
pub fn probe_image(bytes: &[u8]) -> Result<ImageHeader, EditError> {
    let decoder = reader(bytes)?.into_decoder().map_err(invalid)?;
    let (width, height) = decoder.dimensions();
    check_size(width, height)?;
    Ok(ImageHeader {
        width,
        height,
        has_alpha: decoder.color_type().has_alpha(),
    })
}

/// Decodes `bytes` (PNG or JPEG), refusing before any allocation a picture
/// larger than the core's limits.
pub fn decode_image(bytes: &[u8]) -> Result<DynamicImage, EditError> {
    probe_image(bytes)?;
    reader(bytes)?.decode().map_err(invalid)
}

fn reader(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, EditError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| EditError::InvalidImage(error.to_string()))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    Ok(reader)
}

fn check_size(width: u32, height: u32) -> Result<(), EditError> {
    let pixels = u64::from(width) * u64::from(height);
    if width > MAX_IMAGE_SIDE || height > MAX_IMAGE_SIDE || pixels > MAX_IMAGE_PIXELS {
        return Err(EditError::InvalidImage(format!(
            "the image is too large ({width} × {height} pixels; at most \
             {MAX_IMAGE_PIXELS} pixels, {MAX_IMAGE_SIDE} on a side)"
        )));
    }
    Ok(())
}

fn invalid(error: image::ImageError) -> EditError {
    EditError::InvalidImage(error.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{ImageFormat, RgbaImage};

    /// A complete 69-byte PNG whose header declares 11000 × 11000 RGBA
    /// pixels (about 484 MB once decoded).
    pub(crate) const OVERSIZED_PNG: [u8; 69] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x2a, 0xf8, 0x00, 0x00, 0x2a, 0xf8, 0x08, 0x06, 0x00, 0x00, 0x00, 0xce,
        0x2f, 0x9a, 0x8a, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0xa0, 0x0c, 0x00, 0x00, 0x00, 0x40, 0x00, 0x01, 0xb7, 0x34, 0x7c, 0xef, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        RgbaImage::from_pixel(width, height, image::Rgba([1, 2, 3, 128]))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    fn too_large(result: Result<impl std::fmt::Debug, EditError>) -> bool {
        matches!(result, Err(EditError::InvalidImage(message)) if message.contains("too large"))
    }

    #[test]
    fn a_header_declaring_more_pixels_than_the_budget_is_refused_before_decoding() {
        assert!(too_large(probe_image(&OVERSIZED_PNG)));
        assert!(too_large(decode_image(&OVERSIZED_PNG)));
    }

    #[test]
    fn a_side_longer_than_the_limit_is_refused_even_within_the_pixel_budget() {
        // 20000 × 1: few pixels, one side too long.
        let mut wide = OVERSIZED_PNG;
        wide[16..20].copy_from_slice(&20_000u32.to_be_bytes());
        wide[20..24].copy_from_slice(&1u32.to_be_bytes());
        wide[29..33].copy_from_slice(&[0x3b, 0xb4, 0x9e, 0x8e]);
        assert!(matches!(
            probe_image(&wide),
            Err(EditError::InvalidImage(_))
        ));
    }

    #[test]
    fn an_image_within_the_limits_decodes_and_its_header_reads() {
        let bytes = png(40, 20);
        assert_eq!(
            probe_image(&bytes).unwrap(),
            ImageHeader {
                width: 40,
                height: 20,
                has_alpha: true
            }
        );
        assert_eq!(decode_image(&bytes).unwrap().width(), 40);
    }

    #[test]
    fn bytes_that_are_no_image_are_refused() {
        assert!(matches!(
            decode_image(b"not an image"),
            Err(EditError::InvalidImage(_))
        ));
    }
}
