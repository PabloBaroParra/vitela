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
