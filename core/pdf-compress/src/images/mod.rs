//! Every image the document actually draws, measured against the paper.
//!
//! ## The rule this module exists to keep
//!
//! *An image this pass never saw drawn is an image this crate never touches.*
//!
//! This file is the measuring half of the image stage (T-193); [`rewrite`] is
//! the half that resamples (T-194). Splitting them that way is not ceremony —
//! deciding *which* images may be touched is the decision that can destroy a
//! document, and it is worth making on its own, against tests that do not
//! have to encode a single JPEG to run.
//!
//! The stage in full, in the order a single image moves through it:
//!
//! - here — every image the document *draws*, measured against the paper;
//! - [`dpi`] — the arithmetic of that measurement, and which placement wins
//!   when there is more than one;
//! - [`format`] — what an image's dictionary declares it to be, and every
//!   declaration this crate refuses;
//! - [`codec`] — an image opened into samples, and closed again into a
//!   stream;
//! - [`resample`] — the new sample counts and the scaling itself;
//! - [`rewrite`] — putting the result back, with its soft mask, if and only
//!   if it is smaller.
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
//! `Do`. [`rewrite`] reaches one through the image it belongs to, which is
//! the only way it may be resampled at all — alone, nothing in the document
//! says how large it is drawn.
//!
//! ## What it costs
//!
//! One interpreter walk per page, and only under a preset that has an image
//! policy at all — [`crate::CompressPreset::Lossless`] never asks. A page
//! whose content stream cannot be tokenized contributes nothing rather than
//! failing the compression: a malformed page is a reason to leave that page's
//! images alone, not a reason to hand the user back a file they cannot
//! shrink.

mod codec;
pub(crate) mod dpi;
mod format;
mod resample;
mod rewrite;

use std::collections::BTreeMap;

use lopdf::{Document, ObjectId};

use crate::preset::ImagePolicy;
use crate::report::Work;
use dpi::EffectiveDpi;

/// One image XObject, as the document draws it.
///
/// Everything [`rewrite`] needs to decide what to do with it: what it is
/// scaling from (`pixels`), what it is scaling to (`governing_dpi` against
/// the preset's target), and which object to swap the bytes of (`id`). They
/// are measured here because measuring is the part that has to be right.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlacedImage {
    /// The XObject's object id — what [`rewrite`] swaps the bytes of.
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

/// Brings every image the document draws down to `policy`'s target, and
/// reports what moved.
///
/// The two counts are of images *measured*, not of images present, and that
/// is deliberate: an image this crate could not see placed is not an image it
/// declined to resample, so claiming it as skipped would be claiming a
/// decision that was never made.
///
/// `images_skipped` is therefore still the larger number on most documents,
/// and that is the feature working: it counts every image already below the
/// target, every image [`codec`] will not open, and every resample that came
/// out bigger than what it replaced. All three come out byte-identical.
pub(crate) fn pass(document: &mut Document, policy: ImagePolicy) -> Work {
    let mut work = Work::default();

    for image in inventory(document) {
        if rewrite::shrink(document, &image, policy) {
            work.images_resampled += 1;
        } else {
            work.images_skipped += 1;
        }
    }

    work
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{document_drawing_one_image, loaded_document};

    /// The middle preset's row of the table, spelled out so the stage's tests
    /// read against a number rather than against `CompressPreset::Balanced`
    /// — the table itself is [`crate::preset`]'s to test.
    const BALANCED: ImagePolicy = ImagePolicy {
        target_dpi: 150,
        jpeg_quality: 75,
    };

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
        let mut document =
            document_drawing_one_image((300, 300), &[(150.0, 150.0), (300.0, 300.0)]);

        assert_eq!(
            pass(&mut document, BALANCED),
            Work {
                images_skipped: 1,
                ..Work::default()
            },
            "72 effective dpi is under the target, so the image keeps its bytes"
        );
    }

    #[test]
    fn a_document_without_images_reports_no_image_work() {
        assert_eq!(pass(&mut loaded_document(3), BALANCED), Work::default());
    }

    /// The stage end to end: one oversized image in, one resampled image out,
    /// counted as resampled rather than skipped.
    #[test]
    fn an_oversized_image_is_resampled_and_counted_as_one() {
        let mut document = document_drawing_one_image((600, 600), &[(72.0, 72.0)]);

        assert_eq!(
            pass(&mut document, BALANCED),
            Work {
                images_resampled: 1,
                ..Work::default()
            }
        );
    }

    /// The safety property carried through to the rewrite: the fixture's
    /// permanent `/Unplaced` is as oversized as the image beside it, and it
    /// is still holding every one of its bytes afterwards. An image painted
    /// from inside a form XObject, or written inline, is out of reach in
    /// exactly this way.
    #[test]
    fn an_image_the_document_never_paints_keeps_every_byte() {
        let mut document = document_drawing_one_image((600, 600), &[(72.0, 72.0)]);
        let painted: Vec<_> = inventory(&document).iter().map(|image| image.id).collect();
        let (id, before) = document
            .objects
            .iter()
            .find(|(id, object)| {
                object.type_name().ok() == Some(b"XObject") && !painted.contains(id)
            })
            .map(|(id, object)| (*id, object.clone()))
            .expect("the fixture declares an image nothing paints");

        pass(&mut document, BALANCED);

        assert_eq!(document.objects.get(&id), Some(&before));
    }
}
