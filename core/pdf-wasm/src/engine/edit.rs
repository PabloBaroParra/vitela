//! The `Engine`'s edit surface: page edits, undo/redo, save, and the preview
//! refresh (B26 T-224).
//!
//! The session rules live in [`crate::model`]'s edit module, over the core;
//! this is the JavaScript boundary plus the one step that needs pdfium:
//! reopening the preview snapshot as the document's render side.
//!
//! As in `pdf-ffi`, an edit changes the model at once (`pageCount`,
//! `pageDimensions`, `annotations`) but not the pixels or the text: those
//! come from the pdfium document, which still holds the last snapshot. After
//! every edit, undo or redo, ask `previewStale` and call `refreshPreview`
//! when it says so before rendering or reading text again. Annotation edits
//! never make it stale: the preview leaves the session's annotations out and
//! the shell draws them from `annotations` (the core's rule,
//! `pdf_save::shows_in_session_preview`); a form fill does, because pdfium
//! paints the field.

use pdf_document::Orientation;
use wasm_bindgen::prelude::*;

use super::{js_error, Engine};
use crate::model::{
    AnnotationEdit, DocumentProperties, FieldFill, PageEdit, TextRunEdit, WatermarkRequest,
};

/// What an `EditCommand` carries: a page edit, an annotation edit, a form
/// fill, a change of the document's properties, a watermark or an edit of a
/// page's text runs.
#[derive(Debug, Clone)]
pub(super) enum Edit {
    Page(PageEdit),
    Annotation(AnnotationEdit),
    Form(FieldFill),
    Metadata(DocumentProperties),
    Watermark(WatermarkRequest),
    TextRun(TextRunEdit),
}

/// One edit for `Engine.applyEdit`: a page edit (here), an annotation edit
/// (`engine/annotation.rs`), a form fill (`engine/form.rs`) or a change of
/// the document's properties (`engine/metadata.rs`) or a watermark
/// (`engine/watermark.rs`) or an edit of a page's text runs
/// (`engine/text_run.rs`). Positions are
/// 0-based.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct EditCommand(pub(super) Edit);

#[wasm_bindgen]
impl EditCommand {
    /// Moves the `count` pages starting at `from` so the first of them ends
    /// up at `to`.
    #[wasm_bindgen(js_name = movePages)]
    pub fn move_pages(from: u32, count: u32, to: u32) -> EditCommand {
        EditCommand(Edit::Page(PageEdit::Move { from, count, to }))
    }

    /// Turns each of the `count` pages starting at `from` clockwise by
    /// `delta_degrees`, as one undo step.
    #[wasm_bindgen(js_name = rotatePages)]
    pub fn rotate_pages(from: u32, count: u32, delta_degrees: i32) -> EditCommand {
        EditCommand(Edit::Page(PageEdit::Rotate {
            from,
            count,
            delta_degrees,
        }))
    }

    /// Removes the `count` pages starting at `index`, as one undo step.
    #[wasm_bindgen(js_name = removePages)]
    pub fn remove_pages(index: u32, count: u32) -> EditCommand {
        EditCommand(Edit::Page(PageEdit::Remove { index, count }))
    }

    /// Inserts one blank A4 page — portrait, or `landscape` — so it ends up
    /// at position `index` (`pageCount` appends), as one undo step. The new
    /// page gets a `pageIds` entry of its own.
    #[wasm_bindgen(js_name = insertBlankPage)]
    pub fn insert_blank_page(index: u32, landscape: bool) -> EditCommand {
        let orientation = if landscape {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        };
        EditCommand(Edit::Page(PageEdit::InsertBlank { index, orientation }))
    }
}

