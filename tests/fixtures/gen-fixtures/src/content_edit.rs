//! Fixtures for Batch 21 page-content editing tests (T-159): a lightweight
//! single-page document with one raster image XObject, sized for
//! `pdf-edit`'s move/resize/replace round-trip tests — the image
//! counterpart to this crate's `build_multi_line_page_document` (text) and
//! `tests/fixtures/content-edit/reportlab_embedded_subset.pdf` (embedded
//! font). Compare `large::build_large_document`, which builds the same
//! shape of document at perf-test scale instead of test-fixture scale.

use lopdf::content::{Content, Operation};
use lopdf::xref::XrefType;
use lopdf::{dictionary, Document, Object, Stream};

/// The image XObject's resource name in every document this builds —
/// matching `pdf-edit`'s own unit-test fixtures (`fixture::image_resources`
/// in `core/pdf-edit/src/parse/interpreter.rs`), so a test can name it
/// without threading the value through.
pub const IMAGE_RESOURCE_NAME: &str = "Im1";

/// A single-page, unencrypted document with one small raster image XObject
/// painted at (100, 600), 80x40 points — a known, non-square rect so a
/// move/resize test can assert on width and height independently.
pub fn build_image_page_document() -> Document {
    let mut doc = Document::with_version("1.5");
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

    let pages_id = doc.new_object_id();

    // 4x4 RGB checkerboard: real varying pixel data (not a degenerate
    // single-color image) while staying trivial to inspect.
    let side = 4u32;
    let mut pixels = Vec::with_capacity((side * side * 3) as usize);
    for y in 0..side {
        for x in 0..side {
            let value = if (x + y) % 2 == 0 { 220u8 } else { 40u8 };
            pixels.extend_from_slice(&[value, value, value]);
        }
    }

    let mut image_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(side),
            "Height" => i64::from(side),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
        },
        pixels,
    );
    image_stream
        .compress()
        .expect("compress fixture image stream");
    let image_id = doc.add_object(image_stream);

    let resources_id = doc.add_object(dictionary! {
        "XObject" => dictionary! { IMAGE_RESOURCE_NAME => image_id },
    });

    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    80.into(),
                    0.into(),
                    0.into(),
                    40.into(),
                    100.into(),
                    600.into(),
                ],
            ),
            Operation::new("Do", vec![IMAGE_RESOURCE_NAME.into()]),
            Operation::new("Q", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        content.encode().expect("encode fixture content stream"),
    ));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    doc
}
