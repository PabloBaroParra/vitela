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
    let insert = Command::insert_blank_page(&mut document, 0, PageSize::A4, Orientation::Portrait)
        .expect("a zero-page document has ids to spare");
    apply_command(&mut document, insert);
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
    let insert = Command::insert_blank_page(&mut document, 0, PageSize::A4, Orientation::Portrait)
        .expect("a zero-page document has ids to spare");
    apply_command(&mut document, insert);
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

// --- FreeText ------------------------------------------------------------

const BOX: Rect = Rect {
    x: 100.0,
    y: 500.0,
    width: 200.0,
    height: 60.0,
};

fn document_with_free_text(
    contents: &str,
) -> (pdf_document::Document, pdf_manip::LopdfDocument, Annotation) {
    let base = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
    let mut document = pdf_save::document_from_lopdf(&base, None).unwrap();
    let insert = Command::insert_blank_page(&mut document, 0, PageSize::A4, Orientation::Portrait)
        .expect("a zero-page document has ids to spare");
    apply_command(&mut document, insert);
    let annotation = pdf_annotate::free_text(AnnotationId(1), PageId(0), BOX, contents)
        .expect("valid free text");
    apply_command(&mut document, Command::AddAnnotation(annotation.clone()));
    (document, base, annotation)
}

/// The page at 72 DPI as `(width, rgba)`: one pixel per point.
fn rasterize(bytes: Vec<u8>) -> (usize, Vec<u8>) {
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
    (
        bitmap.width().unwrap() as usize,
        bitmap.get_pixels().unwrap().to_vec(),
    )
}

fn is_ink(pixels: &[u8], width: usize, x: usize, y: usize) -> bool {
    let offset = (y * width + x) * 4;
    pixels[offset] < 200 || pixels[offset + 1] < 200 || pixels[offset + 2] < 200
}

/// Page row (measured down from the top) of a PDF y coordinate.
fn row_of(pdf_y: f64) -> usize {
    (PAGE_HEIGHT_PT - pdf_y).round() as usize
}

fn box_columns() -> std::ops::Range<usize> {
    (BOX.x as usize)..((BOX.x + BOX.width) as usize)
}

/// Pixels of ink inside [`BOX`].
fn ink_inside_the_box(bytes: Vec<u8>) -> usize {
    let (width, pixels) = rasterize(bytes);
    let mut ink = 0;
    for y in row_of(BOX.y + BOX.height)..row_of(BOX.y) {
        for x in box_columns() {
            ink += usize::from(is_ink(&pixels, width, x, y));
        }
    }
    ink
}

/// Pixels of ink anywhere on the page outside [`BOX`].
fn ink_outside_the_box(bytes: Vec<u8>) -> usize {
    let (width, pixels) = rasterize(bytes);
    let rows = pixels.len() / 4 / width;
    let inside_rows = row_of(BOX.y + BOX.height)..row_of(BOX.y);
    let mut ink = 0;
    for y in 0..rows {
        for x in 0..width {
            let inside = inside_rows.contains(&y) && box_columns().contains(&x);
            ink += usize::from(!inside && is_ink(&pixels, width, x, y));
        }
    }
    ink
}

/// Rows (page space, top-down) inside [`BOX`] that carry any ink.
fn inked_rows(bytes: Vec<u8>) -> Vec<usize> {
    let (width, pixels) = rasterize(bytes);
    (row_of(BOX.y + BOX.height)..row_of(BOX.y))
        .filter(|&y| box_columns().any(|x| is_ink(&pixels, width, x, y)))
        .collect()
}

fn saved(document: &pdf_document::Document, base: &pdf_manip::LopdfDocument) -> Vec<u8> {
    save_document(input(document, base)).expect("save")
}

/// The premise of the whole feature: a real save puts the text into
/// pdfium's raster through the `/AP` we wrote, inside the box and nowhere
/// else.
#[test]
fn a_saved_free_text_is_painted_by_pdfium_inside_its_box_only() {
    let (document, base, _) = document_with_free_text("Año ¿cómo está?");
    let bytes = saved(&document, &base);

    assert!(
        ink_inside_the_box(bytes.clone()) > 50,
        "pdfium must paint the text from the saved /AP"
    );
    assert_eq!(
        ink_outside_the_box(bytes),
        0,
        "no ink may land outside the box"
    );
}

