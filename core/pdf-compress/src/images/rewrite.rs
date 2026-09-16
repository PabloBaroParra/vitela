//! Putting a resampled image back, or deciding not to.
//!
//! ## The rule this module exists to keep
//!
//! *An image only changes if it changed for the better, and it changes with
//! everything that belongs to it.*
//!
//! Two promises meet here, and both of them are about doing nothing:
//!
//! **Byte-identical, not merely equivalent.** An image the preset has no
//! business touching is not re-encoded, not re-flated, not rewritten with the
//! same samples under a tidier dictionary. Its object is never assigned to.
//! The difference is invisible in a render and decisive in a diff — and it is
//! what makes "compressing an already-compressed file changes nothing at all"
//! true of a document full of photographs, not only of one full of text.
//!
//! **Smaller or nothing.** Decision 4 of `docs/batch-compress.md` is a
//! whole-file guarantee, enforced in [`crate::guarantee`]; this is the same
//! rule one scale down, per image. A resample that produces more bytes than
//! it replaces is thrown away, and the reason it can happen at all is worth
//! stating: re-encoding an image that was stored at a low JPEG quality, at
//! the preset's higher one, buys detail nobody asked for at a price the user
//! did not agree to. Measured on stored bytes, which is the only number that
//! reaches the file.
//!
//! **And with everything that belongs to it.** A `/SMask` is that image's
//! alpha channel, drawn over exactly the same area — so it has an effective
//! resolution of its own, inherited from its parent's, and the preset's
//! target applies to it on its own terms. Not by its parent's *factor*: a
//! mask at half its parent's resolution is already halfway down, and halving
//! it again would leave the alpha channel coarser than the colour it hides.
//! Both come out at the target, which is the whole point of there being one.
//!
//! It follows the parent rather than being found on its own, because alone it
//! is not an image anybody paints: it is never the operand of a `Do`, so
//! [`super::inventory`] cannot see it, and nothing in the document says how
//! large it is drawn except the image it masks. It is also never *counted*:
//! one photograph with an alpha channel is one image resampled, not two.

use lopdf::{Document, Object, ObjectId};

use super::dpi::EffectiveDpi;
use super::{codec, resample, PlacedImage};
use crate::preset::ImagePolicy;

/// Brings `placed` down to `policy`'s target, with its soft mask, and answers
/// whether anything was written.
///
/// `false` means the image is byte-identical to how it arrived — which is the
/// answer for every image already below the target, every image
/// [`codec::open`] declines to read, and every resample that came out
/// larger than the original.
pub(crate) fn shrink(document: &mut Document, placed: &PlacedImage, policy: ImagePolicy) -> bool {
    if !replaced(
        document,
        placed.id,
        placed.pixels,
        placed.governing_dpi,
        policy,
    ) {
        return false;
    }

    // Only once the image itself has shipped: a mask shrunk beside an image
    // that kept its pixels is a mask at the wrong resolution for nothing.
    if let Some((mask, pixels, dpi)) = soft_mask(document, placed) {
        replaced(document, mask, pixels, dpi, policy);
    }

    true
}

/// Resamples the XObject at `id` and writes it back, or leaves it untouched.
fn replaced(
    document: &mut Document,
    id: ObjectId,
    pixels: (u32, u32),
    dpi: EffectiveDpi,
    policy: ImagePolicy,
) -> bool {
    let Some(to) = resample::shrunk_to(pixels, dpi, policy.target_dpi) else {
        return false;
    };
    let Some(raster) = codec::open(document, id) else {
        return false;
    };

    let samples = resample::resized(&raster, to);
    let Ok(Object::Stream(original)) = document.get_object(id) else {
        return false;
    };
    let (dict, was) = (original.dict.clone(), original.content.len());

    let Some(stream) = codec::stored(dict, &raster, to, samples, policy.jpeg_quality) else {
        return false;
    };
    if stream.content.len() >= was {
        return false;
    }

    document.objects.insert(id, Object::Stream(stream));
    true
}

