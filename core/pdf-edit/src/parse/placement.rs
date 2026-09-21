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
//! ## Scope, and why the name is not resolved here
//!
//! The walk descends into **form XObjects**, so an image a form paints is a
//! placement like any other. It has to be: a form is the one place where the
//! same name means two different images, because a form's own `/Resources`
//! *replace* its caller's rather than merging. Resolving `/Im0` against the
//! page after walking into a form that redefines it reports the page's image
//! with the form's matrix — the wrong object, measured by a placement it
//! never had, which is how a caller resamples a photograph it never saw
//! drawn small. So the name is resolved by the interpreter, in the scope
//! that painted it (`LocatedImage::xobject`), and this module only measures.
//!
//! ## What is deliberately not here
//!
//! An **inline image** (`BI … ID … EI`) is invisible to this walk: the lexer
//! skips it as one opaque operation
//! (`crate::parse::lexer::skip_inline_image`), and it has no resource name to
//! report anyway. That is the existing contract, not an omission — and it is
//! why this returns *placements observed* rather than *images present*. A
//! caller that acts only on what is returned here cannot act on an image
//! whose placement it never saw, which is the safe half of the trade.

use lopdf::{Document, ObjectId};

use super::matrix::Matrix;
use crate::error::EditError;

/// One `Do` of an image XObject, and the transform that placed it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePlacement {
    /// The image XObject painted, as an addressable object — so a caller can
    /// read or replace its bytes without touching the content stream that
    /// names it.
    pub xobject: ObjectId,
    /// The key naming it in the resource dictionary that was in effect where
    /// it was painted — the page's, or a form's own.
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

/// Every image `page_object` paints, in the order it paints them —
/// including the ones painted from inside the form XObjects it invokes.
///
/// An image resource that is not an indirect object is left out: a stream
/// written directly into the resource dictionary has no id to address, so a
/// caller could not act on it even if it were reported. An **inline image**
/// is left out for the same reason — its samples are bytes inside the
/// content stream, not an object — and it is `crate::edit` that reaches
/// those, through the stream.
///
/// # Errors
///
/// [`EditError`] when the page's content streams cannot be read or tokenized,
/// or when its form invocations nest deeper than the interpreter allows.
pub fn page_image_placements(
    document: &Document,
    page_object: ObjectId,
) -> Result<Vec<ImagePlacement>, EditError> {
    let located = super::read_located_content(document, page_object)?;

    Ok(located
        .images
        .iter()
        .filter_map(|image| {
            Some(ImagePlacement {
                xobject: image.xobject?,
                resource_xobject_name: image.item.resource_xobject_name()?.to_string(),
                ctm: image.ctm_at_paint,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use lopdf::{dictionary, Dictionary, Object, Stream};

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

    /// Bind `object` under `name` in the page's `/Resources /XObject`.
    fn add_page_xobject(document: &mut Document, page: ObjectId, name: &str, object: ObjectId) {
        let resources = document
            .get_object_mut(page)
            .and_then(|object| object.as_dict_mut())
            .expect("the fixture page is a dictionary")
            .get_mut(b"Resources")
            .and_then(|object| object.as_dict_mut())
            .expect("the fixture page carries a direct resource dictionary");

        if resources.get(b"XObject").is_err() {
            resources.set("XObject", Dictionary::new());
        }
        resources
            .get_mut(b"XObject")
            .and_then(|object| object.as_dict_mut())
            .expect("the resource dictionary holds a direct /XObject")
            .set(name, Object::Reference(object));
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

    /// A form's own `/Resources` replace its caller's, so `/Im0` inside a
    /// form and `/Im0` on the page are two different images that happen to
    /// share a name — and `/Im0` is the name most producers generate. The
    /// walk descends into forms, so it has to resolve the name in the scope
    /// that painted it or it reports the wrong object with the right matrix.
    #[test]
    fn an_image_inside_a_form_resolves_against_the_forms_own_resources() {
        let (mut document, page) = document_with_content(
            b"q 100 0 0 50 0 0 cm /Im0 Do Q q 1 0 0 1 0 0 cm /Fm0 Do Q",
            image_resources("Im0"),
        );

        let inner = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 640,
                "Height" => 320,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            vec![0; 16],
        ));
        let form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "Resources" => dictionary! {
                    "XObject" => dictionary! { "Im0" => inner },
                },
            },
            b"q 20 0 0 10 0 0 cm /Im0 Do Q".to_vec(),
        ));
        add_page_xobject(&mut document, page, "Fm0", form);

        let placed = page_image_placements(&document, page).expect("the fixture page reads");

        assert_eq!(placed.len(), 2, "the page image and the form's own image");
        assert_eq!(placed[0].drawn_width(), 100.0);
        assert_eq!(placed[1].drawn_width(), 20.0);
        assert_ne!(
            placed[0].xobject, placed[1].xobject,
            "the form's /Im0 is not the page's /Im0"
        );
        assert_eq!(placed[1].xobject, inner);
    }

    /// The other half of the same divergence: a form image whose name the
    /// page cannot resolve at all was dropped on the floor, which is how an
    /// image inside a form stayed invisible to compression.
    #[test]
    fn an_image_only_a_form_can_name_is_still_a_placement() {
        let (mut document, page) = document_with_content(b"/Fm0 Do", Dictionary::new());

        let inner = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 64,
                "Height" => 32,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            vec![0; 16],
        ));
        let form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "Resources" => dictionary! {
                    "XObject" => dictionary! { "Inner" => inner },
                },
            },
            b"q 300 0 0 150 0 0 cm /Inner Do Q".to_vec(),
        ));
        add_page_xobject(&mut document, page, "Fm0", form);

        let placed = page_image_placements(&document, page).expect("the fixture page reads");

        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].xobject, inner);
        assert_eq!(placed[0].resource_xobject_name, "Inner");
        assert_eq!(placed[0].drawn_width(), 300.0);
        assert_eq!(placed[0].drawn_height(), 150.0);
    }

    /// A form's `/Matrix` is part of the transform that placed what it
    /// paints, so a placement measured from inside one has to carry it.
    #[test]
    fn a_forms_matrix_composes_into_the_placement_it_holds() {
        let (mut document, page) =
            document_with_content(b"q 2 0 0 2 0 0 cm /Fm0 Do Q", Dictionary::new());

        let inner = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 64,
                "Height" => 32,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            vec![0; 16],
        ));
        let form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "Matrix" => vec![3.into(), 0.into(), 0.into(), 3.into(), 0.into(), 0.into()],
                "Resources" => dictionary! {
                    "XObject" => dictionary! { "Inner" => inner },
                },
            },
            b"q 10 0 0 10 0 0 cm /Inner Do Q".to_vec(),
        ));
        add_page_xobject(&mut document, page, "Fm0", form);

        let placed = page_image_placements(&document, page).expect("the fixture page reads");

        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].drawn_width(), 60.0, "10 x /Matrix 3 x cm 2");
        assert_eq!(placed[0].drawn_height(), 60.0);
    }

    #[test]
    fn a_page_without_xobject_resources_paints_no_images() {
        assert!(placements(b"BT /F1 12 Tf (hello) Tj ET", Dictionary::new()).is_empty());
    }
}
