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

/// What `pdf-save` needs to write a `FreeText` annotation visibly: the
/// `/AP /N` form XObject and the `/DA` default-appearance string that
/// matches it.
#[derive(Debug, Clone, PartialEq)]
pub struct FreeTextAppearance {
    pub form: Stream,
    pub da: String,
}

/// Builds the appearance of a `FreeText` annotation from its current rect
/// and contents.
///
/// The text comes from [`crate::freetext::layout`] — the same function the
/// shells draw from — so the saved file wraps exactly where the editor did.
/// The form's `/BBox` is the rect's size with its origin at 0,0 (the `/AP`
/// is mapped onto `/Rect` by the viewer), the lines are clipped to it, and
/// nothing is stroked or filled: no border, no background. Text is written
/// as WinAnsi bytes under `/Encoding /WinAnsiEncoding`, octal-escaped so the
/// stream stays printable ASCII whatever the contents.
pub fn build_free_text_appearance(
    annotation: &Annotation,
) -> Result<FreeTextAppearance, AnnotateError> {
    let AnnotationKind::FreeText {
        rect,
        contents,
        style,
    } = &annotation.kind
    else {
        return Err(AnnotateError::UnsupportedOperation(
            "build_free_text_appearance: not a FreeText",
        ));
    };
    let laid_out = crate::freetext::layout(contents, style, rect.width, rect.height)?;

    let colour = format!(
        "{} {} {} rg",
        channel(style.color.r),
        channel(style.color.g),
        channel(style.color.b)
    );
    let size = number(style.size_pt);
    let mut content = format!(
        "q\n0 0 {} {} re W n\nBT\n{colour}\n/Helv {size} Tf\n",
        number(rect.width),
        number(rect.height)
    );
    for line in laid_out.lines.iter().filter(|l| !l.text.is_empty()) {
        let bytes = pdf_edit::encoding::winansi::encode_winansi(&line.text)
            .map_err(|character| AnnotateError::EncodingGap { character })?;
        content.push_str(&format!(
            "1 0 0 1 {} {} Tm\n({}) Tj\n",
            number(line.x_pt),
            number(rect.height - line.baseline_from_top_pt),
            escape_literal(&bytes)
        ));
    }
    content.push_str("ET\nQ\n");

    let mut helvetica = Dictionary::new();
    helvetica.set("Type", "Font");
    helvetica.set("Subtype", "Type1");
    helvetica.set("BaseFont", "Helvetica");
    helvetica.set("Encoding", "WinAnsiEncoding");
    let mut fonts = Dictionary::new();
    fonts.set("Helv", Object::Dictionary(helvetica));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));

    let mut dict = Dictionary::new();
    dict.set("Type", "XObject");
    dict.set("Subtype", "Form");
    dict.set(
        "BBox",
        Object::Array(vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(rect.width as f32),
            Object::Real(rect.height as f32),
        ]),
    );
    dict.set("Resources", Object::Dictionary(resources));

    Ok(FreeTextAppearance {
        form: Stream::new(dict, content.into_bytes()),
        da: format!("{colour} /Helv {size} Tf"),
    })
}

/// A number as PDF content-stream text: at most three decimals, no
/// trailing zeros, never `-0`.
fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".to_string()
    } else {
        text.to_string()
    }
}

fn channel(value: u8) -> String {
    number(f64::from(value) / 255.0)
}

