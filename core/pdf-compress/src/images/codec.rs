//! The stream ⇄ samples boundary: an image XObject opened, and closed again.
//!
//! ## The rule this module exists to keep
//!
//! *An image this module cannot read back exactly is an image nobody
//! resamples — and one it does read comes back in the family it arrived in.*
//!
//! Both directions live here because they are one question answered twice.
//! [`open`] turns a stream into plain samples; [`stored`] turns plain samples
//! back into a stream. A change to either that the other does not match is
//! how an image comes out of a compression unreadable, so they are written
//! where they can be read together.
//!
//! [`super::inventory`] decided which images are even visible;
//! [`super::format`] decided what their dictionaries declare. What is left to
//! this module is the part that touches bytes, and its own two refusals:
//!
//! - **8 bits per component.** A 1-, 2-, 4- or 16-bit image packs its samples
//!   in a layout with row padding, and resampling the packed bytes would
//!   scramble it.
//! - **No `/Decode`, no `/Mask`, not an `/ImageMask`.** Each of these makes a
//!   sample mean something other than its own value: an inverted range, a
//!   colour-key range that interpolation can wander into, a stencil.
//!
//! A `/SMask` is *not* refused, and is the one companion that follows its
//! parent down — see [`super::rewrite`].

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::format::{self, Channels, Family};

/// One image XObject, decoded into plain samples.
pub(crate) struct Raster {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) channels: Channels,
    pub(crate) family: Family,
    /// `width * height * channels.count()` bytes, row-major, top row first.
    pub(crate) samples: Vec<u8>,
}

/// The ceiling on what one image may decode to.
///
/// A stream that claims to be a postage stamp and inflates to a gigabyte is a
/// decompression bomb, and this crate runs at save time on whatever the user
/// opened. The limit is per image and generous — a 400 DPI A3 scan in RGB is
/// about 60 MB — because its job is to refuse an attack, not to second-guess
/// a large document.
const MAX_DECODED_BYTES: usize = 256 * 1024 * 1024;

