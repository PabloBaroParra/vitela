//! Watermarks — the "Add watermark" tool's data model.
//!
//! A watermark is page content, not an annotation: it is painted into each
//! page's content stream so every viewer shows it and no viewer offers to
//! delete it as a comment. Like the other page-content edits, the model does
//! not hold it — `pdf-edit` writes it into the file during the save rewrite —
//! so this type is only the description of what to stamp and where.
//!
//! The pages are named by [`PageId`] rather than by position for the same
//! reason `Command::RotatePages` names them that way: nothing about a
//! watermark is a position, and the one thing it has to survive is a later
//! reorder of the very pages it stamped.
//!
//! What is stamped is a [`WatermarkMark`] — a line of text or a picture — and
//! where it goes in the paint order is a [`WatermarkLayer`]. The two are
//! independent: a text stamp can sit behind the page's own content, and a
//! picture can sit in front of it.

use std::collections::HashSet;

use crate::annotation::Color;
use crate::document::{Document, PageId};

/// The mark, look and pages of one "Add watermark" action.
#[derive(Debug, Clone, PartialEq)]
pub struct Watermark {
    /// Every page that gets the stamp. One action is one undo step, however
    /// many pages it covers.
    pub pages: Vec<PageId>,
    pub mark: WatermarkMark,
    /// Fill opacity, `0.0` exclusive to `1.0` inclusive. A fully transparent
    /// watermark is refused: it would be an edit nobody can see.
    pub opacity: f64,
    /// Counter-clockwise, as the reader sees the page. A page carrying a
    /// `/Rotate` is compensated for when the stamp is written, so `45.0`
    /// reads as the same diagonal on a portrait page and a rotated one.
    pub rotation_degrees: f64,
    pub layer: WatermarkLayer,
}

/// What a watermark paints, centred in each page's visible box.
#[derive(Debug, Clone, PartialEq)]
pub enum WatermarkMark {
    Text {
        /// Painted in Helvetica, so it must fit WinAnsi — `pdf-edit` refuses
        /// a character outside it rather than drawing a missing glyph.
        text: String,
        /// In points.
        font_size: f64,
        color: Color,
    },
    Image {
        /// The encoded picture, PNG or JPEG, exactly as the user picked it.
        /// Decoding is `pdf-edit`'s job; an alpha channel is kept.
        bytes: Vec<u8>,
        /// How much of the page the picture may cover, `0.0` exclusive to
        /// `1.0` inclusive: it is fitted, aspect kept, inside the visible box
        /// shrunk by this factor — `1.0` touches two opposite edges.
        scale: f64,
    },
}

/// Where a watermark sits in the page's paint order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WatermarkLayer {
    /// Painted after the page's own content, so nothing covers it. The only
    /// choice that shows on a scanned page, whose content is one opaque image.
    #[default]
    OverContent,
    /// Painted before the page's own content, as a background: text and
    /// pictures on the page cover it wherever they are opaque.
    UnderContent,
}

impl Watermark {
    /// The look every shell starts from: a large grey diagonal at 30%
    /// opacity. Kept in the core so the three shells' first stamp matches.
    pub const DEFAULT_FONT_SIZE: f64 = 60.0;
    pub const DEFAULT_COLOR: Color = Color {
        r: 128,
        g: 128,
        b: 128,
    };
    pub const DEFAULT_OPACITY: f64 = 0.3;
    pub const DEFAULT_ROTATION_DEGREES: f64 = 45.0;
    /// A picture starts at half the page, upright: a logo turned 45° reads
    /// as a mistake, where diagonal text reads as a stamp.
    pub const DEFAULT_IMAGE_SCALE: f64 = 0.5;
    pub const DEFAULT_IMAGE_ROTATION_DEGREES: f64 = 0.0;

    /// A watermark showing `text` on `pages` with the default look, over
    /// the page's content.
    pub fn diagonal(pages: Vec<PageId>, text: impl Into<String>) -> Self {
        Self {
            pages,
            mark: WatermarkMark::Text {
                text: text.into(),
                font_size: Self::DEFAULT_FONT_SIZE,
                color: Self::DEFAULT_COLOR,
            },
            opacity: Self::DEFAULT_OPACITY,
            rotation_degrees: Self::DEFAULT_ROTATION_DEGREES,
            layer: WatermarkLayer::OverContent,
        }
    }

    /// A watermark showing the picture `bytes` on `pages` with the default
    /// look: upright, half the page, faded, **behind** the page's content —
    /// a picture is asked for as a background far more often than as a
    /// stamp over the text.
    pub fn picture(pages: Vec<PageId>, bytes: Vec<u8>) -> Self {
        Self {
            pages,
            mark: WatermarkMark::Image {
                bytes,
                scale: Self::DEFAULT_IMAGE_SCALE,
            },
            opacity: Self::DEFAULT_OPACITY,
            rotation_degrees: Self::DEFAULT_IMAGE_ROTATION_DEGREES,
            layer: WatermarkLayer::UnderContent,
        }
    }

    /// Whether this watermark can be recorded against `document`.
    ///
    /// Refuses blank text, a non-positive or non-finite size, an empty
    /// picture, a scale outside `(0, 1]`, an opacity outside `(0, 1]`, a
    /// non-finite angle, and a page list that is empty, repeats a page, or
    /// names one the document does not hold. Whether the text can be
    /// *encoded*, or the picture *decoded*, is `pdf-edit`'s question, not this
    /// one: the model knows nothing about fonts or image formats.
    pub fn can_apply_to(&self, document: &Document) -> bool {
        self.has_valid_look() && self.names_known_pages(document)
    }