/// The soft mask of `placed`, with the resolution it inherits.
///
/// A mask is drawn over its parent's area, so its effective DPI is the
/// parent's scaled by how many samples it spends on the same paper: half the
/// parent's samples across is half the parent's DPI across. That number, not
/// the parent's, is what the preset's target then applies to.
fn soft_mask(
    document: &Document,
    placed: &PlacedImage,
) -> Option<(ObjectId, (u32, u32), EffectiveDpi)> {
    let parent = document.get_object(placed.id).ok()?.as_stream().ok()?;
    let id = parent.dict.get(b"SMask").ok()?.as_reference().ok()?;
    let pixels = super::pixel_size(document, id)?;
    let dpi = placed.governing_dpi.shared_with(placed.pixels, pixels)?;

    Some((id, pixels, dpi))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{grey_image, jpeg_image, painted_once};
    use lopdf::Stream;

    const BALANCED: ImagePolicy = ImagePolicy {
        target_dpi: 150,
        jpeg_quality: 75,
    };

    /// A document painting `image` one inch across, so its effective DPI is
    /// simply its width in samples.
    fn painted_at_one_inch(image: Stream) -> (Document, ObjectId, PlacedImage) {
        let (document, id) = painted_once(image, (72.0, 72.0));
        let placed = placement_of(&document, id);

        (document, id, placed)
    }

    /// The measurement of `id`, taken the way the stage takes it rather than
    /// written out by hand — so a test cannot pass against a placement the
    /// inventory would never have produced.
    fn placement_of(document: &Document, id: ObjectId) -> PlacedImage {
        super::super::inventory(document)
            .into_iter()
            .find(|placed| placed.id == id)
            .expect("the fixture paints its image")
    }

    /// Hangs `mask` off the image at `id` as its soft mask.
    fn masking(document: &mut Document, id: ObjectId, mask: Stream) -> ObjectId {
        let mask_id = document.add_object(Object::Stream(mask));
        document
            .get_object_mut(id)
            .and_then(Object::as_stream_mut)
            .expect("the image is a stream")
            .dict
            .set("SMask", mask_id);

        mask_id
    }

    fn stream_of(document: &Document, id: ObjectId) -> Stream {
        document
            .get_object(id)
            .and_then(Object::as_stream)
            .expect("the object is a stream")
            .clone()
    }

    fn side(stream: &Stream, key: &[u8]) -> i64 {
        stream
            .dict
            .get(key)
            .and_then(Object::as_i64)
            .expect("a side")
    }

    #[test]
    fn an_image_above_the_target_is_brought_down_to_it() {
        let (mut document, id, placed) = painted_at_one_inch(grey_image(600, 600));
        let before = stream_of(&document, id);

        assert!(shrink(&mut document, &placed, BALANCED));

        let after = stream_of(&document, id);
        assert_eq!(
            (side(&after, b"Width"), side(&after, b"Height")),
            (150, 150)
        );
        assert!(
            after.content.len() < before.content.len(),
            "a quarter-sized image stored {} bytes against {}",
            after.content.len(),
            before.content.len()
        );
    }

    /// The promise decision 5 makes, tested as identity rather than as
    /// equivalence: the object is not reassigned at all.
    #[test]
    fn an_image_already_below_the_target_comes_out_byte_identical() {
        let (mut document, id, placed) = painted_at_one_inch(grey_image(100, 100));
        let before = stream_of(&document, id);

        assert!(!shrink(&mut document, &placed, BALANCED));

        assert_eq!(stream_of(&document, id), before);
    }

    /// Everything a producer put in the dictionary and this crate has no
    /// opinion about has to survive the rewrite.
    #[test]
    fn a_resample_keeps_the_rest_of_the_dictionary() {
        let mut image = grey_image(600, 600);
        image.dict.set("Intent", "RelativeColorimetric");
        image.dict.set("Interpolate", true);
        let (mut document, id, placed) = painted_at_one_inch(image);

        assert!(shrink(&mut document, &placed, BALANCED));

        let after = stream_of(&document, id);
        assert_eq!(
            after.dict.get(b"Intent").and_then(Object::as_name).ok(),
            Some(b"RelativeColorimetric".as_ref())
        );
        assert_eq!(
            after
                .dict
                .get(b"Interpolate")
                .and_then(Object::as_bool)
                .ok(),
            Some(true)
        );
        assert_eq!(
            after.dict.get(b"ColorSpace").and_then(Object::as_name).ok(),
            Some(b"DeviceGray".as_ref())
        );
        assert_eq!(side(&after, b"BitsPerComponent"), 8);
    }

    /// A lossy image stays lossy and a lossless one stays lossless: the
    /// families never swap, because a JPEG cannot carry the alpha a flate
    /// image's `/SMask` supplies.
    #[test]
    fn a_resample_stores_the_image_the_way_it_found_it() {
        let (mut document, flate, placed) = painted_at_one_inch(grey_image(600, 600));
        assert!(shrink(&mut document, &placed, BALANCED));
        assert_eq!(
            stream_of(&document, flate)
                .dict
                .get(b"Filter")
                .and_then(Object::as_name)
                .ok(),
            Some(b"FlateDecode".as_ref())
        );

        let (mut document, jpeg, placed) = painted_at_one_inch(jpeg_image(600, 600, 80));
        assert!(shrink(&mut document, &placed, BALANCED));
        assert_eq!(
            stream_of(&document, jpeg)
                .dict
                .get(b"Filter")
                .and_then(Object::as_name)
                .ok(),
            Some(b"DCTDecode".as_ref())
        );
    }

    /// The per-image half of the never-grow rule, at the scale where it
    /// really bites: 160 effective DPI is barely over the target, so the
    /// resample buys 12% of the samples back — and re-encoding those at the
    /// preset's quality 75 costs far more than the quality 5 they were stored
    /// at. The resample is thrown away and the original ships.
    #[test]
    fn a_resample_that_would_cost_bytes_is_thrown_away() {
        // 600 samples over 270 units is 160 effective dpi.
        let (mut document, id) = painted_once(jpeg_image(600, 600, 5), (270.0, 270.0));
        let placed = placement_of(&document, id);
        let before = stream_of(&document, id);

        assert!(!shrink(&mut document, &placed, BALANCED));

        assert_eq!(stream_of(&document, id), before);
    }

    /// The soft mask follows its parent down — to the preset's target, which
    /// it reaches from its own inherited resolution rather than by its
    /// parent's factor. A 300-sample mask over a 600-sample image is at half
    /// its parent's DPI, so it is already halfway down and only halves again.
    #[test]
    fn a_soft_mask_comes_down_to_the_same_target_as_the_image_it_masks() {
        let (mut document, id) = painted_once(grey_image(600, 600), (72.0, 72.0));
        let mask = masking(&mut document, id, grey_image(300, 300));
        let placed = placement_of(&document, id);

        assert!(shrink(&mut document, &placed, BALANCED));

        let after = stream_of(&document, mask);
        assert_eq!(
            (side(&after, b"Width"), side(&after, b"Height")),
            (150, 150),
            "the mask was 300 dpi over the same inch, so 150 dpi asks for half of it"
        );
        assert_eq!(
            (
                side(&stream_of(&document, id), b"Width"),
                side(&stream_of(&document, id), b"Height")
            ),
            (150, 150),
            "and the image it masks lands on the same target"
        );
        assert_eq!(
            stream_of(&document, id)
                .dict
                .get(b"SMask")
                .and_then(Object::as_reference)
                .ok(),
            Some(mask),
            "the resampled image lost its alpha channel"
        );
    }

    /// A mask whose parent keeps its pixels keeps its own, so the two never
    /// drift apart in resolution for no reason.
    #[test]
    fn a_soft_mask_of_an_untouched_image_is_untouched() {
        let (mut document, id) = painted_once(grey_image(100, 100), (72.0, 72.0));
        let mask = masking(&mut document, id, grey_image(600, 600));
        let before = stream_of(&document, mask);
        let placed = placement_of(&document, id);

        assert!(!shrink(&mut document, &placed, BALANCED));

        assert_eq!(stream_of(&document, mask), before);
    }
}
