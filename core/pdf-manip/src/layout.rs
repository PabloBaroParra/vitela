//! Page sizes as a viewer lays them out **now**, pending edits included.
//!
//! [`LopdfDocument::page_dimensions`] answers for the bytes as they were
//! opened. A session's model can have moved on since — pages reordered,
//! inserted, or turned by a rotation that is still only pending — and every
//! facade (the UniFFI boundary, the browser one) has to report the size a
//! shell should lay out for each page in its current order. That answer
//! needs both halves: the opened sizes from the lopdf base, and the model's
//! pages from `pdf_document`. This crate is the lowest one that sees both,
//! so the rule lives here rather than in each facade.

use pdf_document::{Document, Orientation, PageOrigin, Rotation};

use crate::document::LopdfDocument;

/// Each base page's rotation as the file was opened, indexed like the base
/// document's pages.
///
/// [`LopdfDocument::page_dimensions`] already has that turn baked in, so
/// [`current_page_dimensions`] needs it to tell whether a pending rotation
/// has put a page on its side since. Capture it once, right after opening,
/// before any edit exists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenedRotations(Vec<Rotation>);

impl OpenedRotations {
    /// Records every page's rotation from a just-opened `document`.
    ///
    /// Only meaningful before any edit is applied: at open every page is a
    /// base page sitting at its own base index, so position and base index
    /// agree.
    pub fn capture(document: &Document) -> Self {
        Self(document.pages.iter().map(|page| page.rotation).collect())
    }

    fn get(&self, page_index: u32) -> Option<Rotation> {
        self.0.get(page_index as usize).copied()
    }
}

/// One page's size in PDF points as a viewer should lay it out, plus the
/// turn it is drawn at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentPageDimensions {
    pub width_pt: f64,
    pub height_pt: f64,
    pub rotation: Rotation,
}

/// Every page's size in points, in the order `document` has them **now**.
///
/// Built from the model rather than asked of a renderer so that an untouched
/// document reports exactly what `base` does: a base page keeps the size
/// `base` gives it, and only swaps width and height when a pending rotation
/// has turned it a quarter from how it opened. A page with no base
/// counterpart (blank, imported, or a base index `base` does not have) is
/// sized from its own recorded paper size and orientation.
pub fn current_page_dimensions(
    document: &Document,
    base: &LopdfDocument,
    opened: &OpenedRotations,
) -> Vec<CurrentPageDimensions> {
    let base_dimensions = base.page_dimensions();
    document
        .pages
        .iter()
        .map(|page| {
            let from_base = match page.origin {
                PageOrigin::Base { page_index } => base_dimensions
                    .get(page_index as usize)
                    .zip(opened.get(page_index)),
                _ => None,
            };
            let ((width_pt, height_pt), opened_rotation) = match from_base {
                Some((dimensions, rotation)) => {
                    ((dimensions.width_pt, dimensions.height_pt), rotation)
                }
                None => {
                    let (width, height) = page.size.dimensions_pt();
                    let upright = match page.orientation {
                        Orientation::Portrait => (width, height),
                        Orientation::Landscape => (height, width),
                    };
                    (upright, Rotation::None)
                }
            };
            let rotation = page.rotation;
            if is_sideways(rotation) == is_sideways(opened_rotation) {
                CurrentPageDimensions {
                    width_pt,
                    height_pt,
                    rotation,
                }
            } else {
                CurrentPageDimensions {
                    width_pt: height_pt,
                    height_pt: width_pt,
                    rotation,
                }
            }
        })
        .collect()
}