    fn has_valid_look(&self) -> bool {
        let mark_is_valid = match &self.mark {
            WatermarkMark::Text {
                text, font_size, ..
            } => !text.trim().is_empty() && font_size.is_finite() && *font_size > 0.0,
            WatermarkMark::Image { bytes, scale } => !bytes.is_empty() && is_unit_fraction(*scale),
        };
        mark_is_valid && is_unit_fraction(self.opacity) && self.rotation_degrees.is_finite()
    }

    fn names_known_pages(&self, document: &Document) -> bool {
        if self.pages.is_empty() {
            return false;
        }
        let mut seen = HashSet::new();
        self.pages
            .iter()
            .all(|page| seen.insert(*page) && document.pages.iter().any(|held| held.id == *page))
    }
}

/// `(0, 1]` — a NaN fails every comparison and so is refused too.
fn is_unit_fraction(value: f64) -> bool {
    value > 0.0 && value <= 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Orientation, Page, PageSize};

    fn two_page_document() -> Document {
        let mut document = Document::blank();
        for id in [0, 1] {
            document
                .pages
                .push(Page::blank(PageId(id), PageSize::A4, Orientation::Portrait));
        }
        document
    }

    fn with_text_mark(base: &Watermark, text: &str, font_size: f64) -> Watermark {
        Watermark {
            mark: WatermarkMark::Text {
                text: text.to_string(),
                font_size,
                color: Watermark::DEFAULT_COLOR,
            },
            ..base.clone()
        }
    }

    #[test]
    fn the_default_look_applies_to_pages_the_document_holds() {
        let watermark = Watermark::diagonal(vec![PageId(0), PageId(1)], "DRAFT");

        assert!(watermark.can_apply_to(&two_page_document()));
        assert_eq!(watermark.layer, WatermarkLayer::OverContent);
    }

    #[test]
    fn a_look_nobody_could_see_or_draw_is_refused() {
        let document = two_page_document();
        let base = Watermark::diagonal(vec![PageId(0)], "DRAFT");

        let refused = [
            with_text_mark(&base, "   ", Watermark::DEFAULT_FONT_SIZE),
            with_text_mark(&base, "DRAFT", 0.0),
            with_text_mark(&base, "DRAFT", f64::NAN),
            Watermark {
                opacity: 0.0,
                ..base.clone()
            },
            Watermark {
                opacity: 1.5,
                ..base.clone()
            },
            Watermark {
                opacity: f64::NAN,
                ..base.clone()
            },
            Watermark {
                rotation_degrees: f64::INFINITY,
                ..base.clone()
            },
        ];

        for watermark in refused {
            assert!(!watermark.can_apply_to(&document), "{watermark:?}");
        }
    }

    #[test]
    fn a_page_list_that_is_empty_repeated_or_unknown_is_refused() {
        let document = two_page_document();

        for pages in [
            vec![],
            vec![PageId(0), PageId(0)],
            vec![PageId(0), PageId(7)],
        ] {
            let watermark = Watermark::diagonal(pages.clone(), "DRAFT");
            assert!(!watermark.can_apply_to(&document), "{pages:?}");
        }
    }

    #[test]
    fn full_opacity_is_allowed() {
        let watermark = Watermark {
            opacity: 1.0,
            ..Watermark::diagonal(vec![PageId(1)], "COPY")
        };

        assert!(watermark.can_apply_to(&two_page_document()));
    }

    #[test]
    fn a_picture_starts_upright_and_behind_the_content() {
        let watermark = Watermark::picture(vec![PageId(0)], vec![1, 2, 3]);

        assert!(watermark.can_apply_to(&two_page_document()));
        assert_eq!(watermark.layer, WatermarkLayer::UnderContent);
        assert_eq!(
            watermark.rotation_degrees,
            Watermark::DEFAULT_IMAGE_ROTATION_DEGREES
        );
    }

    /// The model cannot tell a PNG from noise — that is `pdf-edit`'s check —
    /// but it can refuse a picture with no bytes or a size nobody could see.
    #[test]
    fn a_picture_with_no_bytes_or_an_impossible_scale_is_refused() {
        let document = two_page_document();
        let picture = |bytes: Vec<u8>, scale: f64| Watermark {
            mark: WatermarkMark::Image { bytes, scale },
            ..Watermark::picture(vec![PageId(0)], Vec::new())
        };

        for watermark in [
            picture(Vec::new(), 0.5),
            picture(vec![1], 0.0),
            picture(vec![1], 1.01),
            picture(vec![1], f64::NAN),
        ] {
            assert!(!watermark.can_apply_to(&document), "{watermark:?}");
        }
        assert!(picture(vec![1], 1.0).can_apply_to(&document));
    }

    #[test]
    fn either_mark_can_go_on_either_layer() {
        let document = two_page_document();

        for layer in [WatermarkLayer::OverContent, WatermarkLayer::UnderContent] {
            let text = Watermark {
                layer,
                ..Watermark::diagonal(vec![PageId(0)], "DRAFT")
            };
            let picture = Watermark {
                layer,
                ..Watermark::picture(vec![PageId(0)], vec![1])
            };
            assert!(text.can_apply_to(&document), "{layer:?}");
            assert!(picture.can_apply_to(&document), "{layer:?}");
        }
    }
}
