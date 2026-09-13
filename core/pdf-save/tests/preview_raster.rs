//! What `save_preview` is actually for, checked where the bug was visible:
//! the raster.
//!
//! The object-tree tests in `strategy.rs` pin what the writers put in the
//! file. These pin the consequence — that pdfium paints a model annotation
//! from a real save and paints nothing from a preview — which is the only
//! reason the distinction exists. A shell overlay draws that annotation
//! itself, so a preview whose raster already carries it shows it twice.

use pdf_document::{
    Annotation, AnnotationId, AnnotationKind, Color, Command, FieldOrigin, FieldValue, FontFamily,
    FormField, FormFieldId, FormFieldKind, Orientation, PageId, PageSize, Rect, TextStyle,
};
use pdf_save::{save_document, save_preview, SaveInput, SaveIntent, SignatureAcknowledgement};

/// Page geometry in PDF points. At 72 DPI one point is one pixel, so a
/// rectangle in user space indexes the bitmap directly — the whole reason the
/// sampling below can be written by hand.
const PAGE_WIDTH_PT: f64 = 595.0;
const PAGE_HEIGHT_PT: f64 = 842.0;

const HIGHLIGHT: Rect = Rect {
    x: 100.0,
    y: 400.0,
    width: 200.0,
    height: 100.0,
};

fn apply_command(document: &mut pdf_document::Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

/// One blank A4 page carrying one saturated-red Highlight in the model —
/// nothing else, so any non-white pixel in the rendered page came from that
/// annotation.
fn document_with_one_highlight() -> (pdf_document::Document, pdf_manip::LopdfDocument) {
    let base = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
    let mut document = pdf_save::document_from_lopdf(&base, None).unwrap();
    apply_command(
        &mut document,
        Command::insert_page(
            0,
            pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait),
        ),
    );
    apply_command(
        &mut document,
        Command::AddAnnotation(Annotation {
            id: AnnotationId(1),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: HIGHLIGHT,
                color: Color { r: 255, g: 0, b: 0 },
            },
        }),
    );
    (document, base)
}

/// The pixel at the middle of [`HIGHLIGHT`], as `(r, g, b)`.
///
/// PDF user space puts the origin at the bottom-left and the bitmap puts it
/// at the top-left, so the row is measured down from the page's top edge.
fn highlight_center_pixel(bytes: Vec<u8>) -> (u8, u8, u8) {
    let renderer = pdf_render::PdfiumRenderer::new();
    let doc = renderer
        .open_document_from_bytes(bytes, None)
        .expect("saved bytes must reopen in pdfium");
    let bitmap = renderer
        .render_page(
            doc,
            0,
            72,
            None,
            pdf_render::RenderOptions::default(),
            pdf_render::Priority::Visible,
        )
        .wait()
        .expect("page one must render");

    let width = bitmap.width().unwrap() as usize;
    let pixels = bitmap.get_pixels().unwrap();

    let x = (HIGHLIGHT.x + HIGHLIGHT.width / 2.0) as usize;
    let y = (PAGE_HEIGHT_PT - (HIGHLIGHT.y + HIGHLIGHT.height / 2.0)) as usize;
    let offset = (y * width + x) * 4;
    (pixels[offset], pixels[offset + 1], pixels[offset + 2])
}

fn input<'a>(
    document: &'a pdf_document::Document,
    base: &'a pdf_manip::LopdfDocument,
) -> SaveInput<'a> {
    SaveInput {
        document,
        base,
        original_bytes: None,
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::none(),
    }
}

/// The premise. Without this, nothing about `save_preview` would matter: it
/// is only because pdfium *does* rasterize a model annotation out of a saved
/// file that a preview built the same way ends up showing it a second time,
/// under the copy the shell's overlay draws.
#[test]
fn a_real_save_puts_the_model_highlight_into_pdfiums_raster() {
    let (document, base) = document_with_one_highlight();

    let (red, green, blue) =
        highlight_center_pixel(save_document(input(&document, &base)).expect("save"));

    assert!(
        red > 200 && green < 120 && blue < 120,
        "pdfium must paint the saved Highlight red, got ({red}, {green}, {blue})"
    );
}

/// The fix. Same document, same page, same renderer — only the entry point
/// changes, and the annotation the overlay owns is gone from the raster.
#[test]
fn a_preview_save_leaves_pdfiums_raster_clean_for_the_overlay() {
    let (document, base) = document_with_one_highlight();

    let (red, green, blue) =
        highlight_center_pixel(save_preview(input(&document, &base)).expect("preview"));

    assert_eq!(
        (red, green, blue),
        (255, 255, 255),
        "a preview page must stay blank where the overlay is about to draw"
    );
}

