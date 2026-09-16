//! What the image stage does to the rows the corpus gained for it (T-197).
//!
//! `corpus.rs` next door holds the statements that must be true of *every*
//! document: nothing grows, nothing loses a page, a protected file comes back
//! untouched. This file holds the other kind — what one particular fixture
//! was added to say, and would stop saying if it were quietly replaced.
//!
//! It exists because until T-197 the repository could not make these
//! statements at all. T-193 measured every image in every real PDF here and
//! found the most detailed one at **37 effective dpi** — a third of the
//! lowest preset's ceiling. The resampler could therefore run over the whole
//! corpus, find nothing to do, and pass. Four fixtures close that:
//! `scan_200dpi.pdf` is a page sampled far above every ceiling,
//! `reused_image_two_scales.pdf` paints one XObject at two sizes,
//! `transparency_smask.pdf` carries a real soft mask, and `vector_only.pdf`
//! carries no image at all.
//!
//! Every assertion below is on a **sample count**, not on a byte count
//! wherever one will do. "It got smaller" is satisfied by a great many wrong
//! answers; "this image is now 1275 samples across, which is 150 dpi over
//! 612 points of paper" is satisfied by one.

mod common;

use lopdf::{Document, Object, ObjectId, Stream};
use pdf_compress::{compress, CompressPreset, Compressed, SignedDocuments};

use common::{fixture, read};

/// Compresses the corpus row whose file name is `file_name`.
fn compressed(file_name: &str, preset: CompressPreset) -> Compressed {
    let entry = fixture(file_name);
    let input = read(entry).expect("the compression corpus is committed");

    compress(&input, preset, SignedDocuments::LeaveAlone)
        .unwrap_or_else(|err| panic!("{} could not be compressed: {err}", entry.path))
}

/// Every image XObject in `bytes`, by object id.
///
/// Read back out of the *written* document rather than out of an in-memory
/// graph, because what a user opens is the file. An image that came out right
/// in the pipeline and wrong on the way to disk is a bug this would catch and
/// an inspection of the graph would not.
fn images_in(bytes: &[u8]) -> Vec<(ObjectId, Stream)> {
    let document = Document::load_mem(bytes).expect("a compressed document reloads");

    let mut found: Vec<(ObjectId, Stream)> = document
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let stream = object.as_stream().ok()?;
            (stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
                .then(|| (*id, stream.clone()))
        })
        .collect();
    found.sort_by_key(|(id, _)| *id);
    found
}

/// `/Width` x `/Height`, as the dictionary declares them.
fn samples(stream: &Stream) -> (i64, i64) {
    let side = |key: &[u8]| {
        stream
            .dict
            .get(key)
            .and_then(Object::as_i64)
            .expect("an image declares both its sides")
    };

    (side(b"Width"), side(b"Height"))
}

/// The one image in a single-image fixture.
fn only_image(compressed: &Compressed) -> Stream {
    let images = images_in(compressed.bytes());
    assert_eq!(images.len(), 1, "this fixture holds exactly one image");

    images.into_iter().next().expect("checked just above").1
}

/// A scan, brought down to each preset's ceiling and no further.
///
/// The sheet is sampled at 200 dpi over US Letter — 1700 x 2200 samples
/// across 612 x 792 points. Every expected number below is that sample count
/// scaled by `ceiling / 200`, so each one can be checked by hand rather than
/// copied out of a failing run: 1700 x 150/200 is 1275, and 1700 x 96/200 is
/// 816.
#[test]
fn a_scan_is_resampled_to_exactly_the_preset_ceiling() {
    let cases = [
        (CompressPreset::Balanced, (1275, 1650)),
        (CompressPreset::Small, (816, 1056)),
    ];

    for (preset, expected) in cases {
        let compressed = compressed("scan_200dpi.pdf", preset);

        assert_eq!(
            compressed.report().work().images_resampled,
            1,
            "{preset:?} left the scan's only image alone"
        );
        assert_eq!(
            samples(&only_image(&compressed)),
            expected,
            "{preset:?} did not bring the scan to its own ceiling"
        );
    }
}

/// And `Lossless` does not merely leave it alone — it never looks.
///
/// Stated on the scan specifically because this is the fixture where looking
/// would be *tempting*: it is the one file in the repository where there is a
/// large, obvious saving on the table, and the preset that promised not to
/// touch a pixel has to leave it there.
#[test]
fn lossless_leaves_a_scan_at_two_hundred_dpi() {
    let compressed = compressed("scan_200dpi.pdf", CompressPreset::Lossless);

    assert_eq!(compressed.report().work().images_resampled, 0);
    assert_eq!(compressed.report().work().images_skipped, 0);
    assert_eq!(samples(&only_image(&compressed)), (1700, 2200));
}

/// The rule the whole inventory is built around: when one image is painted
/// more than once, the **largest** placement governs.
///
/// `reused_image_two_scales.pdf` paints one 300-sample image 72 points across
/// on its first page and 18 points across on its second — 300 dpi and 1200
/// dpi for the same object. `Balanced` targets 150, so the answer is 150 x
/// 150: half, because the placement that can be ruined is the large one.
///
/// The failure this exists to catch is the plausible one. Taking the maximum
/// reads as "satisfy the most demanding placement", and it produces a 37 x 37
/// image — the thumbnail stays crisp and the full-size placement is destroyed.
/// Both answers shrink the file, so only the sample count tells them apart.
#[test]
fn the_largest_placement_governs_on_a_real_file() {
    let compressed = compressed("reused_image_two_scales.pdf", CompressPreset::Balanced);

    assert_eq!(compressed.report().work().images_resampled, 1);
    assert_eq!(
        samples(&only_image(&compressed)),
        (150, 150),
        "the image is painted at 300 dpi and at 1200 dpi; 150 is the target \
         against the 300 the large placement demands. 37 x 37 would mean the \
         1200 dpi thumbnail governed and the full-size placement was ruined"
    );
}

