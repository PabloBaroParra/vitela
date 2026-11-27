use super::*;
use crate::error::ErrorKind;
use crate::model::{pdfs, ContentImageView};
use pdf_manip::OpenCredentials;

fn open(bytes: &[u8]) -> DocumentModel {
    DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
}

/// A `width` x `height` opaque blue PNG.
fn png(width: u32, height: u32) -> Vec<u8> {
    use image::{DynamicImage, ImageFormat, RgbImage};
    let mut bytes = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, image::Rgb([0, 0, 255])))
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encodes");
    bytes.into_inner()
}

fn images(model: &DocumentModel) -> Vec<ContentImageView> {
    model.read_page_content(0).expect("reads").images
}

/// The one image [`pdfs::content_document`] paints: 100 x 50 at (72, 500).
fn the_image(model: &DocumentModel) -> ContentImageView {
    let shown = images(model);
    assert_eq!(shown.len(), 1, "{shown:?}");
    shown.into_iter().next().unwrap()
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Whether two boxes agree to the precision a written `cm` keeps.
fn near(a: Rect, b: Rect) -> bool {
    [a.x - b.x, a.y - b.y, a.width - b.width, a.height - b.height]
        .iter()
        .all(|delta| delta.abs() < 1e-3)
}

/// The `/Width` of every image XObject `bytes` holds, sorted.
fn saved_image_widths(bytes: &[u8]) -> Vec<i64> {
    let saved = lopdf::Document::load_mem(bytes).expect("loads");
    let mut widths: Vec<i64> = saved
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .filter(|stream| {
            stream
                .dict
                .get(b"Subtype")
                .and_then(|subtype| subtype.as_name())
                .is_ok_and(|name| name == b"Image")
        })
        .filter_map(|stream| stream.dict.get(b"Width").and_then(|w| w.as_i64()).ok())
        .collect();
    widths.sort_unstable();
    widths
}

#[test]
fn a_move_is_one_step_the_reader_sees_and_the_save_writes() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model);
    model
        .apply_image_edit(ImageEdit::Move {
            page: 0,
            image: image.id,
            dx: 30.0,
            dy: -20.0,
        })
        .expect("moves");

    assert_eq!(the_image(&model).bbox, rect(102.0, 480.0, 100.0, 50.0));
    assert!(model.can_undo());
    assert!(model.preview_stale());
    assert_ne!(model.paint_revision(), 0);

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert_eq!(the_image(&saved).bbox, rect(102.0, 480.0, 100.0, 50.0));

    assert!(model.undo());
    assert_eq!(the_image(&model).bbox, rect(72.0, 500.0, 100.0, 50.0));
    assert!(model.redo());
    assert_eq!(the_image(&model).bbox, rect(102.0, 480.0, 100.0, 50.0));
}

#[test]
fn a_second_move_of_the_same_image_amends_its_step() {
    let mut model = open(&pdfs::content_document());
    for _ in 0..2 {
        let image = the_image(&model).id;
        model
            .apply_image_edit(ImageEdit::Move {
                page: 0,
                image,
                dx: 10.0,
                dy: 0.0,
            })
            .expect("moves");
    }
    assert_eq!(the_image(&model).bbox.x, 92.0);
    assert!(model.undo());
    assert_eq!(the_image(&model).bbox.x, 72.0);
    assert!(!model.can_undo());
}

#[test]
fn a_resize_paints_the_image_into_the_new_box() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model).id;
    let to = rect(72.0, 450.0, 200.0, 100.0);
    model
        .apply_image_edit(ImageEdit::Resize { page: 0, image, to })
        .expect("resizes");
    assert_eq!(the_image(&model).bbox, to);

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert_eq!(the_image(&saved).bbox, to);
    assert!(model.undo());
    assert_eq!(the_image(&model).bbox, rect(72.0, 500.0, 100.0, 50.0));
}

#[test]
fn what_changes_nothing_records_nothing() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model);
    model
        .apply_image_edit(ImageEdit::Move {
            page: 0,
            image: image.id,
            dx: 0.0,
            dy: 0.0,
        })
        .expect("a zero move is no edit");
    model
        .apply_image_edit(ImageEdit::Resize {
            page: 0,
            image: image.id,
            to: image.bbox,
        })
        .expect("the same box is no edit");
    assert!(!model.can_undo());
}

#[test]
fn a_box_with_no_area_or_a_distance_that_is_not_finite_is_refused() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model).id;
    for edit in [
        ImageEdit::Resize {
            page: 0,
            image,
            to: rect(72.0, 500.0, 0.0, 50.0),
        },
        ImageEdit::Resize {
            page: 0,
            image,
            to: rect(f64::NAN, 500.0, 10.0, 50.0),
        },
        ImageEdit::Move {
            page: 0,
            image,
            dx: f64::INFINITY,
            dy: 0.0,
        },
    ] {
        assert_eq!(
            model.apply_image_edit(edit).unwrap_err().kind,
            ErrorKind::UnsupportedOperation
        );
    }
    assert!(!model.can_undo());
}

