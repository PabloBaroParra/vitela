//! The five documents of the compression corpus.
//!
//! One builder per row of [`CORPUS`](super::CORPUS), and each one's doc
//! comment says what its fixture is answerable for — because a fixture whose
//! reason has been forgotten is a fixture the next person will "simplify"
//! into one that proves nothing.
//!
//! What a builder decides is the *shape*: how many pages, what is painted on
//! them, and at what size. How a set of samples becomes an image XObject is
//! [`super::images`]'s question, and what those samples look like is
//! [`super::raster`]'s.

use lopdf::content::{Content, Operation};
use lopdf::xref::XrefType;
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use super::images::{flate_image, jpeg, Colour};
use super::raster;

/// US Letter, in PDF user-space units (1/72 inch).
const LETTER: (f64, f64) = (612.0, 792.0);

/// The dpi the scan fixture is sampled at.
///
/// Comfortably above `Balanced`'s 150 and `Small`'s 96 without being so far
/// above that the fixture is mostly repository weight: the resampler has real
/// work to do at both presets, and the file stays a couple of hundred
/// kilobytes rather than a couple of megabytes.
const SCAN_DPI: f64 = 200.0;

/// A scanned sheet: one full-bleed `/DCTDecode` grey image at [`SCAN_DPI`].
pub(super) fn scan() -> lopdf::Result<Document> {
    let pixels = (
        (LETTER.0 * SCAN_DPI / 72.0) as u32,
        (LETTER.1 * SCAN_DPI / 72.0) as u32,
    );

    let mut builder = Builder::new();
    let sheet = raster::scanned_sheet(pixels.0, pixels.1);
    let image_id = builder
        .document
        .add_object(jpeg(pixels.0, pixels.1, 90, sheet, Colour::Grey));
    let resources_id = builder.document.add_object(dictionary! {
        "XObject" => dictionary! { "Im0" => image_id },
    });
    builder.add_page(resources_id, draw_image("Im0", (0.0, 0.0), LETTER), LETTER)?;

    Ok(builder.finish())
}

/// The same image XObject painted twice at different sizes: 72 points across
/// on the first page, 18 on the second.
///
/// The rule this fixture exists for is `images::dpi`'s: the **largest**
/// placement governs, which is the *lowest* effective dpi, not the highest.
/// 300 samples across 72 points is 300 dpi and across 18 points is 1200, so
/// an implementation that took the maximum would resample this image to an
/// eighth of its samples and leave the large placement drawing 37 dpi. The
/// corpus can say that in one assertion on `/Width`.
///
/// It is also the corpus's only `/FlateDecode` image above a preset's
/// ceiling — the scan and the transparency fixture are both lossy — so it is
/// what keeps the resampler's flate-in/flate-out path honest on a real file.
pub(super) fn reused_image() -> lopdf::Result<Document> {
    let image = flate_image(300, 300, "DeviceGray", raster::textured(300, 300, 1))?;

    let mut builder = Builder::new();
    let image_id = builder.document.add_object(image);
    let resources_id = builder.document.add_object(dictionary! {
        "XObject" => dictionary! { "Im0" => image_id },
    });

    builder.add_page(
        resources_id,
        draw_image("Im0", (72.0, 648.0), (72.0, 72.0)),
        LETTER,
    )?;
    builder.add_page(
        resources_id,
        draw_image("Im0", (72.0, 702.0), (18.0, 18.0)),
        LETTER,
    )?;

    Ok(builder.finish())
}

/// A lossy RGB photograph with a real `/SMask`, both at 300 effective dpi.
///
/// Both halves are above every preset's ceiling on purpose. A mask already
/// below the target would be left byte-identical whatever the resampler did
/// with its parent, and the fixture would then prove nothing about the one
/// thing that is easy to get wrong here — that the mask follows the image
/// down and goes on covering exactly the same paper.
///
/// The parent is `/DCTDecode` and the mask is `/FlateDecode`, which is not a
/// contrivance: JPEG has no alpha channel, so *every* transparent photograph
/// in the wild is exactly this pair. It is also the arrangement that makes
/// `rewrite`'s ordering visible — the mask is only ever touched after the
/// image it belongs to has actually shipped.
pub(super) fn transparency() -> lopdf::Result<Document> {
    let side = 600;
    let mut image = jpeg(side, side, 90, raster::textured(side, side, 3), Colour::Rgb);
    let mask = flate_image(side, side, "DeviceGray", raster::soft_mask(side, side))?;

    let mut builder = Builder::new();
    let mask_id = builder.document.add_object(mask);
    image.dict.set("SMask", mask_id);
    let image_id = builder.document.add_object(image);

    let resources_id = builder.document.add_object(dictionary! {
        "XObject" => dictionary! { "Im0" => image_id },
    });
    builder.add_page(
        resources_id,
        draw_image("Im0", (162.0, 288.0), (144.0, 144.0)),
        LETTER,
    )?;

    Ok(builder.finish())
}

