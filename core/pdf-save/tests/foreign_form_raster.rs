//! A foreign form's fields, checked where the bug was visible: the raster.
//!
//! Seen on Android (2026-10-09) with `reportlab_acroform.pdf`: the first
//! form-field edit refreshed the preview, and from then on *every* field lost
//! its `/MK` border box and the checkbox/radio marks came out as the glyph
//! codes `4` and `l` in a fallback font — including after undoing back to
//! zero edits. Nothing about it is Android's: the FFI `refresh_preview`, the
//! GTK `save_preview` and a real save all run the same field writer.

use pdf_document::{Command, Document, FormField};
use pdf_save::{
    document_from_lopdf, save_document, save_preview, SaveInput, SaveIntent,
    SignatureAcknowledgement,
};

fn fixture_bytes() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("forms")
        .join("reportlab_acroform.pdf");
    std::fs::read(path).expect("read the reportlab fixture")
}

fn open(bytes: &[u8]) -> (Document, pdf_manip::LopdfDocument) {
    let (base, security) = pdf_manip::open_document_from_bytes(bytes, None).expect("open");
    let document = document_from_lopdf(&base, security).expect("model");
    (document, base)
}

fn input<'a>(document: &'a Document, base: &'a pdf_manip::LopdfDocument) -> SaveInput<'a> {
    SaveInput {
        document,
        base,
        original_bytes: None,
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::none(),
    }
}

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    assert!(log.apply(document, command), "the command must apply");
    document.pending_edits = log;
}

fn field<'a>(document: &'a Document, name: &str) -> &'a FormField {
    document
        .form_fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("the fixture has a `{name}` field"))
}

/// Page one at 72 DPI as `(width, height, rgba)`: one pixel per point.
fn rasterize(bytes: Vec<u8>) -> (usize, usize, Vec<u8>) {
    let renderer = pdf_render::PdfiumRenderer::new();
    let doc = renderer
        .open_document_from_bytes(bytes, None)
        .expect("bytes must reopen in pdfium");
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
        bitmap.height().unwrap() as usize,
        bitmap.get_pixels().unwrap().to_vec(),
    )
}

/// The pixels covering every widget of `field`, one point of margin around
/// each so the border stroke is inside the sample.
fn widget_pixels(raster: &(usize, usize, Vec<u8>), field: &FormField) -> Vec<u8> {
    let (width, height, pixels) = raster;
    let rects = match &field.kind {
        pdf_document::FormFieldKind::RadioGroup { options } => {
            options.iter().map(|option| option.rect).collect()
        }
        _ => vec![field.rect],
    };
    let mut sample = Vec::new();
    for rect in rects {
        let top = *height as f64 - (rect.y + rect.height) - 1.0;
        let bottom = *height as f64 - rect.y + 1.0;
        for y in (top.max(0.0) as usize)..(bottom as usize).min(*height) {
            for x in ((rect.x - 1.0).max(0.0) as usize)..((rect.x + rect.width + 1.0) as usize) {
                let offset = (y * width + x) * 4;
                sample.extend_from_slice(&pixels[offset..offset + 4]);
            }
        }
    }
    sample
}

const UNTOUCHED: [&str; 4] = ["full_name", "subscribe", "plan", "country"];

fn assert_untouched_fields_render_as_the_original(after: Vec<u8>, what: &str) {
    let bytes = fixture_bytes();
    let (document, _) = open(&bytes);
    let original = rasterize(bytes);
    let edited = rasterize(after);
    for name in UNTOUCHED {
        let field = field(&document, name);
        assert!(
            widget_pixels(&original, field) == widget_pixels(&edited, field),
            "{what}: `{name}` was not edited, yet it no longer renders as the file drew it"
        );
    }
}

fn with_notes_deleted() -> (Document, pdf_manip::LopdfDocument) {
    let (mut document, base) = open(&fixture_bytes());
    let notes = field(&document, "notes").clone();
    apply_command(&mut document, Command::RemoveFormField(notes));
    (document, base)
}

