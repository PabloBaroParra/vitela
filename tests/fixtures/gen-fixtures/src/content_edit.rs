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
pub const TARGET_IMAGE_RESOURCE_NAME: &str = "ImTarget";
pub const CONTROL_IMAGE_RESOURCE_NAME: &str = "ImControl";

pub fn target_image_pixels() -> Vec<u8> {
    checkerboard_pixels(4, 220, 40)
}

pub fn control_image_pixels() -> Vec<u8> {
    checkerboard_pixels(4, 30, 180)
}

pub fn replacement_image_png_bytes() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x08, 0x02, 0x00, 0x00, 0x00, 0x36,
        0x88, 0x49, 0xd6, 0x00, 0x00, 0x00, 0x12, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xe0,
        0x8a, 0x3a, 0x71, 0x22, 0x8a, 0x8b, 0x01, 0x85, 0x02, 0x00, 0x49, 0xe9, 0x07, 0x09, 0x02,
        0xc1, 0xa0, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ]
}

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
    let pixels = target_image_pixels();

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

    let content: Content = Content {
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

pub fn build_roundtrip_image_page_document() -> Document {
    let mut doc = Document::with_version("1.5");
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let pages_id = doc.new_object_id();
    let target_id = add_image(&mut doc, target_image_pixels());
    let control_id = add_image(&mut doc, control_image_pixels());
    let resources_id = doc.add_object(dictionary! {
        "XObject" => dictionary! {
            TARGET_IMAGE_RESOURCE_NAME => target_id,
            CONTROL_IMAGE_RESOURCE_NAME => control_id,
        },
    });
    let content: Content<Vec<Operation>> = Content {
        operations: vec![
            paint_image(TARGET_IMAGE_RESOURCE_NAME, 80, 40, 100, 600),
            paint_image(CONTROL_IMAGE_RESOURCE_NAME, 50, 30, 300, 500),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>(),
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        content
            .encode()
            .expect("encode roundtrip fixture content stream"),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
        "Resources" => resources_id, "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc
}

fn checkerboard_pixels(side: u32, light: u8, dark: u8) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((side * side * 3) as usize);
    for y in 0..side {
        for x in 0..side {
            let value = if (x + y) % 2 == 0 { light } else { dark };
            pixels.extend_from_slice(&[value, value, value]);
        }
    }
    pixels
}

fn add_image(doc: &mut Document, pixels: Vec<u8>) -> lopdf::ObjectId {
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 4,
            "Height" => 4, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        },
        pixels,
    );
    stream.compress().expect("compress fixture image stream");
    doc.add_object(stream)
}

fn paint_image(name: &str, width: i64, height: i64, x: i64, y: i64) -> Vec<Operation> {
    vec![
        Operation::new("q", vec![]),
        Operation::new(
            "cm",
            vec![
                width.into(),
                0.into(),
                0.into(),
                height.into(),
                x.into(),
                y.into(),
            ],
        ),
        Operation::new("Do", vec![name.into()]),
        Operation::new("Q", vec![]),
    ]
}