#[test]
fn a_removal_takes_the_image_off_until_it_is_undone() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model).id;
    model
        .apply_image_edit(ImageEdit::Remove { page: 0, image })
        .expect("removes");
    assert!(images(&model).is_empty());

    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert!(images(&saved).is_empty());

    assert!(model.undo());
    assert_eq!(the_image(&model).bbox, rect(72.0, 500.0, 100.0, 50.0));
}

#[test]
fn a_replacement_keeps_the_place_and_the_save_writes_the_new_picture() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model).id;
    model
        .check_image_replacement(0, image)
        .expect("replaceable");
    model
        .apply_image_edit(ImageEdit::Replace {
            page: 0,
            image,
            bytes: png(4, 2),
        })
        .expect("replaces");
    assert_eq!(the_image(&model).bbox, rect(72.0, 500.0, 100.0, 50.0));

    let bytes = model.save_to_bytes(false).expect("saves");
    assert!(
        saved_image_widths(&bytes).contains(&4),
        "{:?}",
        saved_image_widths(&bytes)
    );
    assert_eq!(
        the_image(&open(&bytes)).bbox,
        rect(72.0, 500.0, 100.0, 50.0)
    );

    assert!(model.undo());
    let bytes = model.save_to_bytes(false).expect("saves");
    assert_eq!(saved_image_widths(&bytes), vec![1]);
}

#[test]
fn a_replacement_that_does_not_decode_is_refused_without_a_step() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model).id;
    let refused = model
        .apply_image_edit(ImageEdit::Replace {
            page: 0,
            image,
            bytes: b"not an image".to_vec(),
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::InvalidImage);
    assert!(!model.can_undo());
}

#[test]
fn an_insert_is_placed_by_the_core_with_its_top_left_at_the_point() {
    let mut model = open(&pdfs::content_document());
    model
        .apply_image_edit(ImageEdit::Insert {
            page: 0,
            at: (300.0, 700.0),
            bytes: png(4, 2),
        })
        .expect("inserts");
    let shown = images(&model);
    assert_eq!(shown.len(), 2, "{shown:?}");
    // The longest side at the core's default, the proportions kept.
    assert!(shown
        .iter()
        .any(|image| image.bbox == rect(300.0, 628.0, 144.0, 72.0)));

    let bytes = model.save_to_bytes(false).expect("saves");
    assert_eq!(images(&open(&bytes)).len(), 2);
    assert_eq!(saved_image_widths(&bytes), vec![1, 4]);

    assert!(model.undo());
    assert_eq!(images(&model).len(), 1);
}

#[test]
fn an_inserted_image_can_be_moved_before_a_save() {
    let mut model = open(&pdfs::content_document());
    model
        .apply_image_edit(ImageEdit::Insert {
            page: 0,
            at: (300.0, 700.0),
            bytes: png(4, 2),
        })
        .expect("inserts");
    let inserted = images(&model)
        .into_iter()
        .find(|image| image.bbox.x == 300.0)
        .expect("the inserted image reads");
    model
        .apply_image_edit(ImageEdit::Move {
            page: 0,
            image: inserted.id,
            dx: 10.0,
            dy: 10.0,
        })
        .expect("moves the pending insert");
    assert!(images(&model)
        .iter()
        .any(|image| image.bbox == rect(310.0, 638.0, 144.0, 72.0)));
    let saved = images(&open(&model.save_to_bytes(false).expect("saves")));
    assert!(
        saved
            .iter()
            .any(|image| near(image.bbox, rect(310.0, 638.0, 144.0, 72.0))),
        "{saved:?}"
    );
}

#[test]
fn two_inserts_get_two_names() {
    let mut model = open(&pdfs::content_document());
    for at in [(300.0, 700.0), (300.0, 300.0)] {
        model
            .apply_image_edit(ImageEdit::Insert {
                page: 0,
                at,
                bytes: png(4, 2),
            })
            .expect("inserts");
    }
    assert_eq!(images(&model).len(), 3);
    let saved = open(&model.save_to_bytes(false).expect("saves"));
    assert_eq!(images(&saved).len(), 3);
}