/// The Android repro: Delete one field, refresh the preview.
#[test]
fn a_preview_after_deleting_one_field_leaves_the_others_as_the_file_drew_them() {
    let (document, base) = with_notes_deleted();
    let preview = save_preview(input(&document, &base)).expect("preview");
    assert_untouched_fields_render_as_the_original(preview, "preview");
}

/// The same writer runs on a real save, so the file on disk had it too.
#[test]
fn a_save_after_deleting_one_field_leaves_the_others_as_the_file_drew_them() {
    let (document, base) = with_notes_deleted();
    let saved = save_document(input(&document, &base)).expect("save");
    assert_untouched_fields_render_as_the_original(saved, "save");
}

/// "Undoing back to zero pending edits does not fix it" — with nothing
/// pending, the bytes must render exactly as the file did.
#[test]
fn a_preview_after_undoing_every_edit_renders_the_file_unchanged() {
    let (mut document, base) = with_notes_deleted();
    let mut log = std::mem::take(&mut document.pending_edits);
    assert!(log.undo(&mut document));
    document.pending_edits = log;

    let preview = save_preview(input(&document, &base)).expect("preview");
    assert_untouched_fields_render_as_the_original(preview, "undone preview");
}

// --- Edited fields keep their frame ----------------------------------------

/// Dark pixels in the band `inset` points deep along `rect`'s edges — where a
/// `/MK` border is stroked — or, with `inset` covering the rect, in all of it.
fn ink_near_the_edge(
    raster: &(usize, usize, Vec<u8>),
    rect: pdf_document::Rect,
    inset: f64,
) -> usize {
    let (width, height, pixels) = raster;
    let top = (*height as f64 - (rect.y + rect.height)).round() as usize;
    let bottom = (*height as f64 - rect.y).round() as usize;
    let left = rect.x.round() as usize;
    let right = (rect.x + rect.width).round() as usize;
    let band = inset.round() as usize;
    let mut ink = 0;
    for y in top..bottom {
        for x in left..right {
            let near = y < top + band || y >= bottom - band || x < left + band || x >= right - band;
            let offset = (y * width + x) * 4;
            let dark = pixels[offset] < 200 || pixels[offset + 1] < 200 || pixels[offset + 2] < 200;
            if near && dark {
                ink += 1;
            }
        }
    }
    ink
}

fn edited(edit: impl FnOnce(&mut Document)) -> (usize, usize, Vec<u8>) {
    let (mut document, base) = open(&fixture_bytes());
    edit(&mut document);
    rasterize(save_preview(input(&document, &base)).expect("preview"))
}

fn set_value(document: &mut Document, name: &str, to: pdf_document::FieldValue) {
    let field = field(document, name).clone();
    apply_command(
        document,
        Command::SetFieldValue {
            id: field.id,
            from: field.value,
            to,
        },
    );
}

/// Most of the original's border ink must still be there. Not exact: our
/// stroke is the same geometry as reportlab's, but antialiasing is pdfium's.
fn assert_frame_kept(original: usize, now: usize, what: &str) {
    assert!(
        original > 20,
        "{what}: the fixture must draw a border to begin with ({original})"
    );
    assert!(
        now * 10 >= original * 9,
        "{what}: the edited field lost its /MK border ({now} dark edge pixels, the file had {original})"
    );
}

#[test]
fn an_edited_text_field_keeps_its_border() {
    let bytes = fixture_bytes();
    let rect = field(&open(&bytes).0, "full_name").rect;
    let original = ink_near_the_edge(&rasterize(bytes), rect, 2.0);
    let now = ink_near_the_edge(
        &edited(|document| {
            set_value(
                document,
                "full_name",
                pdf_document::FieldValue::Text("Grace Hopper".into()),
            )
        }),
        rect,
        2.0,
    );
    assert_frame_kept(original, now, "full_name");
}

#[test]
fn an_unchecked_checkbox_keeps_its_box() {
    let bytes = fixture_bytes();
    let rect = field(&open(&bytes).0, "subscribe").rect;
    let original = ink_near_the_edge(&rasterize(bytes), rect, 2.0);
    let now = ink_near_the_edge(
        &edited(|document| {
            set_value(
                document,
                "subscribe",
                pdf_document::FieldValue::Checked(false),
            )
        }),
        rect,
        2.0,
    );
    assert_frame_kept(original, now, "subscribe");
}