/// The image at `id`, decoded — or `None` when this crate will not touch it.
///
/// Every `None` is one of the refusals in this module's header or in
/// [`super::format`]'s, and every one of them leaves the image
/// byte-identical.
pub(crate) fn open(document: &Document, id: ObjectId) -> Option<Raster> {
    let stream = document.get_object(id).ok()?.as_stream().ok()?;
    let dict = &stream.dict;

    if dict.get(b"Decode").is_ok() || dict.get(b"Mask").is_ok() {
        return None;
    }
    if dict
        .get(b"ImageMask")
        .and_then(Object::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    if dict.get(b"BitsPerComponent").ok()?.as_i64().ok()? != 8 {
        return None;
    }

    let width = side(dict, b"Width")?;
    let height = side(dict, b"Height")?;
    let channels = format::channels(document, dict)?;
    let family = format::family(stream)?;

    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(channels.count())?;
    if expected == 0 || expected > MAX_DECODED_BYTES {
        return None;
    }

    let mut samples = match family {
        Family::Lossless => decoded(stream, expected)?,
        Family::Jpeg => from_jpeg(stream, (width, height), channels)?,
    };
    // A producer may leave padding on the tail of a stream; it may not come
    // up short, because the missing rows would have to be invented.
    if samples.len() < expected {
        return None;
    }
    samples.truncate(expected);

    Some(Raster {
        width,
        height,
        channels,
        family,
        samples,
    })
}

/// `samples` at size `to`, stored the way `raster` was already stored.
///
/// `dict` is the original image's, with two entries changed and the storage
/// adjusted — so `/ColorSpace`, `/BitsPerComponent`, `/SMask`, `/Intent` and
/// anything else a producer put there survive a resample untouched. A stage
/// that rebuilt the dictionary out of what it understands would silently drop
/// what it does not.
pub(crate) fn stored(
    mut dict: Dictionary,
    raster: &Raster,
    to: (u32, u32),
    samples: Vec<u8>,
    quality: u8,
) -> Option<Stream> {
    dict.set("Width", i64::from(to.0));
    dict.set("Height", i64::from(to.1));

    match raster.family {
        Family::Jpeg => {
            let bytes = as_jpeg(&samples, to, raster.channels, quality)?;
            dict.set("Filter", "DCTDecode");
            // The bytes are already a compressed format; flating them again
            // is the one thing the structural pass must not do to them.
            Some(Stream::new(dict, bytes).with_compression(false))
        }
        Family::Lossless => {
            // The samples are plain again, so whatever filter described the
            // old ones no longer describes these. `compress` puts one back if
            // it wins, and the caller throws the whole stream away if it did
            // not win enough.
            dict.remove(b"Filter");
            let mut stream = Stream::new(dict, samples);
            stream.compress().ok()?;
            Some(stream)
        }
    }
}

/// `key` read as a sample count.
fn side(dict: &Dictionary, key: &[u8]) -> Option<u32> {
    u32::try_from(dict.get(key).ok()?.as_i64().ok()?).ok()
}

/// The stream's raw samples, decoded under a bound of `expected` bytes.
///
/// The bound is the image's own declared size plus a little slack for a
/// producer's trailing bytes, which makes it the tightest honest limit
/// available: a stream that inflates well past the pixels it claims to hold
/// is not a stream this module could make sense of anyway.
fn decoded(stream: &Stream, expected: usize) -> Option<Vec<u8>> {
    const SLACK: usize = 1024;

    stream
        .get_plain_content_with_limit(expected.saturating_add(SLACK))
        .ok()
}

/// The stream's JPEG file, decoded to samples.
///
/// The declared `/Width` and `/Height` are handed to the decoder as limits
/// rather than checked after the fact, so a JPEG whose own header disagrees
/// with its dictionary is refused *before* it allocates for the larger of the
/// two.
fn from_jpeg(stream: &Stream, pixels: (u32, u32), channels: Channels) -> Option<Vec<u8>> {
    let mut reader = image::ImageReader::with_format(
        std::io::Cursor::new(&stream.content),
        image::ImageFormat::Jpeg,
    );
    let mut limits = image::Limits::no_limits();
    limits.max_image_width = Some(pixels.0);
    limits.max_image_height = Some(pixels.1);
    limits.max_alloc = Some(MAX_DECODED_BYTES as u64);
    reader.limits(limits);

    let decoded = reader.decode().ok()?;
    if (decoded.width(), decoded.height()) != pixels {
        return None;
    }

    // The dictionary is the authority on what the samples mean, so a decode
    // that disagrees with it is refused rather than converted: a CMYK JPEG
    // under a `/DeviceCMYK` dictionary arrives here as three channels, and
    // writing those back would silently change the image's colour space.
    match (channels, decoded.color()) {
        (Channels::Grey, image::ColorType::L8) => Some(decoded.into_luma8().into_raw()),
        (Channels::Rgb, image::ColorType::Rgb8) => Some(decoded.into_rgb8().into_raw()),
        _ => None,
    }
}

/// `samples` written as a baseline JPEG at `quality`.
///
/// `None` when the encoder refuses, which is not a failure of the
/// compression: the caller keeps the image it already had.
fn as_jpeg(samples: &[u8], size: (u32, u32), channels: Channels, quality: u8) -> Option<Vec<u8>> {
    let colour = match channels {
        Channels::Grey => image::ExtendedColorType::L8,
        Channels::Rgb => image::ExtendedColorType::Rgb8,
    };

    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode(samples, size.0, size.1, colour)
        .ok()?;

    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{grey_image, jpeg_image, painted_once, rgb_image};

    /// Opens the single image of a document built around `image`.
    fn opened(image: Stream) -> Option<Raster> {
        let (document, id) = painted_once(image, (150.0, 150.0));
        open(&document, id)
    }

    /// The same, for a dictionary tweak applied to an otherwise plain image.
    fn opened_with(edit: impl FnOnce(&mut Dictionary)) -> Option<Raster> {
        let mut image = grey_image(8, 8);
        edit(&mut image.dict);
        opened(image)
    }

    #[test]
    fn a_plain_grey_image_reads_back_its_samples() {
        let raster = opened(grey_image(8, 4)).expect("a plain grey image is readable");

        assert_eq!((raster.width, raster.height), (8, 4));
        assert_eq!(raster.channels, Channels::Grey);
        assert_eq!(raster.family, Family::Lossless);
        assert_eq!(raster.samples.len(), 32);
    }

    #[test]
    fn a_plain_rgb_image_reads_three_components_a_sample() {
        let raster = opened(rgb_image(8, 4)).expect("a plain rgb image is readable");

        assert_eq!(raster.channels, Channels::Rgb);
        assert_eq!(raster.samples.len(), 96);
    }

    #[test]
    fn a_jpeg_image_reads_back_as_the_lossy_family() {
        let raster = opened(jpeg_image(16, 16, 80)).expect("a jpeg image is readable");

        assert_eq!(raster.family, Family::Jpeg);
        assert_eq!(raster.channels, Channels::Rgb);
        assert_eq!(raster.samples.len(), 16 * 16 * 3);
    }

    #[test]
    fn a_depth_other_than_eight_bits_is_refused() {
        for depth in [1, 2, 4, 16] {
            assert!(
                opened_with(|dict| dict.set("BitsPerComponent", depth)).is_none(),
                "{depth} bits per component was opened"
            );
        }
    }

    /// Each of these makes a sample mean something other than its own value.
    #[test]
    fn an_image_whose_samples_are_reinterpreted_is_refused() {
        assert!(opened_with(|dict| dict.set("Decode", vec![1.into(), 0.into()])).is_none());
        assert!(opened_with(|dict| dict.set("Mask", vec![0.into(), 16.into()])).is_none());
        assert!(opened_with(|dict| dict.set("ImageMask", true)).is_none());
    }

    /// The missing rows would have to be invented, and an invented row is a
    /// band of black across the bottom of the image.
    #[test]
    fn a_stream_shorter_than_the_pixels_it_claims_is_refused() {
        let mut image = grey_image(8, 8);
        image.set_content(vec![0x40; 8 * 8 - 1]);

        assert!(opened(image).is_none());
    }

    /// A JPEG whose header disagrees with its dictionary is refused before it
    /// decodes, which is also what stops it allocating for the larger of the
    /// two.
    #[test]
    fn a_jpeg_that_is_not_the_size_its_dictionary_claims_is_refused() {
        let mut image = jpeg_image(16, 16, 80);
        image.dict.set("Width", 32);

        assert!(opened(image).is_none());
    }

    /// The round trip, which is the reason both directions live in one file:
    /// what [`stored`] writes, [`open`] reads back at the new size.
    #[test]
    fn what_is_stored_can_be_opened_again() {
        for image in [grey_image(8, 8), rgb_image(8, 8), jpeg_image(16, 16, 80)] {
            let family = format::family(&image).expect("the fixture is a family this crate reads");
            let (mut document, id) = painted_once(image, (150.0, 150.0));
            let raster = open(&document, id).expect("the fixture is readable");
            let samples = vec![0x40; 4 * 4 * raster.channels.count()];

            let stream = stored(
                document
                    .get_object(id)
                    .and_then(Object::as_stream)
                    .expect("a stream")
                    .dict
                    .clone(),
                &raster,
                (4, 4),
                samples,
                75,
            )
            .expect("a resampled image stores");
            document.objects.insert(id, Object::Stream(stream));

            let reopened = open(&document, id).expect("what was stored reads back");
            assert_eq!((reopened.width, reopened.height), (4, 4));
            assert_eq!(reopened.family, family, "the storage family changed");
            assert_eq!(reopened.channels, raster.channels);
        }
    }

    /// The quality number is the preset's lever, so it has to be one: the
    /// same picture at a lower quality has to cost fewer bytes.
    #[test]
    fn a_lower_quality_writes_fewer_bytes() {
        let samples: Vec<u8> = (0..64u32 * 64)
            .map(|n| (n.wrapping_mul(2_654_435_761) >> 24) as u8)
            .collect();

        let high = as_jpeg(&samples, (64, 64), Channels::Grey, 90).expect("encodes");
        let low = as_jpeg(&samples, (64, 64), Channels::Grey, 20).expect("encodes");

        assert!(
            low.len() < high.len(),
            "quality 20 produced {} bytes and quality 90 produced {}",
            low.len(),
            high.len()
        );
    }
}
