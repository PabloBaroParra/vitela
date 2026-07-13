//! Annotation builders (T-027): construct pure-data
//! [`pdf_document::Annotation`] values for each standard annotation kind.
//!
//! These are intentionally "no I/O" pure functions — callers (shells, or
//! future signature composition per the roadmap's drawn-signature scope
//! change) supply already-in-memory data and get an `Annotation` back.

use crate::error::AnnotateError;
use pdf_document::{Annotation, AnnotationId, AnnotationKind, Color, PageId, Popup, Rect};

/// Builds a `Highlight` annotation over `rect`.
pub fn highlight(id: AnnotationId, page: PageId, rect: Rect, color: Color) -> Annotation {
    Annotation {
        id,
        page,
        kind: AnnotationKind::Highlight { rect, color },
    }
}

/// Builds an `Underline` annotation over `rect`.
pub fn underline(id: AnnotationId, page: PageId, rect: Rect, color: Color) -> Annotation {
    Annotation {
        id,
        page,
        kind: AnnotationKind::Underline { rect, color },
    }
}

/// Builds a `Strikeout` annotation over `rect`.
pub fn strikeout(id: AnnotationId, page: PageId, rect: Rect, color: Color) -> Annotation {
    Annotation {
        id,
        page,
        kind: AnnotationKind::Strikeout { rect, color },
    }
}

/// Builds an `Ink` (freehand) annotation from a polyline of `points`.
///
/// Reusable, bytes/points-in-annotation-out builder: the roadmap's drawn
/// signature feature composes a signature stamp from this exact ink builder
/// plus [`stamp_from_image_bytes`] — no signature-specific logic lives here.
pub fn ink(id: AnnotationId, page: PageId, points: Vec<(f64, f64)>, color: Color) -> Annotation {
    Annotation {
        id,
        page,
        kind: AnnotationKind::Ink { points, color },
    }
}

/// Builds a `Shape` annotation over `rect`.
pub fn shape(id: AnnotationId, page: PageId, rect: Rect, color: Color) -> Annotation {
    Annotation {
        id,
        page,
        kind: AnnotationKind::Shape { rect, color },
    }
}

/// Builds a `TextNote` annotation: the icon/anchor plus its nested `Popup`
/// (spec "Text Note Popup Linking"). The popup starts closed and carries the
/// same `contents` as the note; actual `/Popup` + `/Parent` PDF dictionary
/// keys (and the `/IRT` exclusion) are built by
/// [`crate::appearance::build_text_note_dicts`], not here — this builder
/// only assembles the pure-data pair that `AnnotationSet` stores.
pub fn text_note(
    id: AnnotationId,
    page: PageId,
    rect: Rect,
    contents: impl Into<String>,
) -> Annotation {
    let contents = contents.into();
    Annotation {
        id,
        page,
        kind: AnnotationKind::TextNote {
            rect,
            contents: contents.clone(),
            popup: Popup {
                is_open: false,
                contents,
            },
        },
    }
}

