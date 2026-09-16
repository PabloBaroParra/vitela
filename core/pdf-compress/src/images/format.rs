//! What an image dictionary declares its stream to be.
//!
//! ## The rule this module exists to keep
//!
//! *The dictionary is the authority on what the samples mean. Nothing is
//! inferred from the bytes.*
//!
//! Two questions, and both of them are answered by reading, never by
//! guessing: how many components does a sample carry, and how are those
//! samples stored? [`super::codec`] does the work that depends on the
//! answers; this module only supplies them, and supplies `None` far more
//! often than it supplies a value.
//!
//! Every `None` here is a refusal, and every refusal costs the user nothing
//! but a byte they did not save. The alternative — decoding an image under an
//! assumed layout — costs them the image. So the accepted set is deliberately
//! narrow:
//!
//! - **Grey or RGB.** Named, calibrated, or `/ICCBased` with one or three
//!   components. Every other space is refused, each for its own reason:
//!   `/Indexed` samples are table *offsets*, and the average of two offsets
//!   is a third colour with no relation to either; `/Separation` and
//!   `/DeviceN` are ink amounts behind a tint transform; `/DeviceCMYK` is
//!   four channels that the JPEG decoder hands back as three.
//! - **Unfiltered, `/FlateDecode`, or `/DCTDecode`, alone and with no
//!   `/DecodeParms`.** The first two are raw samples; the third is a JPEG
//!   file, byte for byte. A predictor, an `/ASCII85Decode` wrapper, a
//!   `/JPXDecode` or a `/CCITTFaxDecode` all mean the crate would be
//!   re-encoding something it had only half decoded.

use lopdf::{Dictionary, Document, Object, Stream};

/// How many components a sample carries, and what they mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channels {
    /// One component: `/DeviceGray`, `/CalGray`, or `/ICCBased` with `/N 1`.
    Grey,
    /// Three: `/DeviceRGB`, `/CalRGB`, or `/ICCBased` with `/N 3`.
    Rgb,
}

impl Channels {
    /// Bytes per sample, at the eight bits per component [`super::codec`]
    /// insists on.
    pub(crate) fn count(self) -> usize {
        match self {
            Channels::Grey => 1,
            Channels::Rgb => 3,
        }
    }
}

/// How the stream stores its samples — and therefore how a rewrite has to
/// store them again.
///
/// The families never mix. Decision 5 of `docs/batch-compress.md`: a flate
/// image is not turned into a JPEG to win bytes, because JPEG has no alpha
/// channel and the `/SMask` would go with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    /// Raw samples, stored either in the clear or under `/FlateDecode`.
    /// Lossless: a rewrite re-flates them and loses nothing.
    Lossless,
    /// A JPEG file under `/DCTDecode`. Already lossy, which is the only
    /// reason a preset may re-encode it at a quality number at all.
    Jpeg,
}

/// The colour space, as a component count the resampler can average — or
/// `None` for every space where a sample is not simply its own value.
pub(crate) fn channels(document: &Document, dict: &Dictionary) -> Option<Channels> {
    let space = document.dereference(dict.get(b"ColorSpace").ok()?).ok()?.1;

    match space {
        Object::Name(name) => by_name(name),
        Object::Array(items) => by_family(document, items),
        _ => None,
    }
}

/// The device and calibrated spaces, named directly.
fn by_name(name: &[u8]) -> Option<Channels> {
    match name {
        b"DeviceGray" | b"CalGray" | b"G" => Some(Channels::Grey),
        b"DeviceRGB" | b"CalRGB" | b"RGB" => Some(Channels::Rgb),
        _ => None,
    }
}

/// The array forms. Only `/ICCBased` is opened any further, and only for its
/// `/N`: a profile with one or three components still stores those components
/// as plain samples, which is all the resampler needs to be true.
fn by_family(document: &Document, items: &[Object]) -> Option<Channels> {
    let family = items.first()?.as_name().ok()?;
    if family != b"ICCBased" {
        return by_name(family);
    }

    let profile = document
        .dereference(items.get(1)?)
        .ok()?
        .1
        .as_stream()
        .ok()?;
    match profile.dict.get(b"N").ok()?.as_i64().ok()? {
        1 => Some(Channels::Grey),
        3 => Some(Channels::Rgb),
        _ => None,
    }
}

