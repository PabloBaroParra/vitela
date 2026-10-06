//! Builds actual PDF-level annotation objects: the `/Popup` + `/Parent`
//! dictionary pair for text notes (T-029), and the image `/AP` appearance
//! stream (with `/SMask` alpha) for stamps (T-030).
//!
//! Object numbering (turning the placeholder `lopdf::Object::Reference`
//! values used here into real indirect object ids) is `pdf-save`'s job at
//! write time (Batch 6) — this crate only guarantees the correct *keys* and
//! *shapes* are present; it does not open, number, or write a whole
//! `lopdf::Document`.

use crate::error::AnnotateError;
use lopdf::{Dictionary, Object, Stream};
use pdf_document::{Annotation, AnnotationKind};

/// The two PDF appearance-relevant XObject streams built from a `Stamp`
/// annotation's image bytes: the main `/AP` image XObject, and — only when
/// the source image carried an alpha channel — the `/SMask` soft-mask
/// XObject referenced from it.
#[derive(Debug, Clone, PartialEq)]
pub struct StampAppearance {
    pub image_xobject: Stream,
    pub smask_xobject: Option<Stream>,
}

/// Builds the `/Popup` + `/Parent` linked pair of PDF annotation
/// dictionaries for a `TextNote` (spec "Text Note Popup Linking"): the
/// markup (icon) dict carries a `/Popup` entry; the returned popup dict
/// carries a `/Parent` back-reference to it. `/IRT` is never used for this
/// link — `/IRT` is reserved for reply-thread relationships between
/// annotations, not popup association.
pub fn build_text_note_dicts(
    annotation: &Annotation,
) -> Result<(Dictionary, Dictionary), AnnotateError> {
    let AnnotationKind::TextNote {
        rect,
        contents,
        popup,
    } = &annotation.kind
    else {
        return Err(AnnotateError::UnsupportedOperation(
            "build_text_note_dicts: not a TextNote",
        ));
    };

    let mut markup = Dictionary::new();
    markup.set("Type", "Annot");
    markup.set("Subtype", "Text");
    markup.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rect.x as f32),
            Object::Real(rect.y as f32),
            Object::Real((rect.x + rect.width) as f32),
            Object::Real((rect.y + rect.height) as f32),
        ]),
    );
    markup.set("Contents", Object::string_literal(contents.clone()));
    // Placeholder indirect reference — pdf-save assigns the real object id
    // for the popup dict and rewrites this to point at it.
    markup.set("Popup", Object::Reference((0, 0)));

    let mut popup_dict = Dictionary::new();
    popup_dict.set("Type", "Annot");
    popup_dict.set("Subtype", "Popup");
    popup_dict.set("Open", popup.is_open);
    popup_dict.set("Contents", Object::string_literal(popup.contents.clone()));
    // Back-reference to the markup annotation — `/Parent`, never `/IRT`.
    popup_dict.set("Parent", Object::Reference((0, 0)));

    Ok((markup, popup_dict))
}

/// Builds the image `/AP` appearance stream for a `Stamp` annotation,
/// decoding its stored image bytes and — when the source carried an alpha
/// channel — an accompanying `/SMask` soft-mask XObject so transparency
/// composites correctly (spec "Image Stamp Annotations" / "Insert Image
/// from Bytes", T-030).
pub fn build_stamp_appearance(annotation: &Annotation) -> Result<StampAppearance, AnnotateError> {
    let AnnotationKind::Stamp {
        image_bytes,
        has_alpha,
        ..
    } = &annotation.kind
    else {
        return Err(AnnotateError::UnsupportedOperation(
            "build_stamp_appearance: not a Stamp",
        ));
    };

    let decoded = image::load_from_memory(image_bytes)
        .map_err(|e| AnnotateError::InvalidImage(e.to_string()))?;
    let (width, height) = (decoded.width(), decoded.height());
    let rgba = decoded.to_rgba8();
    let raw = rgba.into_raw();

    let pixel_count = width as usize * height as usize;
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    let mut alpha = Vec::with_capacity(pixel_count);
    for pixel in raw.as_chunks::<4>().0 {
        rgb.extend_from_slice(&pixel[0..3]);
        alpha.push(pixel[3]);
    }

    let smask_xobject = if *has_alpha {
        let mut smask_dict = Dictionary::new();
        smask_dict.set("Type", "XObject");
        smask_dict.set("Subtype", "Image");
        smask_dict.set("Width", width);
        smask_dict.set("Height", height);
        smask_dict.set("ColorSpace", "DeviceGray");
        smask_dict.set("BitsPerComponent", 8);
        Some(flate(Stream::new(smask_dict, alpha)))
    } else {
        None
    };

    let mut image_dict = Dictionary::new();
    image_dict.set("Type", "XObject");
    image_dict.set("Subtype", "Image");
    image_dict.set("Width", width);
    image_dict.set("Height", height);
    image_dict.set("ColorSpace", "DeviceRGB");
    image_dict.set("BitsPerComponent", 8);
    if smask_xobject.is_some() {
        // Placeholder — pdf-save assigns the real indirect object id when it
        // embeds `smask_xobject` and links it here.
        image_dict.set("SMask", Object::Reference((0, 0)));
    }

    Ok(StampAppearance {
        image_xobject: flate(Stream::new(image_dict, rgb)),
        smask_xobject,
    })
}

