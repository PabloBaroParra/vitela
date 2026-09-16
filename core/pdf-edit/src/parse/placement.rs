//! Where a page actually paints its images.
//!
//! ## The rule this module exists to keep
//!
//! *The size an image is drawn at is a property of the matrix that drew it,
//! not of the box around the result.*
//!
//! [`crate::parse::read_page_content`] already hands a shell every image on a
//! page, each with the `bbox` it occupies — and that is the right answer for
//! hit-testing, which is what a shell does with it. It is the wrong answer for
//! *measuring*: `bbox` is the axis-aligned box covering the placed unit
//! square, so an image turned a quarter-turn reports its height as its width,
//! and one placed under a skew reports a box larger than anything that was
//! painted. A caller sizing pixels against paper — `pdf-compress`, batch 24
//! task T-193 — would resample by the wrong factor on every rotated
//! photograph in the document.
//!
//! So this hands back the CTM itself. The image occupies the unit square
//! mapped through it (PDF 32000-1 8.9.5.2), which makes the drawn width the
//! length of the transformed *u* axis and the drawn height the length of the
//! transformed *v* axis — both correct under rotation, because a rotation
//! does not change a length.
//!
//! ## What is deliberately not here
//!
//! An image painted from **inside a form XObject's** own content stream, and
//! an **inline image** (`BI … ID … EI`), are both invisible to this walk: the
//! interpreter treats a form as an opaque `Do` and the lexer skips an inline
//! image as one opaque operation (`crate::parse::lexer::skip_inline_image`).
//! That is the existing contract, not an omission — and it is why this
//! returns *placements observed* rather than *images present*. A caller that
//! acts only on what is returned here cannot act on an image whose placement
//! it never saw, which is the safe half of the trade.

use lopdf::{Document, Object, ObjectId};

use super::matrix::Matrix;
use crate::error::EditError;

/// One `Do` of an image XObject, and the transform that placed it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePlacement {
    /// The image XObject painted, as an addressable object — so a caller can
    /// read or replace its bytes without touching the content stream that
    /// names it.
    pub xobject: ObjectId,
    /// The key naming it in the page's `/Resources /XObject`.
    pub resource_xobject_name: String,
    /// The CTM in effect at the `Do`.
    pub ctm: Matrix,
}

impl ImagePlacement {
    /// The width this placement draws the image at, in user-space units
    /// (1/72 inch), whatever rotation or reflection the matrix also applies.
    ///
    /// Negative scale is a reflection, and a reflected image is drawn just as
    /// wide as an unreflected one — which is why this is a length and not
    /// `ctm.a`.
    pub fn drawn_width(&self) -> f64 {
        self.ctm.a.hypot(self.ctm.b)
    }

    /// The height this placement draws the image at, in user-space units.
    pub fn drawn_height(&self) -> f64 {
        self.ctm.c.hypot(self.ctm.d)
    }
}