/// Builds a `Stamp` annotation from raw image bytes (spec "Insert Image from
/// Bytes" / "Image Stamp Annotations", T-030).
///
/// Decodes `image_bytes` in-memory only (no filesystem/network I/O) to
/// detect whether the source image carries an alpha channel — this flag
/// drives whether [`crate::appearance::build_stamp_appearance`] later emits
/// an `/SMask` soft mask when the actual `/AP` appearance stream is built.
/// The raw bytes are stored as-is on the returned `Annotation` (pure data);
/// no lopdf/appearance-stream construction happens in this builder.
///
/// Reusable, bytes-in/annotation-out builder: the roadmap's drawn signature
/// feature composes a signature stamp from this exact function plus
/// [`ink`] — no signature-specific logic lives here.
pub fn stamp_from_image_bytes(
    id: AnnotationId,
    page: PageId,
    image_bytes: &[u8],
    rect: Rect,
) -> Result<Annotation, AnnotateError> {
    let decoded = image::load_from_memory(image_bytes)
        .map_err(|e| AnnotateError::InvalidImage(e.to_string()))?;
    let has_alpha = decoded.color().has_alpha();

    Ok(Annotation {
        id,
        page,
        kind: AnnotationKind::Stamp {
            rect,
            image_bytes: image_bytes.to_vec(),
            has_alpha,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        }
    }

    fn color() -> Color {
        Color { r: 255, g: 0, b: 0 }
    }

    fn encode_png(width: u32, height: u32, has_alpha: bool) -> Vec<u8> {
        use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};
        use std::io::Cursor;

        let dynamic = if has_alpha {
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(
                width,
                height,
                image::Rgba([10, 20, 30, 128]),
            ))
        } else {
            DynamicImage::ImageRgb8(RgbImage::from_pixel(
                width,
                height,
                image::Rgb([10, 20, 30]),
            ))
        };

        let mut buf = Cursor::new(Vec::new());
        dynamic
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encode png fixture");
        buf.into_inner()
    }

    #[test]
    fn highlight_has_expected_shape() {
        let annotation = highlight(AnnotationId(1), PageId(0), rect(), color());
        assert_eq!(annotation.id, AnnotationId(1));
        assert_eq!(annotation.page, PageId(0));
        match annotation.kind {
            AnnotationKind::Highlight { rect: r, color: c } => {
                assert_eq!(r, rect());
                assert_eq!(c, color());
            }
            other => panic!("expected Highlight, got {other:?}"),
        }
    }

    #[test]
    fn underline_has_expected_shape() {
        let annotation = underline(AnnotationId(2), PageId(1), rect(), color());
        match annotation.kind {
            AnnotationKind::Underline { rect: r, color: c } => {
                assert_eq!(r, rect());
                assert_eq!(c, color());
            }
            other => panic!("expected Underline, got {other:?}"),
        }
    }

    #[test]
    fn strikeout_has_expected_shape() {
        let annotation = strikeout(AnnotationId(3), PageId(1), rect(), color());
        match annotation.kind {
            AnnotationKind::Strikeout { rect: r, color: c } => {
                assert_eq!(r, rect());
                assert_eq!(c, color());
            }
            other => panic!("expected Strikeout, got {other:?}"),
        }
    }

    #[test]
    fn ink_has_expected_shape() {
        let points = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)];
        let annotation = ink(AnnotationId(4), PageId(0), points.clone(), color());
        match annotation.kind {
            AnnotationKind::Ink {
                points: p,
                color: c,
            } => {
                assert_eq!(p, points);
                assert_eq!(c, color());
            }
            other => panic!("expected Ink, got {other:?}"),
        }
    }

    #[test]
    fn shape_has_expected_shape() {
        let annotation = shape(AnnotationId(5), PageId(0), rect(), color());
        match annotation.kind {
            AnnotationKind::Shape { rect: r, color: c } => {
                assert_eq!(r, rect());
                assert_eq!(c, color());
            }
            other => panic!("expected Shape, got {other:?}"),
        }
    }

    #[test]
    fn text_note_pairs_popup_closed_with_same_contents() {
        let annotation = text_note(AnnotationId(6), PageId(0), rect(), "hello");
        match annotation.kind {
            AnnotationKind::TextNote {
                rect: r,
                contents,
                popup,
            } => {
                assert_eq!(r, rect());
                assert_eq!(contents, "hello");
                assert_eq!(popup.contents, "hello");
                assert!(!popup.is_open);
            }
            other => panic!("expected TextNote, got {other:?}"),
        }
    }

    #[test]
    fn stamp_from_image_bytes_detects_alpha() {
        let png = encode_png(2, 2, true);
        let annotation = stamp_from_image_bytes(AnnotationId(7), PageId(0), &png, rect())
            .expect("valid png should decode");
        match annotation.kind {
            AnnotationKind::Stamp {
                rect: r,
                has_alpha,
                image_bytes,
            } => {
                assert_eq!(r, rect());
                assert!(has_alpha);
                assert_eq!(image_bytes, png);
            }
            other => panic!("expected Stamp, got {other:?}"),
        }
    }

    #[test]
    fn stamp_from_image_bytes_detects_no_alpha() {
        let png = encode_png(2, 2, false);
        let annotation = stamp_from_image_bytes(AnnotationId(8), PageId(0), &png, rect())
            .expect("valid png should decode");
        match annotation.kind {
            AnnotationKind::Stamp { has_alpha, .. } => assert!(!has_alpha),
            other => panic!("expected Stamp, got {other:?}"),
        }
    }

    #[test]
    fn stamp_from_image_bytes_rejects_garbage() {
        let result = stamp_from_image_bytes(AnnotationId(9), PageId(0), b"not an image", rect());
        assert!(matches!(result, Err(AnnotateError::InvalidImage(_))));
    }
}