#[wasm_bindgen]
impl Engine {
    /// Records one edit as one undo step and answers the id of the
    /// annotation it added, if it added one.
    ///
    /// A page edit is refused with `EditNotPermitted` when the document
    /// withholds the assembly permission (asked first),
    /// `PageIndexOutOfBounds` naming a position that does not exist, or
    /// `UnsupportedOperation` when the edit cannot be recorded (a move onto
    /// itself, an empty run, a rewrite the document's encryption does not
    /// allow, or an insert with no page id left to give). An annotation edit is refused with `EditNotPermitted` when
    /// the document withholds the annotate permission (asked first), then
    /// `PageIndexOutOfBounds`, `AnnotationNotFound`, `EncodingGap`,
    /// `InvalidImage` or `UnsupportedOperation`; a retext to the text a box
    /// already has records nothing. A form fill is refused with
    /// `EditNotPermitted` when the document withholds the annotate-or-fill
    /// permission (asked first), then `FormFieldNotFound` or
    /// `UnsupportedOperation` (a value the field cannot hold); unlike an
    /// annotation edit it makes `previewStale` true. A properties change is
    /// refused with `EditNotPermitted` when the document withholds
    /// modification (`metadataRefusal`); the values it already has record
    /// nothing. A watermark is refused as `watermarkRefusal` says (asked
    /// first), then `PageIndexOutOfBounds`, `EncodingGap`, `InvalidImage` or
    /// `UnsupportedOperation`; like a form fill it makes `previewStale`
    /// true. A text-run edit is refused as `contentEditingRefusal` says
    /// (asked first), then `PageIndexOutOfBounds`, `UnsupportedOperation`
    /// for a run the page does not hold now, `EncodingGap` for a character
    /// the run's font cannot show; it makes `previewStale` true and moves
    /// `paintRevision`.
    #[wasm_bindgen(js_name = applyEdit)]
    pub fn apply_edit(&mut self, doc: u32, edit: &EditCommand) -> Result<Option<u32>, JsValue> {
        self.document_mut(doc)
            .and_then(|open| match &edit.0 {
                Edit::Page(page) => open.model.apply_edit(*page).map(|()| None),
                // Cloned: the command stays the caller's (and a stamp's
                // image is moved into the model).
                Edit::Annotation(annotation) => {
                    open.model.apply_annotation_edit(annotation.clone())
                }
                Edit::Form(fill) => open.model.fill_form_field(fill.clone()).map(|()| None),
                Edit::Metadata(properties) => open
                    .model
                    .set_document_properties(properties.clone())
                    .map(|()| None),
                Edit::Watermark(request) => {
                    open.model.add_watermark(request.clone()).map(|()| None)
                }
                Edit::TextRun(edit) => open.model.apply_text_run_edit(edit.clone()).map(|()| None),
            })
            .map_err(js_error)
    }

    /// Undoes the last edit; `false` when there was nothing to undo.
    pub fn undo(&mut self, doc: u32) -> Result<bool, JsValue> {
        self.document_mut(doc)
            .map(|open| open.model.undo())
            .map_err(js_error)
    }

    /// Redoes the last undone edit; `false` when there was nothing to redo.
    pub fn redo(&mut self, doc: u32) -> Result<bool, JsValue> {
        self.document_mut(doc)
            .map(|open| open.model.redo())
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = canUndo)]
    pub fn can_undo(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.can_undo())
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = canRedo)]
    pub fn can_redo(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.can_redo())
            .map_err(js_error)
    }

    /// The document with every edit applied, as a complete PDF computed from
    /// the bytes it was opened from, keeping its encryption. A save that
    /// would break a signature is refused with `SignaturesWouldBeInvalidated`
    /// unless `invalidateSignatures` says the user was told.
    #[wasm_bindgen(js_name = saveToBytes)]
    pub fn save_to_bytes(&self, doc: u32, invalidate_signatures: bool) -> Result<Vec<u8>, JsValue> {
        self.document(doc)
            .and_then(|open| open.model.save_to_bytes(invalidate_signatures))
            .map_err(js_error)
    }

    /// Reopens the document's render side from a snapshot of its unsaved
    /// edits, so pages, thumbnails and text show them. The edit history and
    /// the next save are untouched.
    #[wasm_bindgen(js_name = refreshPreview)]
    pub fn refresh_preview(&mut self, doc: u32) -> Result<(), JsValue> {
        self.reopen_render(doc).map_err(js_error)
    }

    /// Which repaint of the pages in place the history stands at: the
    /// newest page-content step still applied (a watermark), 0 for the file
    /// as opened. A page whose id and turn are unchanged shows the same
    /// pixels at the same revision; a page move, a rotation or an annotation
    /// leaves it as it is. A number, not a `BigInt`: it counts steps.
    #[wasm_bindgen(js_name = paintRevision)]
    pub fn paint_revision(&self, doc: u32) -> Result<f64, JsValue> {
        self.document(doc)
            .map(|open| open.model.paint_revision() as f64)
            .map_err(js_error)
    }

    /// Whether the edit history stands at a state the pixels and the text,
    /// as of the last `refreshPreview` (or the open), do not show — when the
    /// shell has to call `refreshPreview` before rendering again. Exact:
    /// undoing a page step that was never shown leaves it `false` again.
    /// Annotation edits never make it `true`.
    #[wasm_bindgen(js_name = previewStale)]
    pub fn preview_stale(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.preview_stale())
            .map_err(js_error)
    }
}