/// `/FlateDecode`s a stamp's raw samples. Nothing downstream compresses them:
/// the writers serialize streams as given, so without this a 528x258 drawn
/// signature cost ~540 KB on disk.
///
/// `Stream::compress` only sets `/Filter` once the deflate has succeeded and
/// actually saves bytes, so a failure (impossible into a `Vec`) or a tiny
/// image leaves a valid unfiltered stream behind.
fn flate(mut stream: Stream) -> Stream {
    let _ = stream.compress();
    stream
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{AnnotationId, Color, PageId, Popup, Rect};

    fn text_note_annotation() -> Annotation {
        Annotation {
            id: AnnotationId(1),
            page: PageId(0),
            kind: AnnotationKind::TextNote {
                rect: Rect {
                    x: 1.0,
                    y: 2.0,
                    width: 10.0,
                    height: 20.0,
                },
                contents: "anchor text".to_string(),
                popup: Popup {
                    is_open: true,
                    contents: "comment body".to_string(),
                },
            },
        }
    }

    fn highlight_annotation() -> Annotation {
        Annotation {
            id: AnnotationId(2),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                color: Color { r: 0, g: 0, b: 0 },
            },
        }
    }

    fn encode_png(width: u32, height: u32, has_alpha: bool) -> Vec<u8> {
        use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};
        use std::io::Cursor;

        let dynamic = if has_alpha {
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(
                width,
                height,
                image::Rgba([100, 150, 200, 60]),
            ))
        } else {
            DynamicImage::ImageRgb8(RgbImage::from_pixel(
                width,
                height,
                image::Rgb([100, 150, 200]),
            ))
        };

        let mut buf = Cursor::new(Vec::new());
        dynamic
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encode png fixture");
        buf.into_inner()
    }

    fn stamp_annotation(has_alpha_png: bool) -> Annotation {
        let bytes = encode_png(2, 2, has_alpha_png);
        Annotation {
            id: AnnotationId(3),
            page: PageId(0),
            kind: AnnotationKind::Stamp {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 2.0,
                    height: 2.0,
                },
                image_bytes: bytes,
                has_alpha: has_alpha_png,
            },
        }
    }

    #[test]
    fn text_note_markup_has_popup_key_no_irt() {
        let (markup, _popup) = build_text_note_dicts(&text_note_annotation()).expect("valid");
        assert!(markup.has(b"Popup"));
        assert!(!markup.has(b"IRT"));
        assert_eq!(markup.get(b"Subtype").unwrap().as_name().unwrap(), b"Text");
    }

    #[test]
    fn text_note_popup_has_parent_key_no_irt() {
        let (_markup, popup) = build_text_note_dicts(&text_note_annotation()).expect("valid");
        assert!(popup.has(b"Parent"));
        assert!(!popup.has(b"IRT"));
        assert_eq!(popup.get(b"Subtype").unwrap().as_name().unwrap(), b"Popup");
        assert_eq!(popup.get(b"Open").unwrap(), &Object::Boolean(true));
    }

    #[test]
    fn build_text_note_dicts_rejects_non_text_note() {
        let result = build_text_note_dicts(&highlight_annotation());
        assert!(matches!(
            result,
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn stamp_appearance_has_smask_when_alpha() {
        let appearance = build_stamp_appearance(&stamp_annotation(true)).expect("valid");
        assert!(appearance.smask_xobject.is_some());
        assert!(appearance.image_xobject.dict.has(b"SMask"));

        let smask = appearance.smask_xobject.unwrap();
        assert_eq!(
            smask.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
            b"DeviceGray"
        );
        assert_eq!(smask.content.len(), 4); // 2x2 grayscale alpha
    }

    #[test]
    fn stamp_appearance_omits_smask_when_no_alpha() {
        let appearance = build_stamp_appearance(&stamp_annotation(false)).expect("valid");
        assert!(appearance.smask_xobject.is_none());
        assert!(!appearance.image_xobject.dict.has(b"SMask"));
    }

    #[test]
    fn stamp_appearance_dimensions_match_image() {
        let appearance = build_stamp_appearance(&stamp_annotation(true)).expect("valid");
        assert_eq!(
            appearance.image_xobject.dict.get(b"Width").unwrap(),
            &Object::Integer(2)
        );
        assert_eq!(
            appearance.image_xobject.dict.get(b"Height").unwrap(),
            &Object::Integer(2)
        );
        assert_eq!(appearance.image_xobject.content.len(), 2 * 2 * 3); // RGB8
    }

    /// A drawn signature is mostly transparent background with a few dark
    /// strokes — the shape that made a 528x258 stamp cost ~540 KB raw.
    fn signature_like_stamp(width: u32, height: u32) -> Annotation {
        use image::{DynamicImage, ImageFormat, RgbaImage};
        use std::io::Cursor;

        let canvas = RgbaImage::from_fn(width, height, |x, y| {
            let wave = (f64::from(x) / 20.0).sin() * f64::from(height) / 4.0;
            let stroke_y = f64::from(height) / 2.0 + wave;
            if (f64::from(y) - stroke_y).abs() < 3.0 {
                image::Rgba([20, 20, 60, 255])
            } else {
                image::Rgba([255, 255, 255, 0])
            }
        });
        let mut buf = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(canvas)
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encode png fixture");

        Annotation {
            id: AnnotationId(5),
            page: PageId(0),
            kind: AnnotationKind::Stamp {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: f64::from(width),
                    height: f64::from(height),
                },
                image_bytes: buf.into_inner(),
                has_alpha: true,
            },
        }
    }

    fn filter_of(stream: &Stream) -> &[u8] {
        stream
            .dict
            .get(b"Filter")
            .and_then(Object::as_name)
            .expect("stream must carry a /Filter")
    }

    #[test]
    fn stamp_image_and_smask_are_flate_compressed() {
        let appearance = build_stamp_appearance(&signature_like_stamp(528, 258)).expect("valid");
        let smask = appearance
            .smask_xobject
            .expect("alpha source keeps its mask");

        assert_eq!(filter_of(&appearance.image_xobject), b"FlateDecode");
        assert_eq!(filter_of(&smask), b"FlateDecode");
        assert!(appearance.image_xobject.content.len() < 528 * 258 * 3 / 10);
        assert!(smask.content.len() < 528 * 258 / 10);
    }

    #[test]
    fn compressed_stamp_streams_inflate_back_to_the_raw_samples() {
        let annotation = signature_like_stamp(64, 32);
        let appearance = build_stamp_appearance(&annotation).expect("valid");
        let AnnotationKind::Stamp { image_bytes, .. } = &annotation.kind else {
            unreachable!()
        };
        let rgba = image::load_from_memory(image_bytes).unwrap().to_rgba8();
        let (rgb, alpha): (Vec<_>, Vec<_>) =
            rgba.pixels().map(|p| ([p[0], p[1], p[2]], p[3])).unzip();

        assert_eq!(
            appearance.image_xobject.decompressed_content().unwrap(),
            rgb.concat()
        );
        assert_eq!(
            appearance
                .smask_xobject
                .unwrap()
                .decompressed_content()
                .unwrap(),
            alpha
        );
    }

    #[test]
    fn build_stamp_appearance_rejects_non_stamp() {
        let result = build_stamp_appearance(&text_note_annotation());
        assert!(matches!(
            result,
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn build_stamp_appearance_rejects_invalid_bytes() {
        let annotation = Annotation {
            id: AnnotationId(4),
            page: PageId(0),
            kind: AnnotationKind::Stamp {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                image_bytes: b"garbage".to_vec(),
                has_alpha: false,
            },
        };
        let result = build_stamp_appearance(&annotation);
        assert!(matches!(result, Err(AnnotateError::InvalidImage(_))));
    }
}