fn is_sideways(rotation: Rotation) -> bool {
    matches!(rotation, Rotation::Clockwise90 | Rotation::Clockwise270)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Page, PageId, PageSize};

    const A4: (f64, f64) = (595.0, 842.0);
    const LETTER: (f64, f64) = (612.0, 792.0);

    /// A two-page A4 base, the first page carrying `/Rotate first_rotate`.
    fn base(first_rotate: i64) -> LopdfDocument {
        let blank = crate::create_blank_document(PageSize::A4, Orientation::Portrait);
        let one = crate::insert_blank_page(&blank, 0, PageSize::A4, Orientation::Portrait)
            .expect("first page should insert");
        let mut two = crate::insert_blank_page(&one, 1, PageSize::A4, Orientation::Portrait)
            .expect("second page should insert");
        let first = *two
            .as_lopdf()
            .get_pages()
            .values()
            .next()
            .expect("base has pages");
        two.as_lopdf_mut()
            .get_dictionary_mut(first)
            .expect("page dictionary should exist")
            .set("Rotate", first_rotate);
        two
    }

    /// The model `pdf-save` would populate for [`base`]: one base page per
    /// base index, carrying the rotation the file opened with.
    fn model(first_rotation: Rotation) -> Document {
        Document::with_pages(vec![
            Page::base(
                PageId(0),
                0,
                PageSize::A4,
                Orientation::Portrait,
                first_rotation,
            ),
            Page::base(
                PageId(1),
                1,
                PageSize::A4,
                Orientation::Portrait,
                Rotation::None,
            ),
        ])
    }

    fn dims(width_pt: f64, height_pt: f64, rotation: Rotation) -> CurrentPageDimensions {
        CurrentPageDimensions {
            width_pt,
            height_pt,
            rotation,
        }
    }

    #[test]
    fn an_untouched_document_reports_the_base_sizes() {
        let base = base(0);
        let document = model(Rotation::None);
        let opened = OpenedRotations::capture(&document);

        assert_eq!(
            current_page_dimensions(&document, &base, &opened),
            vec![dims(A4.0, A4.1, Rotation::None); 2]
        );
    }

    #[test]
    fn a_page_opened_sideways_is_not_swapped_twice() {
        let base = base(90);
        let document = model(Rotation::Clockwise90);
        let opened = OpenedRotations::capture(&document);

        assert_eq!(
            current_page_dimensions(&document, &base, &opened)[0],
            dims(A4.1, A4.0, Rotation::Clockwise90)
        );
    }

    #[test]
    fn a_pending_quarter_turn_swaps_width_and_height() {
        let base = base(0);
        let mut document = model(Rotation::None);
        let opened = OpenedRotations::capture(&document);
        document.pages[0].rotation = Rotation::Clockwise270;

        assert_eq!(
            current_page_dimensions(&document, &base, &opened),
            vec![
                dims(A4.1, A4.0, Rotation::Clockwise270),
                dims(A4.0, A4.1, Rotation::None),
            ]
        );
    }

    #[test]
    fn a_pending_turn_that_stands_a_sideways_page_up_swaps_it_back() {
        let base = base(90);
        let mut document = model(Rotation::Clockwise90);
        let opened = OpenedRotations::capture(&document);
        document.pages[0].rotation = Rotation::Clockwise180;

        assert_eq!(
            current_page_dimensions(&document, &base, &opened)[0],
            dims(A4.0, A4.1, Rotation::Clockwise180)
        );
    }

    #[test]
    fn a_pending_half_turn_keeps_the_axes() {
        let base = base(0);
        let mut document = model(Rotation::None);
        let opened = OpenedRotations::capture(&document);
        document.pages[0].rotation = Rotation::Clockwise180;

        assert_eq!(
            current_page_dimensions(&document, &base, &opened)[0],
            dims(A4.0, A4.1, Rotation::Clockwise180)
        );
    }

    #[test]
    fn pages_come_back_in_the_current_order() {
        let base = base(90);
        let mut document = model(Rotation::Clockwise90);
        let opened = OpenedRotations::capture(&document);
        document.pages.reverse();

        assert_eq!(
            current_page_dimensions(&document, &base, &opened),
            vec![
                dims(A4.0, A4.1, Rotation::None),
                dims(A4.1, A4.0, Rotation::Clockwise90),
            ]
        );
    }

    #[test]
    fn a_page_without_a_base_is_sized_from_its_paper_and_orientation() {
        let base = base(0);
        let mut document = model(Rotation::None);
        let opened = OpenedRotations::capture(&document);
        document.pages.push(Page::blank(
            PageId(2),
            PageSize::Letter,
            Orientation::Landscape,
        ));
        let mut turned = Page::blank(PageId(3), PageSize::Letter, Orientation::Portrait);
        turned.rotation = Rotation::Clockwise90;
        document.pages.push(turned);

        let dimensions = current_page_dimensions(&document, &base, &opened);
        assert_eq!(dimensions[2], dims(LETTER.1, LETTER.0, Rotation::None));
        assert_eq!(
            dimensions[3],
            dims(LETTER.1, LETTER.0, Rotation::Clockwise90)
        );
    }

    #[test]
    fn a_base_index_the_base_lacks_falls_back_to_the_model_size() {
        let base = base(0);
        let mut document = model(Rotation::None);
        let opened = OpenedRotations::capture(&document);
        document.pages.push(Page::base(
            PageId(2),
            7,
            PageSize::Letter,
            Orientation::Portrait,
            Rotation::None,
        ));

        assert_eq!(
            current_page_dimensions(&document, &base, &opened)[2],
            dims(LETTER.0, LETTER.1, Rotation::None)
        );
    }
}