/// The storage family, or `None` for a filter chain this crate does not
/// decode whole.
pub(crate) fn family(stream: &Stream) -> Option<Family> {
    if stream.dict.get(b"DecodeParms").is_ok() {
        return None;
    }

    match stream.filters() {
        // No `/Filter` at all: the content already is the samples.
        Err(_) => Some(Family::Lossless),
        Ok(filters) => match filters.as_slice() {
            [b"FlateDecode"] => Some(Family::Lossless),
            [b"DCTDecode"] => Some(Family::Jpeg),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::grey_image;
    use lopdf::dictionary;

    /// The colour space of a plain image whose dictionary has been edited.
    fn space_of(edit: impl FnOnce(&mut Document, &mut Dictionary)) -> Option<Channels> {
        let mut document = Document::with_version("1.5");
        let mut dict = grey_image(8, 8).dict;
        edit(&mut document, &mut dict);

        channels(&document, &dict)
    }

    #[test]
    fn the_device_spaces_are_read_by_name() {
        assert_eq!(space_of(|_, _| {}), Some(Channels::Grey));
        assert_eq!(
            space_of(|_, dict| dict.set("ColorSpace", "DeviceRGB")),
            Some(Channels::Rgb)
        );
    }

    /// A space may be written as a reference rather than inline, and it is
    /// the same space.
    #[test]
    fn a_colour_space_behind_a_reference_is_followed() {
        assert_eq!(
            space_of(|document, dict| {
                let id = document.add_object(Object::Name(b"DeviceRGB".to_vec()));
                dict.set("ColorSpace", id);
            }),
            Some(Channels::Rgb)
        );
    }

    /// An `/ICCBased` space is the one array form opened further, because its
    /// `/N` says the samples are ordinary.
    #[test]
    fn an_icc_based_space_is_read_by_its_component_count() {
        for (components, expected) in [(1, Some(Channels::Grey)), (3, Some(Channels::Rgb))] {
            assert_eq!(
                space_of(|document, dict| {
                    let profile = document
                        .add_object(Stream::new(dictionary! { "N" => components }, vec![0; 4]));
                    dict.set("ColorSpace", vec!["ICCBased".into(), profile.into()]);
                }),
                expected,
                "an ICC profile of {components} components"
            );
        }
    }

    /// Four components would come back from the JPEG decoder as three, and
    /// the image would be quietly rewritten in another colour space.
    #[test]
    fn a_four_component_space_is_refused() {
        assert_eq!(
            space_of(|_, dict| dict.set("ColorSpace", "DeviceCMYK")),
            None
        );
        assert_eq!(
            space_of(|document, dict| {
                let profile =
                    document.add_object(Stream::new(dictionary! { "N" => 4 }, vec![0; 4]));
                dict.set("ColorSpace", vec!["ICCBased".into(), profile.into()]);
            }),
            None
        );
    }

    /// Averaging two palette *offsets* produces a third offset, which is a
    /// colour unrelated to both.
    #[test]
    fn an_indexed_space_is_refused() {
        assert_eq!(
            space_of(|_, dict| {
                dict.set(
                    "ColorSpace",
                    vec![
                        "Indexed".into(),
                        "DeviceRGB".into(),
                        1.into(),
                        Object::string_literal("......"),
                    ],
                );
            }),
            None
        );
    }

    #[test]
    fn a_space_the_module_cannot_name_is_refused() {
        assert_eq!(
            space_of(|_, dict| dict.set("ColorSpace", "Separation")),
            None
        );
        assert_eq!(
            space_of(|_, dict| {
                dict.remove(b"ColorSpace");
            }),
            None
        );
    }

    /// The two storage families, and the shape each is recognised by.
    #[test]
    fn the_families_are_read_off_the_filter() {
        let raw = grey_image(8, 8);
        assert_eq!(family(&raw), Some(Family::Lossless));

        let mut flated = grey_image(8, 8);
        flated.dict.set("Filter", "FlateDecode");
        assert_eq!(family(&flated), Some(Family::Lossless));

        let mut jpeg = grey_image(8, 8);
        jpeg.dict.set("Filter", "DCTDecode");
        assert_eq!(family(&jpeg), Some(Family::Jpeg));
    }

    #[test]
    fn a_filter_this_crate_does_not_decode_whole_is_refused() {
        for filter in ["JPXDecode", "CCITTFaxDecode", "JBIG2Decode", "LZWDecode"] {
            let mut image = grey_image(8, 8);
            image.dict.set("Filter", filter);
            assert_eq!(family(&image), None, "{filter} was accepted");
        }

        let mut chained = grey_image(8, 8);
        chained
            .dict
            .set("Filter", vec!["ASCII85Decode".into(), "DCTDecode".into()]);
        assert_eq!(family(&chained), None);
    }

    /// A predictor means the bytes under the filter are differences between
    /// rows, not samples.
    #[test]
    fn a_stream_with_decode_parameters_is_refused() {
        let mut image = grey_image(8, 8);
        image.dict.set("Filter", "FlateDecode");
        image
            .dict
            .set("DecodeParms", dictionary! { "Predictor" => 15 });

        assert_eq!(family(&image), None);
    }
}