/// A preview is still a real render of the document: the blank page the model
/// asks for exists at the size it asks for, so the sampling above is reading
/// a page and not an accident.
#[test]
fn a_preview_save_still_renders_the_page_the_model_describes() {
    let (document, base) = document_with_one_highlight();
    let bytes = save_preview(input(&document, &base)).expect("preview");

    let renderer = pdf_render::PdfiumRenderer::new();
    let doc = renderer
        .open_document_from_bytes(bytes, None)
        .expect("preview bytes must reopen");
    let bitmap = renderer
        .render_page(
            doc,
            0,
            72,
            None,
            pdf_render::RenderOptions::default(),
            pdf_render::Priority::Visible,
        )
        .wait()
        .expect("page one must render");

    assert_eq!(bitmap.width().unwrap(), PAGE_WIDTH_PT as u32);
    assert_eq!(bitmap.height().unwrap(), PAGE_HEIGHT_PT as u32);
}

// --- What pdfium draws for itself ----------------------------------------

const FIELD: Rect = Rect {
    x: 100.0,
    y: 200.0,
    width: 200.0,
    height: 40.0,
};

/// One blank A4 page carrying one filled text field in the model.
fn document_with_one_filled_field() -> (pdf_document::Document, pdf_manip::LopdfDocument) {
    let base = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
    let mut document = pdf_save::document_from_lopdf(&base, None).unwrap();
    apply_command(
        &mut document,
        Command::insert_page(
            0,
            pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait),
        ),
    );
    apply_command(
        &mut document,
        Command::AddFormField(FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Name".to_string(),
            rect: FIELD,
            style: TextStyle {
                font: FontFamily::Helvetica,
                size_pt: 24.0,
                color: Color { r: 0, g: 0, b: 0 },
            },
            // Wide glyphs at 24pt, so the ink is unmistakable against the
            // blank page and the count below cannot be antialiasing noise.
            value: FieldValue::Text("AAAAAAAA".to_string()),
            kind: FormFieldKind::Text {
                multiline: false,
                max_len: None,
            },
            origin: FieldOrigin::New,
        }),
    );
    (document, base)
}

/// Pixels inside [`FIELD`] that are not the blank page's white.
fn ink_inside_the_field(bytes: Vec<u8>) -> usize {
    let renderer = pdf_render::PdfiumRenderer::new();
    let doc = renderer
        .open_document_from_bytes(bytes, None)
        .expect("saved bytes must reopen in pdfium");
    let bitmap = renderer
        .render_page(
            doc,
            0,
            72,
            None,
            pdf_render::RenderOptions::default(),
            pdf_render::Priority::Visible,
        )
        .wait()
        .expect("page one must render");

    let width = bitmap.width().unwrap() as usize;
    let pixels = bitmap.get_pixels().unwrap();

    let top = (PAGE_HEIGHT_PT - (FIELD.y + FIELD.height)) as usize;
    let bottom = (PAGE_HEIGHT_PT - FIELD.y) as usize;
    let mut ink = 0;
    for y in top..bottom {
        for x in (FIELD.x as usize)..((FIELD.x + FIELD.width) as usize) {
            let offset = (y * width + x) * 4;
            if pixels[offset] < 250 || pixels[offset + 1] < 250 || pixels[offset + 2] < 250 {
                ink += 1;
            }
        }
    }
    ink
}

/// **The premise behind the Linux shell's overlay filter.** pdfium rasterizes
/// a form field's value out of the file all on its own, from the widget's
/// `/AP` — so a shell that also paints values from its model must skip the
/// ones the open bytes already carry, or every filled field of an opened form
/// comes out doubled. Delete this test and the filter in
/// `selection::overlay_owns_field_value` starts looking like dead weight.
#[test]
fn pdfium_rasterizes_a_form_field_value_from_the_saved_file() {
    let (document, base) = document_with_one_filled_field();

    let ink = ink_inside_the_field(save_document(input(&document, &base)).expect("save"));

    assert!(
        ink > 200,
        "pdfium must draw the field's value; found only {ink} non-white pixels in its rect"
    );
}

/// And a preview draws it too — unlike the highlight above. A field's
/// appearance is pdfium's to render, so the preview carries it and the shell
/// refreshes on every form-field command rather than approximating the glyphs
/// on an overlay.
#[test]
fn a_preview_save_still_draws_a_form_field_because_pdfium_owns_it() {
    let (document, base) = document_with_one_filled_field();

    let ink = ink_inside_the_field(save_preview(input(&document, &base)).expect("preview"));

    assert!(
        ink > 200,
        "a preview must carry the field pdfium draws; found only {ink} non-white pixels"
    );
}
