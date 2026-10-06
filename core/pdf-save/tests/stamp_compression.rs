//! An image stamp is stored compressed, and compressing it changes nothing
//! pdfium paints.
//!
//! A 528x258 drawn signature placed on a 2 KB page used to save as a 548 KB
//! file: the stamp's image XObject and its `/SMask` went out as raw samples
//! with no `/Filter`. These pin the fix where it was visible — the bytes on
//! disk — and its only acceptable cost, which is none: the same file with
//! those streams inflated back to raw must rasterize pixel for pixel the same.

use lopdf::Object;
use pdf_document::{AnnotationId, Command, Orientation, PageId, PageSize, Rect};
use pdf_save::{save_document, SaveInput, SaveIntent, SignatureAcknowledgement};

const WIDTH: u32 = 528;
const HEIGHT: u32 = 258;

/// What an RGBA image of the signature's size costs unfiltered: the RGB
/// image XObject plus its one-byte-per-pixel `/SMask`.
const RAW_SAMPLE_BYTES: usize = (WIDTH * HEIGHT * 4) as usize;

const STAMP: Rect = Rect {
    x: 100.0,
    y: 400.0,
    width: 264.0,
    height: 129.0,
};

fn apply_command(document: &mut pdf_document::Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

/// Transparent background, one dark stroke with soft edges — the shape of a
/// drawn signature, so the alpha plane has real partial coverage to keep.
fn signature_png() -> Vec<u8> {
    use image::{DynamicImage, ImageFormat, RgbaImage};
    use std::io::Cursor;

    let canvas = RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let wave = (f64::from(x) / 24.0).sin() * f64::from(HEIGHT) / 4.0;
        let distance = (f64::from(y) - (f64::from(HEIGHT) / 2.0 + wave)).abs();
        let coverage = (1.0 - (distance - 3.0) / 2.0).clamp(0.0, 1.0);
        image::Rgba([20, 30, 90, (coverage * 255.0) as u8])
    });
    let mut buf = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(canvas)
        .write_to(&mut buf, ImageFormat::Png)
        .expect("encode png fixture");
    buf.into_inner()
}

fn saved_with_signature_stamp() -> Vec<u8> {
    let base = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
    let mut document = pdf_save::document_from_lopdf(&base, None).unwrap();
    let insert = Command::insert_blank_page(&mut document, 0, PageSize::A4, Orientation::Portrait)
        .expect("a zero-page document has ids to spare");
    apply_command(&mut document, insert);
    let stamp =
        pdf_annotate::stamp_from_image_bytes(AnnotationId(1), PageId(0), &signature_png(), STAMP)
            .expect("a valid png");
    apply_command(&mut document, Command::AddAnnotation(stamp));

    save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: None,
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::none(),
    })
    .expect("save")
}

fn image_streams(document: &lopdf::Document) -> Vec<(lopdf::ObjectId, &lopdf::Stream)> {
    document
        .objects
        .iter()
        .filter_map(|(id, object)| match object {
            Object::Stream(stream)
                if stream.dict.get(b"Subtype").and_then(Object::as_name).ok()
                    == Some(b"Image".as_slice()) =>
            {
                Some((*id, stream))
            }
            _ => None,
        })
        .collect()
}

/// The same file with every image stream inflated back to raw samples —
/// exactly what the writer produced before the fix.
fn with_raw_image_streams(bytes: &[u8]) -> Vec<u8> {
    let mut document = lopdf::Document::load_mem(bytes).expect("saved bytes reload");
    let ids: Vec<_> = image_streams(&document)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    for id in ids {
        let stream = document
            .get_object_mut(id)
            .and_then(Object::as_stream_mut)
            .unwrap();
        stream.decompress().expect("flate inflates");
        assert!(!stream.dict.has(b"Filter"));
    }
    let mut out = Vec::new();
    document.save_to(&mut out).expect("re-serialize");
    out
}

fn render_page_one(bytes: Vec<u8>) -> Vec<u8> {
    let renderer = pdf_render::PdfiumRenderer::new();
    let doc = renderer
        .open_document_from_bytes(bytes, None)
        .expect("bytes must open in pdfium");
    let bitmap = renderer
        .render_page(
            doc,
            0,
            144,
            None,
            pdf_render::RenderOptions::default(),
            pdf_render::Priority::Visible,
        )
        .wait()
        .expect("page one must render");
    bitmap.get_pixels().unwrap().to_vec()
}

#[test]
fn a_png_stamps_image_and_smask_are_saved_flate_compressed() {
    let bytes = saved_with_signature_stamp();
    let document = lopdf::Document::load_mem(&bytes).expect("saved bytes reload");

    let images = image_streams(&document);
    assert_eq!(images.len(), 2, "the stamp image and its /SMask");
    for (id, stream) in images {
        assert_eq!(
            stream.dict.get(b"Filter").and_then(Object::as_name).ok(),
            Some(b"FlateDecode".as_slice()),
            "image stream {id:?} must not be written as raw samples"
        );
    }
}

#[test]
fn a_png_stamp_saves_far_smaller_than_its_raw_pixels() {
    let bytes = saved_with_signature_stamp();

    assert!(
        bytes.len() < RAW_SAMPLE_BYTES / 10,
        "saved {} bytes for {RAW_SAMPLE_BYTES} bytes of raw samples",
        bytes.len()
    );
}

#[test]
fn compressing_the_stamp_leaves_the_rendered_page_unchanged() {
    let compressed = saved_with_signature_stamp();
    let raw = with_raw_image_streams(&compressed);
    assert!(raw.len() > RAW_SAMPLE_BYTES, "the raw copy really is raw");

    let compressed_pixels = render_page_one(compressed);
    let raw_pixels = render_page_one(raw);

    assert!(
        compressed_pixels
            .chunks_exact(4)
            .any(|p| p[0] < 128 && p[1] < 128),
        "the stamp must actually paint, or the comparison proves nothing"
    );
    assert!(
        compressed_pixels == raw_pixels,
        "flate is lossless: the page must rasterize identically"
    );
}