/// Three pages of paths and type, and not one image anywhere in the file.
///
/// The row that says what compression does when there is nothing lossy to
/// take: the only gain available is structural, and the image stage has to
/// report finding nothing rather than deciding against something.
pub(super) fn vector_only() -> lopdf::Result<Document> {
    let mut builder = Builder::new();
    let font_id = builder.document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = builder.document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    for page in 0..3 {
        builder.add_page(resources_id, vector_artwork(page), LETTER)?;
    }

    Ok(builder.finish())
}

/// The same artwork, written the way a compressed file is written: every
/// stream flated here, dictionaries packed into object streams and a
/// cross-reference stream at save time (see [`serialise`]).
///
/// Nothing in it is orphaned and nothing is duplicated, so there is no slack
/// for any stage to find. What the corpus asks of it is the negative: a file
/// with nothing left to win must come back **byte for byte**, not one byte
/// larger for having been offered to a compressor.
pub(super) fn already_packed() -> lopdf::Result<Document> {
    let mut document = vector_only()?;

    let stream_ids: Vec<ObjectId> = document
        .objects
        .iter()
        .filter(|(_, object)| matches!(object, Object::Stream(_)))
        .map(|(id, _)| *id)
        .collect();
    for id in stream_ids {
        if let Ok(Object::Stream(stream)) = document.get_object_mut(id) {
            stream.compress()?;
        }
    }

    Ok(document)
}

/// A page's worth of vector drawing: a ruled grid, filled bars of varying
/// width, a run of Bézier curves, and a few lines of type.
fn vector_artwork(page: usize) -> Content {
    let mut operations = vec![
        Operation::new("q", vec![]),
        Operation::new("w", vec![0.5.into()]),
    ];

    for row in 0..24u32 {
        let y = 72.0 + f64::from(row) * 28.0;
        operations.push(Operation::new("m", vec![72.0.into(), y.into()]));
        operations.push(Operation::new("l", vec![540.0.into(), y.into()]));
    }
    operations.push(Operation::new("S", vec![]));

    for bar in 0..18u32 {
        let y = 90.0 + f64::from(bar) * 28.0;
        let width = 60.0 + f64::from((bar * 37 + page as u32 * 11) % 380);
        operations.push(Operation::new(
            "rg",
            vec![
                (f64::from(bar % 5) / 5.0).into(),
                0.35.into(),
                (1.0 - f64::from(bar % 7) / 7.0).into(),
            ],
        ));
        operations.push(Operation::new(
            "re",
            vec![72.0.into(), y.into(), width.into(), 12.0.into()],
        ));
        operations.push(Operation::new("f", vec![]));
    }

    operations.push(Operation::new("G", vec![0.into()]));
    for curve in 0..12u32 {
        let x = 90.0 + f64::from(curve) * 36.0;
        operations.push(Operation::new("m", vec![x.into(), 700.0.into()]));
        operations.push(Operation::new(
            "c",
            vec![
                (x + 12.0).into(),
                760.0.into(),
                (x + 24.0).into(),
                640.0.into(),
                (x + 36.0).into(),
                700.0.into(),
            ],
        ));
    }
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));

    operations.push(Operation::new("BT", vec![]));
    for line in 0..8i64 {
        operations.push(Operation::new("Tf", vec!["F1".into(), 11.into()]));
        operations.push(Operation::new(
            "Td",
            vec![72.into(), (740 - line * 13).into()],
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(format!(
                "page {page}, line {line}: a vector page, drawn rather than photographed"
            ))],
        ));
    }
    operations.push(Operation::new("ET", vec![]));

    Content { operations }
}

/// `q <w> 0 0 <h> <x> <y> cm /<name> Do Q` — one image, placed.
fn draw_image(name: &str, at: (f64, f64), drawn: (f64, f64)) -> Content {
    Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    drawn.0.into(),
                    0.into(),
                    0.into(),
                    drawn.1.into(),
                    at.0.into(),
                    at.1.into(),
                ],
            ),
            Operation::new("Do", vec![name.into()]),
            Operation::new("Q", vec![]),
        ],
    }
}

/// The page-tree bookkeeping every fixture above shares.
struct Builder {
    document: Document,
    pages_id: ObjectId,
    kids: Vec<Object>,
}

impl Builder {
    fn new() -> Self {
        let mut document = Document::with_version("1.5");
        // Classic table and loose objects: the shape today's save path
        // produces, which is the shape the compressor is handed.
        document.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        let pages_id = document.new_object_id();

        Builder {
            document,
            pages_id,
            kids: Vec::new(),
        }
    }

    fn add_page(
        &mut self,
        resources_id: ObjectId,
        content: Content,
        media_box: (f64, f64),
    ) -> lopdf::Result<()> {
        let content_id = self
            .document
            .add_object(Stream::new(dictionary! {}, content.encode()?));
        let page_id = self.document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => self.pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), media_box.0.into(), media_box.1.into()],
        });
        self.kids.push(page_id.into());
        Ok(())
    }

    fn finish(mut self) -> Document {
        let count = self.kids.len() as i64;
        self.document.objects.insert(
            self.pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => self.kids,
                "Count" => count,
            }),
        );
        let catalog_id = self.document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => self.pages_id,
        });
        self.document.trailer.set("Root", catalog_id);

        self.document
    }
}
