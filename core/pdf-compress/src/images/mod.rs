//! Every image the document actually draws, measured against the paper.
//!
//! ## The rule this module exists to keep
//!
//! *An image this pass never saw drawn is an image this crate never touches.*
//!
//! T-193 is the measuring half of the image stage; T-194 is the half that
//! resamples. Splitting them that way is not ceremony — deciding *which*
//! images may be touched is the decision that can destroy a document, and it
//! is worth making on its own, against tests that do not have to encode a
//! single JPEG to run.
//!
//! The inventory is built from placements, not from resource dictionaries.
//! That distinction is the safety property. An image can be present in a
//! document and invisible to this walk in two ways, both of them normal:
//! painted from inside a **form XObject's** own content stream, which the
//! interpreter treats as one opaque `Do`, or written as an **inline image**,
//! which the lexer skips whole (`docs/batch-compress.md` fact 8). Neither can
//! be measured, because the matrix that places it was never seen. Building
//! the inventory from `/Resources /XObject` would list those images anyway,
//! with no placement behind them — and the natural reading of "no placement"
//! is "nothing constrains it", which is the reading that resamples a
//! photograph down to nothing. Built from placements, they are simply absent,
//! and absent is untouchable.
//!
//! A `/SMask` is invisible for the same reason and by the same luck: it hangs
//! off its parent image's dictionary and is never itself the operand of a
//! `Do`. T-194 reaches one through the image it belongs to, which is the only
//! way it may be resampled at all — alone, it would lose the alignment with
//! its parent that makes it a mask.
//!
//! ## What it costs
//!
//! One interpreter walk per page, and only under a preset that has an image
//! policy at all — [`crate::CompressPreset::Lossless`] never asks. A page
//! whose content stream cannot be tokenized contributes nothing rather than
//! failing the compression: a malformed page is a reason to leave that page's
//! images alone, not a reason to hand the user back a file they cannot
//! shrink.

pub(crate) mod dpi;

use std::collections::BTreeMap;

use lopdf::{Document, ObjectId};

use crate::report::Work;
use dpi::EffectiveDpi;

/// One image XObject, as the document draws it.
///
/// Fields beyond the count are T-194's: it resamples by `governing_dpi`
/// against a preset's target, rewrites the stream `id` names, and needs
/// `pixels` to know what it is scaling from. They are measured here because
/// measuring is the part that has to be right.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlacedImage {
    /// The XObject's object id — what T-194 swaps the bytes of.
    pub(crate) id: ObjectId,
    /// `/Width` × `/Height`, the samples the stream actually holds.
    pub(crate) pixels: (u32, u32),
    /// The resolution the largest placement in the document demands.
    pub(crate) governing_dpi: EffectiveDpi,
    /// How many times the document paints it.
    pub(crate) placements: usize,
}

/// The images `document` paints, ordered by object id.
///
/// Ordered so that two runs over the same document produce the same list:
/// this feeds a stage that rewrites streams, and a stage whose input order
/// depends on a hash map is a stage whose output bytes do too.
pub(crate) fn inventory(document: &Document) -> Vec<PlacedImage> {
    let mut found: BTreeMap<ObjectId, PlacedImage> = BTreeMap::new();

    for page in document.get_pages().values() {
        // A page that cannot be tokenized is a page whose images keep their
        // pixels. See this module's header.
        let Ok(placements) = pdf_edit::page_image_placements(document, *page) else {
            continue;
        };

        for placement in placements {
            let Some(pixels) = pixel_size(document, placement.xobject) else {
                continue;
            };
            let Some(measured) =
                EffectiveDpi::measure(pixels, (placement.drawn_width(), placement.drawn_height()))
            else {
                continue;
            };

            found
                .entry(placement.xobject)
                .and_modify(|image| {
                    image.governing_dpi = image.governing_dpi.governed_by(measured);
                    image.placements += 1;
                })
                .or_insert(PlacedImage {
                    id: placement.xobject,
                    pixels,
                    governing_dpi: measured,
                    placements: 1,
                });
        }
    }

    found.into_values().collect()
}