/// Swapping the selection swaps which kid draws the dot; the circle each one
/// is drawn in must survive. Compared kid-for-state: the kid now off against
/// the one that was off in the file.
#[test]
fn a_reselected_radio_group_keeps_its_circles() {
    let bytes = fixture_bytes();
    let pdf_document::FormFieldKind::RadioGroup { options } =
        field(&open(&bytes).0, "plan").kind.clone()
    else {
        panic!("plan is a radio group");
    };
    let (basic, pro) = (options[0].rect, options[1].rect);
    let file = rasterize(bytes);
    let now = edited(|document| {
        set_value(
            document,
            "plan",
            pdf_document::FieldValue::Choice(Some("basic".into())),
        )
    });
    let whole = 100.0;
    assert_frame_kept(
        ink_near_the_edge(&file, basic, whole),
        ink_near_the_edge(&now, pro, whole),
        "plan, off",
    );
    assert_frame_kept(
        ink_near_the_edge(&file, pro, whole),
        ink_near_the_edge(&now, basic, whole),
        "plan, on",
    );
}

/// The dot itself: pixels dark in the selected kid but not at the same spot
/// in the unselected one. Both kids carry the same frame, so what is left is
/// what the "on" state adds. Returned as `(area, centre offset from the
/// kid's centre)` in points.
fn the_dot(
    raster: &(usize, usize, Vec<u8>),
    on: pdf_document::Rect,
    off: pdf_document::Rect,
) -> (usize, (f64, f64)) {
    let (width, height, pixels) = raster;
    let dark = |x: usize, y: usize| {
        let offset = (y * width + x) * 4;
        pixels[offset] < 128 || pixels[offset + 1] < 128 || pixels[offset + 2] < 128
    };
    let top = |rect: pdf_document::Rect| (*height as f64 - (rect.y + rect.height)).round() as usize;
    let (side_x, side_y) = (on.width.round() as usize, on.height.round() as usize);
    let (mut area, mut sum_x, mut sum_y) = (0usize, 0.0, 0.0);
    for dy in 0..side_y {
        for dx in 0..side_x {
            let in_on = dark(on.x.round() as usize + dx, top(on) + dy);
            let in_off = dark(off.x.round() as usize + dx, top(off) + dy);
            if in_on && !in_off {
                area += 1;
                sum_x += dx as f64 + 0.5;
                sum_y += dy as f64 + 0.5;
            }
        }
    }
    let n = area.max(1) as f64;
    (
        area,
        (sum_x / n - on.width / 2.0, sum_y / n - on.height / 2.0),
    )
}

/// The dot reportlab draws: a circle of radius 0.2 × the button's side,
/// centred. Ours used to be the ZapfDingbats `l` at 0.8 × the side, placed
/// by fixed fractions — much bigger, and off centre down and to the left.
#[test]
fn a_reselected_radio_draws_a_centred_dot_the_size_the_file_uses() {
    let bytes = fixture_bytes();
    let pdf_document::FormFieldKind::RadioGroup { options } =
        field(&open(&bytes).0, "plan").kind.clone()
    else {
        panic!("plan is a radio group");
    };
    let (basic, pro) = (options[0].rect, options[1].rect);
    let now = edited(|document| {
        set_value(
            document,
            "plan",
            pdf_document::FieldValue::Choice(Some("basic".into())),
        )
    });

    let (area, (off_x, off_y)) = the_dot(&now, basic, pro);
    let radius = 0.2 * basic.width.min(basic.height);
    let expected = std::f64::consts::PI * radius * radius;
    assert!(
        (area as f64 - expected).abs() <= 0.35 * expected,
        "the dot covers {area} px; a radius-{radius} circle covers about {expected:.0}"
    );
    assert!(
        off_x.abs() <= 0.5 && off_y.abs() <= 0.5,
        "the dot's centre is ({off_x:.2}, {off_y:.2}) pt from the button's"
    );
}