/// The shell overlay is the sole painter in the editor, so a preview must
/// carry no FreeText ink at all or the text shows twice.
#[test]
fn a_preview_save_has_no_free_text_ink_because_the_overlay_owns_it() {
    let (document, base, _) = document_with_free_text("Año ¿cómo está?");

    let ink = ink_inside_the_box(save_preview(input(&document, &base)).expect("preview"));

    assert_eq!(ink, 0, "a preview painting the text would double it");
}

/// Layout parity at the raster: one band of ink per laid-out line, with the
/// rows between bands blank, so the wrap pdfium paints is the wrap the
/// shells draw.
#[test]
fn the_painted_lines_match_the_layout_line_for_line() {
    let (document, base, annotation) = document_with_free_text("Hola mundo\nsegunda linea");
    let AnnotationKind::FreeText {
        contents, style, ..
    } = &annotation.kind
    else {
        unreachable!()
    };
    let layout = pdf_annotate::layout_free_text(contents, style, BOX.width, BOX.height).unwrap();
    assert_eq!(layout.lines.len(), 2);

    let rows = inked_rows(saved(&document, &base));

    // Contiguous runs of inked rows are the painted lines.
    let mut bands: Vec<(usize, usize)> = Vec::new();
    for row in rows {
        match bands.last_mut() {
            Some((_, end)) if *end + 1 == row => *end = row,
            _ => bands.push((row, row)),
        }
    }
    assert_eq!(bands.len(), layout.lines.len(), "bands: {bands:?}");

    let size = style.size_pt;
    for (line, (first, last)) in layout.lines.iter().zip(&bands) {
        let baseline_row = row_of(BOX.y + BOX.height) as f64 + line.baseline_from_top_pt;
        assert!(
            (*first as f64) >= baseline_row - 0.8 * size
                && (*last as f64) <= baseline_row + 0.3 * size,
            "{:?} painted rows {first}..={last}, baseline row {baseline_row}",
            line.text
        );
    }
}

/// The accent is in the raster: a tilde sits above x-height, where plain
/// `n` has nothing.
#[test]
fn accents_reach_the_raster_above_the_x_height() {
    let size = 12.0;
    let probe = |text: &str| {
        let (document, base, annotation) = document_with_free_text(text);
        let AnnotationKind::FreeText {
            style, contents, ..
        } = &annotation.kind
        else {
            unreachable!()
        };
        let layout =
            pdf_annotate::layout_free_text(contents, style, BOX.width, BOX.height).unwrap();
        let baseline_row = row_of(BOX.y + BOX.height) as f64 + layout.lines[0].baseline_from_top_pt;
        // Strictly above the x-height (0.523 em) and below the cap height.
        let above_x_height = (baseline_row - 0.72 * size)..(baseline_row - 0.56 * size);
        inked_rows(saved(&document, &base))
            .into_iter()
            .filter(|row| above_x_height.contains(&(*row as f64)))
            .count()
    };

    assert!(probe("ñññ") > 0, "the tilde must be painted");
    assert_eq!(probe("nnn"), 0, "plain n has nothing above x-height");
}

/// Measured, not assumed: given a FreeText with no `/AP`, pdfium paints a
/// default appearance of its own (a box and the text, not ours). That is
/// why the save always writes an `/AP` it controls, and why a preview must
/// leave the annotation out entirely instead of relying on pdfium to stay
/// quiet. Delete this test and the always-write-`/AP` rule loses its reason.
#[test]
fn pdfium_synthesizes_its_own_free_text_appearance_when_the_ap_is_missing() {
    let (document, base, _) = document_with_free_text("Año ¿cómo está?");
    let bytes = saved(&document, &base);

    let mut doc = lopdf::Document::load_mem(&bytes).expect("saved bytes reload");
    let free_text_ids: Vec<lopdf::ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let dict = object.as_dict().ok()?;
            (dict.get(b"Subtype").ok()?.as_name().ok()? == b"FreeText").then_some(*id)
        })
        .collect();
    assert_eq!(free_text_ids.len(), 1);
    for id in free_text_ids {
        doc.get_dictionary_mut(id).unwrap().remove(b"AP");
    }
    let mut stripped = Vec::new();
    doc.save_to(&mut stripped).expect("re-save");

    let with_ap = ink_inside_the_box(bytes);
    let without_ap = ink_inside_the_box(stripped);
    assert!(
        without_ap > 0,
        "pdfium paints a default FreeText appearance"
    );
    assert_ne!(
        with_ap, without_ap,
        "the synthesized appearance is not ours, which is the point of writing one"
    );
}