/// `/Width` and `/Height` off an image XObject, or `None` when it does not
/// declare both as usable sample counts.
fn pixel_size(document: &Document, id: ObjectId) -> Option<(u32, u32)> {
    let dict = &document.get_object(id).ok()?.as_stream().ok()?.dict;
    let side = |key: &[u8]| u32::try_from(dict.get(key).ok()?.as_i64().ok()?).ok();

    Some((side(b"Width")?, side(b"Height")?))
}

/// T-193's contribution to the report: every image the document draws was
/// left byte-identical.
///
/// Which is the whole truth today and will still be true of most of them
/// after T-194 — decision 5's "an image already below the target comes out
/// byte-identical" is a promise, not a shortcut. What T-194 changes here is
/// that some of this count moves to `images_resampled`.
///
/// The count is of images *measured*, not of images present, and that is
/// deliberate: an image this crate could not see placed is not an image it
/// declined to resample, so claiming it as skipped would be claiming a
/// decision that was never made.
pub(crate) fn pass(document: &Document) -> Work {
    Work {
        images_skipped: inventory(document).len(),
        ..Work::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{document_drawing_one_image, loaded_document};

    #[test]
    fn a_document_that_paints_no_image_has_an_empty_inventory() {
        assert!(inventory(&loaded_document(4)).is_empty());
    }

    #[test]
    fn an_image_drawn_once_is_measured_against_the_size_it_is_drawn_at() {
        let document = document_drawing_one_image((300, 300), &[(150.0, 150.0)]);

        let inventory = inventory(&document);

        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].pixels, (300, 300));
        assert_eq!(inventory[0].placements, 1);
        assert_eq!(
            inventory[0].governing_dpi,
            EffectiveDpi {
                horizontal: 144.0,
                vertical: 144.0
            }
        );
    }

    /// One object, two pages, two scales — one entry, governed by the page
    /// that draws it biggest. Resampling to the small page's 576 DPI would
    /// leave the large page drawing a quarter of the pixels it needs.
    #[test]
    fn the_same_image_on_two_pages_is_one_entry_governed_by_the_larger_placement() {
        let document = document_drawing_one_image((576, 576), &[(72.0, 72.0), (288.0, 288.0)]);

        let inventory = inventory(&document);

        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].placements, 2);
        assert_eq!(inventory[0].governing_dpi.horizontal, 144.0);
    }

    /// The safety property, stated as a test rather than as a paragraph. The
    /// fixture's `/Unplaced` is in the same resource dictionary as the image
    /// that *is* drawn, and it is not in the inventory — which is how an
    /// image hidden inside a form XObject, or written inline, stays out of
    /// T-194's reach.
    #[test]
    fn an_image_the_document_never_paints_is_not_in_the_inventory() {
        let document = document_drawing_one_image((300, 300), &[(150.0, 150.0)]);

        let inventory = inventory(&document);

        let declared = document
            .objects
            .values()
            .filter(|object| object.type_name().ok() == Some(b"XObject"))
            .count();
        assert_eq!(declared, 2, "the fixture declares two image XObjects");
        assert_eq!(inventory.len(), 1, "only the painted one is inventoried");
    }

    /// A placement that collapses an axis paints nothing, so it neither
    /// enters the inventory nor governs anything that does.
    #[test]
    fn a_placement_that_draws_nothing_is_not_counted() {
        assert!(inventory(&document_drawing_one_image((300, 300), &[(0.0, 150.0)])).is_empty());

        let both = document_drawing_one_image((300, 300), &[(0.0, 150.0), (150.0, 150.0)]);
        let inventory = inventory(&both);
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].placements, 1);
        assert_eq!(inventory[0].governing_dpi.horizontal, 144.0);
    }

    #[test]
    fn the_report_counts_every_image_the_pass_left_alone() {
        let document = document_drawing_one_image((300, 300), &[(150.0, 150.0), (300.0, 300.0)]);

        assert_eq!(
            pass(&document),
            Work {
                images_skipped: 1,
                ..Work::default()
            }
        );
    }

    #[test]
    fn a_document_without_images_reports_no_image_work() {
        assert_eq!(pass(&loaded_document(3)), Work::default());
    }
}