#[test]
fn an_insert_of_bytes_that_do_not_decode_is_refused() {
    let mut model = open(&pdfs::content_document());
    let refused = model
        .apply_image_edit(ImageEdit::Insert {
            page: 0,
            at: (10.0, 10.0),
            bytes: b"nope".to_vec(),
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::InvalidImage);
    assert!(!model.can_undo());
}

#[test]
fn an_unknown_image_or_page_is_refused_without_a_step() {
    let mut model = open(&pdfs::content_document());
    assert_eq!(
        model
            .apply_image_edit(ImageEdit::Remove {
                page: 0,
                image: 9_999,
            })
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedOperation
    );
    assert_eq!(
        model
            .apply_image_edit(ImageEdit::Remove { page: 7, image: 0 })
            .unwrap_err()
            .kind,
        ErrorKind::PageIndexOutOfBounds
    );
    assert_eq!(
        model.check_image_replacement(0, 9_999).unwrap_err().kind,
        ErrorKind::UnsupportedOperation
    );
    assert!(!model.can_undo());
}

#[test]
fn an_unmodifiable_document_refuses_before_it_looks_for_the_image() {
    let mut model = DocumentModel::open(
        &pdfs::unmodifiable_document(),
        &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
    )
    .expect("opens");
    assert_eq!(
        model
            .apply_image_edit(ImageEdit::Remove {
                page: 0,
                image: 9_999,
            })
            .unwrap_err()
            .kind,
        ErrorKind::EditNotPermitted
    );
    assert_eq!(
        model
            .apply_image_edit(ImageEdit::Insert {
                page: 0,
                at: (10.0, 10.0),
                bytes: png(1, 1),
            })
            .unwrap_err()
            .kind,
        ErrorKind::EditNotPermitted
    );
    assert_eq!(
        model.check_image_replacement(0, 9_999).unwrap_err().kind,
        ErrorKind::EditNotPermitted
    );
    assert!(!model.can_undo());
}

#[test]
fn a_moved_image_can_be_replaced_through_a_checkpoint() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model).id;
    model
        .apply_image_edit(ImageEdit::Move {
            page: 0,
            image,
            dx: 10.0,
            dy: 0.0,
        })
        .expect("moves");
    let image = the_image(&model).id;
    model
        .check_image_replacement(0, image)
        .expect("a session that may checkpoint accepts it");
    model
        .apply_image_edit(ImageEdit::Replace {
            page: 0,
            image,
            bytes: png(4, 2),
        })
        .expect("replaces after a checkpoint");
    assert_eq!(the_image(&model).bbox.x, 82.0);
    let bytes = model.save_to_bytes(false).expect("saves");
    assert!(saved_image_widths(&bytes).contains(&4));

    // Undo walks back through the checkpoint one step at a time.
    assert!(model.undo());
    assert_eq!(the_image(&model).bbox.x, 82.0);
    assert!(model.undo());
    assert_eq!(the_image(&model).bbox.x, 72.0);
}

/// A complete 69-byte PNG whose header declares 11000 × 11000 RGBA pixels
/// (about 484 MB once decoded).
const OVERSIZED_PNG: [u8; 69] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x2a, 0xf8, 0x00, 0x00, 0x2a, 0xf8, 0x08, 0x06, 0x00, 0x00, 0x00, 0xce, 0x2f, 0x9a,
    0x8a, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60, 0xa0, 0x0c, 0x00,
    0x00, 0x00, 0x40, 0x00, 0x01, 0xb7, 0x34, 0x7c, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

fn refused_as_too_large(refused: EngineError) {
    assert_eq!(refused.kind, ErrorKind::InvalidImage);
    assert!(refused.message.contains("too large"), "{}", refused.message);
}

#[test]
fn an_insert_larger_than_the_core_decodes_is_refused_without_a_step() {
    let mut model = open(&pdfs::content_document());
    let refused = model
        .apply_image_edit(ImageEdit::Insert {
            page: 0,
            at: (10.0, 10.0),
            bytes: OVERSIZED_PNG.to_vec(),
        })
        .unwrap_err();
    refused_as_too_large(refused);
    assert!(!model.can_undo());
}

#[test]
fn a_replacement_larger_than_the_core_decodes_is_refused_without_a_step() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model).id;
    let refused = model
        .apply_image_edit(ImageEdit::Replace {
            page: 0,
            image,
            bytes: OVERSIZED_PNG.to_vec(),
        })
        .unwrap_err();
    refused_as_too_large(refused);
    assert!(!model.can_undo());
}

/// A PNG whose header reads but whose samples do not: its `IDAT` CRC is broken.
fn broken_png() -> Vec<u8> {
    let mut bytes = png(4, 4);
    let idat = bytes
        .windows(4)
        .position(|window| window == b"IDAT")
        .expect("an IDAT chunk");
    let length = u32::from_be_bytes(bytes[idat - 4..idat].try_into().unwrap()) as usize;
    bytes[idat + 4 + length] ^= 0xff;
    bytes
}

#[test]
fn a_replacement_whose_samples_do_not_decode_is_refused_without_a_step() {
    let mut model = open(&pdfs::content_document());
    let image = the_image(&model).id;
    let refused = model
        .apply_image_edit(ImageEdit::Replace {
            page: 0,
            image,
            bytes: broken_png(),
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::InvalidImage);
    assert!(!model.can_undo());
}

#[test]
fn an_insert_whose_samples_do_not_decode_is_refused_without_a_step() {
    let mut model = open(&pdfs::content_document());
    let refused = model
        .apply_image_edit(ImageEdit::Insert {
            page: 0,
            at: (10.0, 10.0),
            bytes: broken_png(),
        })
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::InvalidImage);
    assert!(!model.can_undo());
}