/// One image painted twice is still one image resampled.
///
/// The count is a report a user reads, so "2 images resampled" on a document
/// holding one would be a lie about their file even though every byte of it
/// came out right.
#[test]
fn an_image_painted_twice_is_counted_once() {
    let compressed = compressed("reused_image_two_scales.pdf", CompressPreset::Small);

    assert_eq!(compressed.report().work().images_resampled, 1);
    assert_eq!(compressed.report().work().images_skipped, 0);
}

/// Real transparency: a lossy photograph with a `/FlateDecode` soft mask,
/// both at 300 effective dpi.
///
/// Three things have to hold at once, and each has its own way of going
/// wrong. The mask must still be *there* — a resample that rebuilt the
/// image's dictionary without carrying `/SMask` over produces a photograph
/// that is simply opaque. It must be at the **same** sample count as its
/// parent, because it is drawn over exactly the same paper and a mask coarser
/// than the colour it hides shows as a halo. And it must not be *counted*: a
/// photograph with an alpha channel is one image the user has, not two.
///
/// **Both resampling presets, because the acceptance criterion names both.**
/// Until this loop existed, every soft-mask test in the crate — the four unit
/// ones in `images::rewrite` included — ran at `Balanced` and nothing
/// exercised a mask at `Small`. 600 samples over 144 points is 300 dpi, so
/// the expected counts are 600 x 150/300 and 600 x 96/300.
#[test]
fn a_soft_mask_follows_its_image_down_and_is_not_counted_twice() {
    let cases = [
        (CompressPreset::Balanced, (300, 300)),
        (CompressPreset::Small, (192, 192)),
    ];

    for (preset, expected) in cases {
        let compressed = compressed("transparency_smask.pdf", preset);

        assert_eq!(
            compressed.report().work().images_resampled,
            1,
            "{preset:?} counted the mask as an image of its own"
        );

        let images = images_in(compressed.bytes());
        assert_eq!(images.len(), 2, "the image and its mask");

        let (_, parent) = images
            .iter()
            .find(|(_, stream)| stream.dict.has(b"SMask"))
            .unwrap_or_else(|| {
                panic!("{preset:?}: the photograph must still declare its soft mask")
            });
        let mask_id = parent
            .dict
            .get(b"SMask")
            .and_then(Object::as_reference)
            .expect("/SMask is a reference to the mask stream");
        let (_, mask) = images
            .iter()
            .find(|(id, _)| *id == mask_id)
            .unwrap_or_else(|| {
                panic!("{preset:?}: the mask the photograph points at must still exist")
            });

        assert_eq!(
            samples(parent),
            expected,
            "{preset:?} did not bring the photograph to its own ceiling"
        );
        assert_eq!(
            samples(mask),
            samples(parent),
            "{preset:?} left the mask at a resolution its image no longer has"
        );
        assert_eq!(
            parent.dict.get(b"Filter").and_then(Object::as_name).ok(),
            Some(b"DCTDecode".as_slice()),
            "{preset:?}: a lossy image must come back lossy; a JPEG that became \
             a flate would have dropped its own alpha arrangement on the way"
        );
    }
}

/// A document with no images in it gives the image stage nothing to report.
///
/// The distinction the count has to preserve is between *found and left
/// alone* and *never found*. `images_skipped` is a tally of images the pass
/// measured and decided against; on a page of paths and type there is nothing
/// to measure, so a non-zero number here would mean the walk had invented an
/// image — most likely by reading `/Resources` instead of the placements.
#[test]
fn a_vector_document_gives_the_image_stage_nothing_to_find() {
    for preset in CompressPreset::all() {
        let compressed = compressed("vector_only.pdf", preset);

        assert_eq!(
            compressed.report().work().images_resampled,
            0,
            "{preset:?} resampled an image in a document that has none"
        );
        assert_eq!(
            compressed.report().work().images_skipped,
            0,
            "{preset:?} measured an image in a document that has none"
        );
        assert!(
            images_in(compressed.bytes()).is_empty(),
            "{preset:?} put an image into a vector document"
        );
    }
}

/// And it still shrinks, by the only lever left.
///
/// The row's other half: "no images" must not read as "no compression". Three
/// pages of unfiltered content streams are exactly what today's save path
/// produces, and the structural pass is what that case is for.
#[test]
fn a_vector_document_is_still_worth_compressing() {
    let input = read(fixture("vector_only.pdf")).expect("committed");
    let compressed = compressed("vector_only.pdf", CompressPreset::Lossless);

    assert!(
        compressed.report().work().streams_recompressed >= 3,
        "one content stream per page arrived unfiltered and should have been flated"
    );
    assert!(
        compressed.bytes().len() * 2 < input.len(),
        "a vector document of unfiltered streams compressed by less than half: \
         {} bytes from {}",
        compressed.bytes().len(),
        input.len()
    );
}
