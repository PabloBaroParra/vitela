//! Samples into an image XObject.
//!
//! The one place in this generator that knows what a PDF image dictionary
//! must say, kept apart from [`super::documents`] for the reason the crate
//! under test keeps `format` apart from the rest of its image stage: "what
//! does this dictionary declare" is a different question from "what does this
//! page paint", and the fixtures need both answers to be adjustable without
//! disturbing the other.

use lopdf::{dictionary, Stream};

/// How many components a fixture image carries, and what that is called on
/// each side of the boundary.
#[derive(Debug, Clone, Copy)]
pub(super) enum Colour {
    Grey,
    Rgb,
}

impl Colour {
    pub(super) fn space(self) -> &'static str {
        match self {
            Colour::Grey => "DeviceGray",
            Colour::Rgb => "DeviceRGB",
        }
    }

    pub(super) fn encoded_as(self) -> image::ExtendedColorType {
        match self {
            Colour::Grey => image::ExtendedColorType::L8,
            Colour::Rgb => image::ExtendedColorType::Rgb8,
        }
    }
}

/// An 8-bit image stored as a baseline JPEG under `/DCTDecode`.
///
/// `quality` is 90 at both call sites, above `Balanced`'s 75 and `Small`'s 55
/// on purpose. The resampler throws away a re-encode that costs more bytes
/// than it replaces, so a fixture already stored at a preset's own quality
/// would be a fixture where a correct implementation and a careless one
/// produce the same file.
pub(super) fn jpeg(
    width: u32,
    height: u32,
    quality: u8,
    samples: Vec<u8>,
    colour: Colour,
) -> Stream {
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode(&samples, width, height, colour.encoded_as())
        .expect("a synthetic image encodes as a JPEG");

    let mut stream = image_dictionary(width, height, colour.space(), bytes);
    stream.dict.set("Filter", "DCTDecode");
    stream.dict.set("Length", stream.content.len() as i64);
    // Already lossy. Flating a JPEG a second time costs bytes and gains none.
    stream.allows_compression = false;
    stream
}

/// An 8-bit image stored under `/FlateDecode`.
pub(super) fn flate_image(
    width: u32,
    height: u32,
    space: &str,
    samples: Vec<u8>,
) -> lopdf::Result<Stream> {
    let mut stream = image_dictionary(width, height, space, samples);
    stream.compress()?;
    stream.dict.set("Length", stream.content.len() as i64);
    Ok(stream)
}

fn image_dictionary(width: u32, height: u32, space: &str, content: Vec<u8>) -> Stream {
    Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(width),
            "Height" => i64::from(height),
            "ColorSpace" => space,
            "BitsPerComponent" => 8,
        },
        content,
    )
}
