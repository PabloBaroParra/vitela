//! Text watermarks — the "Add watermark" tool's data model.
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

use std::collections::HashSet;

use crate::annotation::Color;
use crate::document::{Document, PageId};

/// The text, look and pages of one "Add watermark" action.
#[derive(Debug, Clone, PartialEq)]
pub struct Watermark {
    /// Every page that gets the stamp. One action is one undo step, however
    /// many pages it covers.
    pub pages: Vec<PageId>,
    /// Painted in Helvetica, so it must fit WinAnsi — `pdf-edit` refuses a
    /// character outside it rather than drawing a missing glyph.
    pub text: String,
    /// In points.
    pub font_size: f64,
    pub color: Color,
    /// Fill opacity, `0.0` exclusive to `1.0` inclusive. A fully transparent
    /// watermark is refused: it would be an edit nobody can see.
    pub opacity: f64,
    /// Counter-clockwise, as the reader sees the page. A page carrying a
    /// `/Rotate` is compensated for when the stamp is written, so `45.0`
    /// reads as the same diagonal on a portrait page and a rotated one.
    pub rotation_degrees: f64,
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

    /// A watermark showing `text` on `pages` with the default look.
    pub fn diagonal(pages: Vec<PageId>, text: impl Into<String>) -> Self {
        Self {
            pages,
            text: text.into(),
            font_size: Self::DEFAULT_FONT_SIZE,
            color: Self::DEFAULT_COLOR,
            opacity: Self::DEFAULT_OPACITY,
            rotation_degrees: Self::DEFAULT_ROTATION_DEGREES,
        }
    }

    /// Whether this watermark can be recorded against `document`.
    ///
    /// Refuses blank text, a non-positive or non-finite size, an opacity
    /// outside `(0, 1]`, a non-finite angle, and a page list that is empty,
    /// repeats a page, or names one the document does not hold. Whether the
    /// text can be *encoded* is `pdf-edit`'s question, not this one: the
    /// model knows nothing about fonts.
    pub fn can_apply_to(&self, document: &Document) -> bool {
        self.has_valid_look() && self.names_known_pages(document)
    }

    fn has_valid_look(&self) -> bool {
        !self.text.trim().is_empty()
            && self.font_size.is_finite()
            && self.font_size > 0.0
            && self.opacity.is_finite()
            && self.opacity > 0.0
            && self.opacity <= 1.0
            && self.rotation_degrees.is_finite()
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

    #[test]
    fn the_default_look_applies_to_pages_the_document_holds() {
        let watermark = Watermark::diagonal(vec![PageId(0), PageId(1)], "DRAFT");

        assert!(watermark.can_apply_to(&two_page_document()));
    }

    #[test]
    fn a_look_nobody_could_see_or_draw_is_refused() {
        let document = two_page_document();
        let base = Watermark::diagonal(vec![PageId(0)], "DRAFT");

        let refused = [
            Watermark {
                text: "   ".to_string(),
                ..base.clone()
            },
            Watermark {
                font_size: 0.0,
                ..base.clone()
            },
            Watermark {
                font_size: f64::NAN,
                ..base.clone()
            },
            Watermark {
                opacity: 0.0,
                ..base.clone()
            },
            Watermark {
                opacity: 1.5,
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
}
