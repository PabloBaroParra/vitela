//! Image XObjects, and the pages that paint them.
//!
//! The image stage's fixtures live apart from the document ones because they
//! answer a different question. A document fixture is "what does a saved file
//! look like"; a raster fixture is "what does *this particular* image look
//! like" — a JPEG at a chosen quality, a colour space the crate refuses, a
//! stream one byte shorter than its dictionary claims. The corpus cannot be
//! relied on to hold any of them, and T-193's measurement of it says so: not
//! one image in the repository's real PDFs reaches even the lowest preset's
//! 96 effective DPI.

use lopdf::{dictionary, Document, Object, ObjectId, Stream};

/// A document whose pages each paint the same image XObject once.
///
/// The image is `pixels` samples; page *n* draws it `drawn[n]` user-space
/// units wide and tall. One page per entry, so a two-entry call is the case
/// that matters most — the same image placed at two scales.
///
/// The resource dictionary also carries `/Unplaced`, a second image XObject
/// nothing paints. Every inventory built from this fixture therefore also
/// says something about the image it must *not* include: an image declared in
/// resources but drawn from inside a form XObject, or as an inline image,
/// looks exactly like this from outside, and neither can be measured.
pub(crate) fn document_drawing_one_image(pixels: (u32, u32), drawn: &[(f64, f64)]) -> Document {
    let (width, height) = pixels;
    painting(image_xobject(width, height, 0x20), drawn).0
}

/// The same document with one page, built around an image the caller supplies
/// — a JPEG, an RGB buffer, a dictionary with something unusual in it.
///
/// Returns the object id of the image it paints, because the tests that use
/// this are the ones that go back to look at the stream afterwards.
pub(crate) fn painted_once(image: Stream, drawn: (f64, f64)) -> (Document, ObjectId) {
    painting(image, &[drawn])
}

/// One page per entry in `drawn`, each painting `image` at that size, plus the
/// permanent `/Unplaced` companion described on
/// [`document_drawing_one_image`].
fn painting(image: Stream, drawn: &[(f64, f64)]) -> (Document, ObjectId) {
    let (width, height) = (
        image
            .dict
            .get(b"Width")
            .and_then(Object::as_i64)
            .unwrap_or(1) as u32,
        image
            .dict
            .get(b"Height")
            .and_then(Object::as_i64)
            .unwrap_or(1) as u32,
    );
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();

    let image_id = document.add_object(image);
    let unplaced_id = document.add_object(image_xobject(width, height, 0x7f));
    let resources_id = document.add_object(dictionary! {
        "XObject" => dictionary! {
            "Im0" => image_id,
            "Unplaced" => unplaced_id,
        },
    });

    let mut kids = Vec::with_capacity(drawn.len());
    for (drawn_width, drawn_height) in drawn {
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            format!("q {drawn_width} 0 0 {drawn_height} 0 0 cm /Im0 Do Q").into_bytes(),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        kids.push(page_id.into());
    }

    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => drawn.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);

    (document, image_id)
}

/// A serialised document whose single page paints one `pixels` grey image at
/// `drawn` user-space units.
///
/// The bytes form, for the stages that work on a file rather than on a graph.
/// The image is noise rather than a flat fill on purpose: a flat fill flates
/// down to nothing whatever its sample count, so a document made of one would
/// show no difference between a preset that resamples and one that does not.
pub(crate) fn raster_document(pixels: (u32, u32), drawn: &[(f64, f64)]) -> Vec<u8> {
    let (mut document, _) = painting(grey_image(pixels.0, pixels.1), drawn);

    super::serialise(&mut document)
}

/// An unfiltered grey image whose samples vary across it.
///
/// A resampler needs something to average, and a uniform fill would let a
/// nearest-neighbour implementation pass every test a real average passes.
pub(crate) fn grey_image(width: u32, height: u32) -> Stream {
    image_stream(width, height, "DeviceGray", noise(width, height, 1))
}

/// The same in three components.
pub(crate) fn rgb_image(width: u32, height: u32) -> Stream {
    image_stream(width, height, "DeviceRGB", noise(width, height, 3))
}

/// An RGB image stored as a JPEG at `quality`, under `/DCTDecode`.
///
/// The content is noise on purpose. Noise is the expensive thing to store in
/// a JPEG, which is what makes `quality` a real lever here — the same picture
/// at 5 and at 75 differ by an order of magnitude, and that gap is what the
/// "a resample that would cost bytes is thrown away" test stands on.
pub(crate) fn jpeg_image(width: u32, height: u32, quality: u8) -> Stream {
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode(
            &noise(width, height, 3),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .expect("a fixture image encodes");

    let mut stream = image_stream(width, height, "DeviceRGB", bytes);
    stream.dict.set("Filter", "DCTDecode");
    stream.dict.set("Length", stream.content.len() as i64);
    stream.allows_compression = false;
    stream
}

/// `channels` bytes per pixel of repeatable pseudo-random noise.
fn noise(width: u32, height: u32, channels: usize) -> Vec<u8> {
    (0..(width as usize) * (height as usize) * channels)
        .map(|n| (((n as u32).wrapping_mul(2_654_435_761)) >> 24) as u8)
        .collect()
}

fn image_stream(width: u32, height: u32, space: &str, content: Vec<u8>) -> Stream {
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

/// One image XObject of `width` × `height` grey samples, filled with `fill`.
///
/// `fill` differs between the placed and unplaced images on purpose: two
/// byte-identical streams are one object as far as [`crate::prune`] is
/// concerned, and a fixture whose two images are the same image would be
/// testing something else.
fn image_xobject(width: u32, height: u32, fill: u8) -> Stream {
    Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(width),
            "Height" => i64::from(height),
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
        },
        vec![fill; (width as usize) * (height as usize)],
    )
}
