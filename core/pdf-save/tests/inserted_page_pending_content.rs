//! Content queued on a page inserted in this session, read as the reader sees
//! it before any save.
//!
//! A blank page the model inserted has no backing bytes until a save
//! materializes it, and the pending read used to answer "nothing there" for
//! it outright — so text inserted, an image placed or a clip pasted onto it
//! painted in the preview but could not be selected or edited until the
//! reader saved, although the save wrote it correctly.

use pdf_document::{
    Command, ContentItemId, Document, FontKind, ImageItem, ImageSource, Orientation, PageId,
    PageSize, Rect, TextRun,
};
use pdf_edit::{pending_log_index, Selection};
use pdf_manip::LopdfDocument;
use pdf_save::{
    read_pending_page_content_of, record_content_command, record_paste, ImportedSources,
};

fn open_fixture(name: &str) -> (LopdfDocument, Document) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/compress")
        .join(name);
    let bytes = std::fs::read(path).expect("committed fixture must be readable");
    let (base, security) = pdf_manip::open_document_from_bytes(&bytes, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();
    (base, document)
}

/// Inserts a blank A4 page after the first one, the way every shell does,
/// and returns the id the core minted for it.
fn insert_blank_page(document: &mut Document) -> PageId {
    let command =
        Command::insert_blank_page(document, 1, PageSize::A4, Orientation::Portrait).unwrap();
    let Command::InsertPage { page, .. } = &command else {
        unreachable!("insert_blank_page builds an InsertPage")
    };
    let id = page.id;
    let mut log = std::mem::take(&mut document.pending_edits);
    assert!(log.apply(document, command));
    document.pending_edits = log;
    id
}

/// A `width` x `height` opaque PNG.
fn png(width: u32, height: u32) -> Vec<u8> {
    use image::{DynamicImage, ImageFormat, RgbImage};
    let mut bytes = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, image::Rgb([0, 0, 255])))
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encodes");
    bytes.into_inner()
}

fn read(document: &Document, base: &LopdfDocument, page: PageId) -> pdf_document::PageContent {
    read_pending_page_content_of(document, page, base, ImportedSources::none())
        .expect("an inserted page reads")
}

#[test]
fn text_inserted_on_an_unsaved_page_reads_as_pending() {
    let (base, mut document) = open_fixture("vector_only.pdf");
    let page = insert_blank_page(&mut document);
    let run = TextRun {
        id: ContentItemId(0),
        page,
        bbox: Rect {
            x: 72.0,
            y: 700.0,
            width: 1.0,
            height: 14.0,
        },
        resource_font_name: "VFins".to_string(),
        font_kind: FontKind::Standard14,
        text: "Added on a new page".to_string(),
    };
    record_content_command(
        &mut document,
        &base,
        ImportedSources::none(),
        Command::InsertTextRun(run),
    )
    .unwrap();

    let content = read(&document, &base, page);

    assert_eq!(content.text_runs.len(), 1, "{content:?}");
    let shown = &content.text_runs[0];
    assert_eq!(shown.text, "Added on a new page");
    assert_eq!(shown.page, page);
    assert_eq!(pending_log_index(shown.id), Some(1));
    // Measured in the inserted font, not left at the placeholder width.
    assert!(shown.bbox.width > 50.0, "{:?}", shown.bbox);
}

#[test]
fn an_image_inserted_on_an_unsaved_page_reads_as_pending() {
    let (base, mut document) = open_fixture("vector_only.pdf");
    let page = insert_blank_page(&mut document);
    let bbox = Rect {
        x: 100.0,
        y: 400.0,
        width: 120.0,
        height: 60.0,
    };
    record_content_command(
        &mut document,
        &base,
        ImportedSources::none(),
        Command::InsertImage {
            item: ImageItem {
                id: ContentItemId(0),
                page,
                bbox,
                source: ImageSource::Resource("VImg1".to_string()),
            },
            source: Some(png(4, 2)),
        },
    )
    .unwrap();

    let content = read(&document, &base, page);

    assert_eq!(content.images.len(), 1, "{content:?}");
    assert_eq!(content.images[0].bbox, bbox);
    assert_eq!(pending_log_index(content.images[0].id), Some(1));
}

#[test]
fn a_clip_pasted_on_an_unsaved_page_of_the_same_document_reads_as_pending() {
    let (base, mut document) = open_fixture("scan_200dpi.pdf");
    let original = read(&document, &base, PageId(0)).images;
    assert_eq!(original.len(), 1);
    let clip = pdf_save::copy_selection_content(
        &document,
        &base,
        ImportedSources::none(),
        &Selection {
            runs: Vec::new(),
            images: original.clone(),
            paths: Vec::new(),
        },
    )
    .unwrap();
    let page = insert_blank_page(&mut document);

    record_paste(
        &mut document,
        &base,
        ImportedSources::none(),
        page,
        clip,
        40.0,
        800.0,
    )
    .unwrap();

    let content = read(&document, &base, page);

    assert_eq!(content.images.len(), 1, "{content:?}");
    let pasted = &content.images[0];
    assert_eq!(pasted.page, page);
    assert_eq!(pending_log_index(pasted.id), Some(1));
    assert!((pasted.bbox.x - 40.0).abs() < 1e-6, "{:?}", pasted.bbox);
    assert!((pasted.bbox.y + pasted.bbox.height - 800.0).abs() < 1e-6);
    assert!((pasted.bbox.width - original[0].bbox.width).abs() < 1e-6);
    // The page the clip was copied from is untouched.
    assert_eq!(read(&document, &base, PageId(0)).images, original);
}

#[test]
fn an_unsaved_page_with_nothing_queued_still_reads_empty() {
    let (base, mut document) = open_fixture("vector_only.pdf");
    let page = insert_blank_page(&mut document);

    let content = read(&document, &base, page);

    assert!(content.text_runs.is_empty());
    assert!(content.images.is_empty());
    assert!(content.paths.is_empty());
}