/// Every image `page_object` paints, in the order it paints them.
///
/// An image resource that is not an indirect object is left out: a stream
/// written directly into the resource dictionary has no id to address, so a
/// caller could not act on it even if it were reported.
///
/// # Errors
///
/// [`EditError`] when the page's content streams cannot be read or tokenized.
pub fn page_image_placements(
    document: &Document,
    page_object: ObjectId,
) -> Result<Vec<ImagePlacement>, EditError> {
    let located = super::read_located_content(document, page_object)?;
    let page_dict = document.get_dictionary(page_object)?;
    let resources = super::page_resources(document, page_dict);

    let Some(Object::Dictionary(xobjects)) = resources
        .get(b"XObject")
        .ok()
        .and_then(|object| super::dereference(document, object))
    else {
        return Ok(Vec::new());
    };

    Ok(located
        .images
        .iter()
        .filter_map(|image| {
            let name = &image.item.resource_xobject_name;
            let xobject = xobjects
                .get(name.as_bytes())
                .ok()
                .and_then(|entry| entry.as_reference().ok())?;

            Some(ImagePlacement {
                xobject,
                resource_xobject_name: name.clone(),
                ctm: image.ctm_at_paint,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use lopdf::{dictionary, Dictionary, Stream};

    use super::*;
    use crate::fixture::document_with_content;

    /// A resource dictionary holding one image XObject under `name`.
    fn image_resources(name: &str) -> Dictionary {
        dictionary! {
            "XObject" => dictionary! {
                name => Stream::new(
                    dictionary! {
                        "Type" => "XObject",
                        "Subtype" => "Image",
                        "Width" => 64,
                        "Height" => 32,
                        "ColorSpace" => "DeviceRGB",
                        "BitsPerComponent" => 8,
                    },
                    vec![0; 64 * 32 * 3],
                ),
            },
        }
    }

    fn placements(content: &[u8], resources: Dictionary) -> Vec<ImagePlacement> {
        let (document, page) = document_with_content(content, resources);
        page_image_placements(&document, page).expect("the fixture page reads")
    }

    #[test]
    fn a_page_that_paints_an_image_reports_the_transform_that_placed_it() {
        let placed = placements(b"q 200 0 0 100 50 60 cm /Im0 Do Q", image_resources("Im0"));

        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].resource_xobject_name, "Im0");
        assert_eq!(placed[0].drawn_width(), 200.0);
        assert_eq!(placed[0].drawn_height(), 100.0);
    }

    /// The reason this module exists rather than the `bbox` every shell
    /// already gets. Quarter-turned, the same 200×100 placement covers a
    /// 100×200 box — and a caller measuring pixels against that box would
    /// resample by the reciprocal of the right factor.
    #[test]
    fn a_rotated_placement_reports_the_size_it_draws_not_the_box_it_covers() {
        let placed = placements(b"q 0 200 -100 0 50 60 cm /Im0 Do Q", image_resources("Im0"));

        assert_eq!(placed[0].drawn_width(), 200.0);
        assert_eq!(placed[0].drawn_height(), 100.0);

        let bbox = placed[0].ctm.bounding_box(0.0, 0.0, 1.0, 1.0);
        assert_eq!(bbox.width, 100.0, "the box is the transposed one");
        assert_eq!(bbox.height, 200.0);
    }

    /// A mirrored image is drawn just as wide as an unmirrored one, so the
    /// measurement is a length rather than the raw coefficient.
    #[test]
    fn a_mirrored_placement_has_a_positive_size() {
        let placed = placements(
            b"q -200 0 0 100 250 60 cm /Im0 Do Q",
            image_resources("Im0"),
        );

        assert_eq!(placed[0].drawn_width(), 200.0);
    }

    #[test]
    fn an_image_painted_twice_is_reported_twice() {
        let placed = placements(
            b"q 200 0 0 100 0 0 cm /Im0 Do Q q 400 0 0 200 0 200 cm /Im0 Do Q",
            image_resources("Im0"),
        );

        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].xobject, placed[1].xobject);
        assert_eq!(placed[0].drawn_width(), 200.0);
        assert_eq!(placed[1].drawn_width(), 400.0);
    }

    /// `cm` composes, so a placement nested inside an outer scale is drawn at
    /// the product — which is the whole reason the CTM is tracked rather than
    /// the operands of the nearest `cm` being read.
    #[test]
    fn a_nested_transform_composes_before_it_is_measured() {
        let placed = placements(
            b"q 2 0 0 3 0 0 cm q 200 0 0 100 0 0 cm /Im0 Do Q Q",
            image_resources("Im0"),
        );

        assert_eq!(placed[0].drawn_width(), 400.0);
        assert_eq!(placed[0].drawn_height(), 300.0);
    }

    /// Form XObjects share the `Do` operator and the `/XObject` dictionary
    /// with images, and are not images.
    #[test]
    fn a_form_xobject_is_not_an_image_placement() {
        let resources = dictionary! {
            "XObject" => dictionary! {
                "Fm0" => Stream::new(
                    dictionary! {
                        "Type" => "XObject",
                        "Subtype" => "Form",
                        "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
                    },
                    b"0 0 10 10 re f".to_vec(),
                ),
            },
        };

        assert!(placements(b"q 200 0 0 100 0 0 cm /Fm0 Do Q", resources).is_empty());
    }

    /// An image only this page's resources can name, painted by nothing, is
    /// not a placement — and this is the case that keeps a caller honest
    /// about images it cannot see, since a form's own content stream and an
    /// inline image both look exactly like this from out here.
    #[test]
    fn an_image_that_is_declared_but_never_painted_is_not_a_placement() {
        assert!(placements(b"0 0 10 10 re f", image_resources("Im0")).is_empty());
    }

    /// A stream written straight into the resource dictionary has no object
    /// id, so there is nothing a caller could address — reporting it would be
    /// reporting an image that cannot be acted on.
    #[test]
    fn an_image_that_is_not_an_indirect_object_is_left_out() {
        let (mut document, page) =
            document_with_content(b"q 200 0 0 100 0 0 cm /Im0 Do Q", image_resources("Im0"));

        let direct = dictionary! {
            "XObject" => dictionary! {
                "Im0" => Stream::new(
                    dictionary! {
                        "Type" => "XObject",
                        "Subtype" => "Image",
                        "Width" => 64,
                        "Height" => 32,
                    },
                    vec![0; 8],
                ),
            },
        };
        document
            .get_object_mut(page)
            .and_then(|object| object.as_dict_mut())
            .expect("the fixture page is a dictionary")
            .set("Resources", direct);

        assert!(page_image_placements(&document, page)
            .expect("the page still reads")
            .is_empty());
    }

    #[test]
    fn a_page_without_xobject_resources_paints_no_images() {
        assert!(placements(b"BT /F1 12 Tf (hello) Tj ET", Dictionary::new()).is_empty());
    }
}