/// The inside of a PDF literal string for `bytes`: delimiters and
/// backslashes escaped, anything outside printable ASCII as `\ooo`.
fn escape_literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(char::from(byte));
            }
            0x20..=0x7E => out.push(char::from(byte)),
            _ => out.push_str(&format!("\\{byte:03o}")),
        }
    }
    out
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
    markup.set("Contents", pdf_manip::pdf_text_string_object(contents));
    // Placeholder indirect reference — pdf-save assigns the real object id
    // for the popup dict and rewrites this to point at it.
    markup.set("Popup", Object::Reference((0, 0)));

    let mut popup_dict = Dictionary::new();
    popup_dict.set("Type", "Annot");
    popup_dict.set("Subtype", "Popup");
    popup_dict.set("Open", popup.is_open);
    popup_dict.set(
        "Contents",
        pdf_manip::pdf_text_string_object(&popup.contents),
    );
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

    fn note_with(contents: &str, popup_contents: &str) -> Annotation {
        let mut annotation = text_note_annotation();
        if let AnnotationKind::TextNote {
            contents: c, popup, ..
        } = &mut annotation.kind
        {
            *c = contents.to_string();
            popup.contents = popup_contents.to_string();
        }
        annotation
    }

    fn utf16_be_with_bom(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        bytes
    }

    #[test]
    fn non_ascii_note_contents_are_utf16_be_with_a_bom() {
        let (markup, popup) =
            build_text_note_dicts(&note_with("Ñandú €", "Ñandú €")).expect("valid");
        let expected = utf16_be_with_bom("Ñandú €");
        assert_eq!(markup.get(b"Contents").unwrap().as_str().unwrap(), expected);
        assert_eq!(popup.get(b"Contents").unwrap().as_str().unwrap(), expected);
    }

    #[test]
    fn ascii_note_contents_stay_plain_bytes() {
        let (markup, popup) = build_text_note_dicts(&note_with("plain", "body")).expect("valid");
        assert_eq!(markup.get(b"Contents").unwrap().as_str().unwrap(), b"plain");
        assert_eq!(popup.get(b"Contents").unwrap().as_str().unwrap(), b"body");
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

    fn free_text(contents: &str, width: f64, height: f64) -> Annotation {
        crate::builders::free_text(
            AnnotationId(9),
            PageId(0),
            Rect {
                x: 50.0,
                y: 60.0,
                width,
                height,
            },
            contents,
        )
        .expect("valid free text")
    }

    fn content_text(appearance: &FreeTextAppearance) -> String {
        String::from_utf8(appearance.form.content.clone()).expect("ASCII-only content stream")
    }

    /// Reads back `x y Tm` / `(literal) Tj` pairs, undoing the literal's
    /// escapes into raw WinAnsi bytes.
    fn painted_lines(content: &str) -> Vec<(Vec<u8>, f64, f64)> {
        let mut out = Vec::new();
        let mut at = (0.0, 0.0);
        for line in content.lines() {
            if let Some(rest) = line.strip_suffix(" Tm") {
                let n: Vec<f64> = rest.split(' ').map(|t| t.parse().unwrap()).collect();
                at = (n[4], n[5]);
            } else if let Some(literal) = line.strip_suffix(") Tj") {
                let body = literal.strip_prefix('(').expect("literal opens with (");
                out.push((unescape(body), at.0, at.1));
            }
        }
        out
    }

    fn unescape(body: &str) -> Vec<u8> {
        let bytes = body.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'\\' {
                out.push(bytes[i]);
                i += 1;
            } else if bytes[i + 1].is_ascii_digit() {
                let octal = std::str::from_utf8(&bytes[i + 1..i + 4]).unwrap();
                out.push(u8::from_str_radix(octal, 8).unwrap());
                i += 4;
            } else {
                out.push(bytes[i + 1]);
                i += 2;
            }
        }
        out
    }

    #[test]
    fn free_text_form_has_a_bbox_of_the_rect_size() {
        let appearance = build_free_text_appearance(&free_text("Hola", 200.0, 50.0)).unwrap();
        let bbox = appearance
            .form
            .dict
            .get(b"BBox")
            .unwrap()
            .as_array()
            .unwrap();
        let numbers: Vec<f32> = bbox.iter().map(|o| o.as_float().unwrap()).collect();
        assert_eq!(numbers, [0.0, 0.0, 200.0, 50.0]);
        assert_eq!(
            appearance
                .form
                .dict
                .get(b"Subtype")
                .unwrap()
                .as_name()
                .unwrap(),
            b"Form"
        );
    }

    #[test]
    fn free_text_form_carries_helvetica_winansi_resources() {
        let appearance = build_free_text_appearance(&free_text("Hola", 200.0, 50.0)).unwrap();
        let resources = appearance
            .form
            .dict
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap();
        let fonts = resources.get(b"Font").unwrap().as_dict().unwrap();
        let helv = fonts.get(b"Helv").unwrap().as_dict().unwrap();
        assert_eq!(helv.get(b"Subtype").unwrap().as_name().unwrap(), b"Type1");
        assert_eq!(
            helv.get(b"BaseFont").unwrap().as_name().unwrap(),
            b"Helvetica"
        );
        assert_eq!(
            helv.get(b"Encoding").unwrap().as_name().unwrap(),
            b"WinAnsiEncoding"
        );
    }

    #[test]
    fn free_text_da_names_helv_size_and_colour() {
        let appearance = build_free_text_appearance(&free_text("Hola", 200.0, 50.0)).unwrap();
        assert_eq!(appearance.da, "0 0 0 rg /Helv 12 Tf");
    }

    #[test]
    fn accented_letters_are_written_as_winansi_octal_not_utf8() {
        let appearance = build_free_text_appearance(&free_text("Canción", 200.0, 50.0)).unwrap();
        let content = content_text(&appearance);
        assert!(content.contains(r"(Canci\363n)"), "{content}");
        let painted = painted_lines(&content);
        assert_eq!(painted[0].0, [b'C', b'a', b'n', b'c', b'i', 0xF3, b'n']);
        assert!(!painted[0].0.contains(&0xC3), "no UTF-8 lead byte");
    }

    #[test]
    fn parentheses_and_backslashes_are_escaped_and_round_trip() {
        let appearance = build_free_text_appearance(&free_text(r"a(b)\c", 200.0, 50.0)).unwrap();
        let content = content_text(&appearance);
        assert!(content.contains(r"(a\(b\)\\c) Tj"), "{content}");
        assert_eq!(painted_lines(&content)[0].0, br"a(b)\c");
    }

    #[test]
    fn free_text_paints_no_border_and_no_background() {
        let appearance = build_free_text_appearance(&free_text("Hola", 200.0, 50.0)).unwrap();
        let content = content_text(&appearance);
        for painting in ["S", "s", "f", "F", "f*", "B", "B*", "b", "b*"] {
            assert!(
                !content.split_whitespace().any(|t| t == painting),
                "{painting} would draw a border or fill:\n{content}"
            );
        }
        assert!(content.contains("re W n"), "text is clipped to the box");
    }

    #[test]
    fn free_text_appearance_matches_the_layout_line_for_line() {
        let fixtures = [
            ("aaa bbb ccc ddd", 60.0, 100.0),
            ("Año ¿cómo está?\nsegunda línea", 120.0, 80.0),
            ("WWWWWWWWWW", 50.0, 200.0),
            ("Hola", 200.0, 50.0),
        ];
        for (text, width, height) in fixtures {
            let annotation = free_text(text, width, height);
            let AnnotationKind::FreeText {
                style, contents, ..
            } = &annotation.kind
            else {
                unreachable!()
            };
            let layout = crate::freetext::layout(contents, style, width, height).unwrap();
            let appearance = build_free_text_appearance(&annotation).unwrap();
            let painted = painted_lines(&content_text(&appearance));

            let expected: Vec<(Vec<u8>, f64, f64)> = layout
                .lines
                .iter()
                .map(|l| {
                    (
                        pdf_edit::encoding::winansi::encode_winansi(&l.text).unwrap(),
                        l.x_pt,
                        height - l.baseline_from_top_pt,
                    )
                })
                .collect();
            assert_eq!(painted.len(), expected.len(), "{text:?}");
            for (got, want) in painted.iter().zip(&expected) {
                assert_eq!(got.0, want.0, "{text:?}");
                assert!((got.1 - want.1).abs() < 1e-3, "{text:?} x");
                assert!((got.2 - want.2).abs() < 1e-3, "{text:?} y");
            }
        }
    }

    #[test]
    fn free_text_with_two_paragraphs_paints_two_lines() {
        let appearance = build_free_text_appearance(&free_text("a\nb", 200.0, 50.0)).unwrap();
        assert_eq!(painted_lines(&content_text(&appearance)).len(), 2);
    }

    #[test]
    fn free_text_appearance_refuses_other_kinds() {
        assert!(matches!(
            build_free_text_appearance(&highlight_annotation()),
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }
}
