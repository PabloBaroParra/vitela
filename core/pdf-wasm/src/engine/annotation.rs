//! The `Engine`'s annotation surface (B26 T-236): the annotation
//! `EditCommand`s `applyEdit` records, the snapshot and comments a shell
//! draws and lists, the annotate permission, and the free-text and stamp
//! geometry asked before an annotation exists.
//!
//! The rules live in [`crate::model`]'s annotation module, over the core;
//! this is the JavaScript boundary. Geometry is in the page's unrotated PDF
//! space with a bottom-left origin, as `pdf-ffi` takes it; a colour is one
//! number, `0xRRGGBB`. An annotation edit leaves `previewStale` alone: the
//! shell draws the session's annotations from `annotations`, not from the
//! pixels.

use pdf_annotate::{AnnotationChange, NewAnnotation};
use wasm_bindgen::prelude::*;

use super::edit::Edit;
use super::{js_error, EditCommand, Engine};
use crate::convert::annotation::{points_from_flat, rect, rect_values, unpack_rgb};
use crate::dto::{AnnotationEntry, CommentEntry, FreeTextLines};
use crate::model::AnnotationEdit;

fn add(page: u32, annotation: NewAnnotation) -> EditCommand {
    EditCommand(Edit::Annotation(AnnotationEdit::Add { page, annotation }))
}

fn change(id: u32, change: AnnotationChange) -> EditCommand {
    EditCommand(Edit::Annotation(AnnotationEdit::Change { id, change }))
}

#[wasm_bindgen]
impl EditCommand {
    /// Highlights the box at (`x`, `y`) of `width` x `height` on the page at
    /// `page`, in colour `rgb`. One box, one annotation, one undo step.
    #[wasm_bindgen(js_name = addHighlight)]
    pub fn add_highlight(page: u32, x: f64, y: f64, width: f64, height: f64, rgb: u32) -> Self {
        add(
            page,
            NewAnnotation::Highlight {
                rect: rect(x, y, width, height),
                color: unpack_rgb(rgb),
            },
        )
    }

    /// Underlines a box; see `addHighlight`.
    #[wasm_bindgen(js_name = addUnderline)]
    pub fn add_underline(page: u32, x: f64, y: f64, width: f64, height: f64, rgb: u32) -> Self {
        add(
            page,
            NewAnnotation::Underline {
                rect: rect(x, y, width, height),
                color: unpack_rgb(rgb),
            },
        )
    }

    /// Strikes a box out; see `addHighlight`.
    #[wasm_bindgen(js_name = addStrikeout)]
    pub fn add_strikeout(page: u32, x: f64, y: f64, width: f64, height: f64, rgb: u32) -> Self {
        add(
            page,
            NewAnnotation::Strikeout {
                rect: rect(x, y, width, height),
                color: unpack_rgb(rgb),
            },
        )
    }

    /// A rectangle outline (`/Square`).
    #[wasm_bindgen(js_name = addShape)]
    pub fn add_shape(page: u32, x: f64, y: f64, width: f64, height: f64, rgb: u32) -> Self {
        add(
            page,
            NewAnnotation::Shape {
                rect: rect(x, y, width, height),
                color: unpack_rgb(rgb),
            },
        )
    }

    /// One freehand stroke through `points` (`x0, y0, x1, y1, ...`).
    #[wasm_bindgen(js_name = addInk)]
    pub fn add_ink(page: u32, points: Vec<f64>, rgb: u32) -> Self {
        add(
            page,
            NewAnnotation::Ink {
                points: points_from_flat(&points),
                color: unpack_rgb(rgb),
            },
        )
    }

    /// A sticky note (`/Text`) saying `contents`.
    #[wasm_bindgen(js_name = addNote)]
    pub fn add_note(page: u32, x: f64, y: f64, width: f64, height: f64, contents: String) -> Self {
        add(
            page,
            NewAnnotation::TextNote {
                rect: rect(x, y, width, height),
                contents,
            },
        )
    }

    /// A visible text box (`/FreeText`). `EncodingGap` on apply when the
    /// text holds a character Helvetica cannot show.
    #[wasm_bindgen(js_name = addFreeText)]
    pub fn add_free_text(
        page: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        contents: String,
    ) -> Self {
        add(
            page,
            NewAnnotation::FreeText {
                rect: rect(x, y, width, height),
                contents,
            },
        )
    }

    /// An image stamp from PNG/JPEG bytes. `InvalidImage` on apply when
    /// they do not decode.
    #[wasm_bindgen(js_name = addStamp)]
    pub fn add_stamp(page: u32, image: Vec<u8>, x: f64, y: f64, width: f64, height: f64) -> Self {
        add(
            page,
            NewAnnotation::Stamp {
                image,
                rect: rect(x, y, width, height),
            },
        )
    }

    /// Moves annotation `id` by (`dx`, `dy`).
    #[wasm_bindgen(js_name = moveAnnotation)]
    pub fn move_annotation(id: u32, dx: f64, dy: f64) -> Self {
        change(id, AnnotationChange::Move { dx, dy })
    }

    /// Gives annotation `id` a new box (not ink, which has none).
    #[wasm_bindgen(js_name = resizeAnnotation)]
    pub fn resize_annotation(id: u32, x: f64, y: f64, width: f64, height: f64) -> Self {
        change(id, AnnotationChange::Resize(rect(x, y, width, height)))
    }

    /// Recolours annotation `id` (markup, shape and ink have a colour).
    #[wasm_bindgen(js_name = recolorAnnotation)]
    pub fn recolor_annotation(id: u32, rgb: u32) -> Self {
        change(id, AnnotationChange::Restyle(unpack_rgb(rgb)))
    }

    /// Retypes free-text box `id`. The same text records nothing.
    #[wasm_bindgen(js_name = retextAnnotation)]
    pub fn retext_annotation(id: u32, contents: String) -> Self {
        change(id, AnnotationChange::SetContents(contents))
    }

    /// Removes annotation `id`.
    #[wasm_bindgen(js_name = removeAnnotation)]
    pub fn remove_annotation(id: u32) -> Self {
        EditCommand(Edit::Annotation(AnnotationEdit::Remove { id }))
    }
}

#[wasm_bindgen]
impl Engine {
    /// The session's annotations in paint order, each with the position its
    /// page is at now. Changes with every edit, undo and redo (a page move
    /// moves the annotations on it). Read-only: answers however restricted
    /// the document is.
    pub fn annotations(&self, doc: u32) -> Result<Vec<AnnotationEntry>, JsValue> {
        self.document(doc)
            .map(|open| {
                open.model
                    .annotations()
                    .into_iter()
                    .map(Into::into)
                    .collect()
            })
            .map_err(js_error)
    }

    /// Every comment in page order: on each page the file's own (read-only,
    /// no `annotationId`) and then the session's notes. Read-only.
    pub fn comments(&self, doc: u32) -> Result<Vec<CommentEntry>, JsValue> {
        self.document(doc)
            .map(|open| open.model.comments().into_iter().map(Into::into).collect())
            .map_err(js_error)
    }

    /// Whether the document lets annotations be added, changed or removed —
    /// what an annotation `applyEdit` refuses with `EditNotPermitted`.
    #[wasm_bindgen(js_name = annotationEditingAllowed)]
    pub fn annotation_editing_allowed(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.annotation_editing_allowed())
            .map_err(js_error)
    }
}

/// How `contents` lays out in a free-text box of `widthPt` x `heightPt` —
/// the layout the box will have once added. Blank text is fine;
/// `EncodingGap` names a character Helvetica cannot show.
#[wasm_bindgen(js_name = freeTextLayout)]
pub fn free_text_layout(
    contents: &str,
    width_pt: f64,
    height_pt: f64,
) -> Result<FreeTextLines, JsValue> {
    crate::model::free_text_layout(contents, width_pt, height_pt)
        .map(Into::into)
        .map_err(js_error)
}

/// The box (`x, y, width, height`) the core gives an image stamp the app
/// places at (`anchorX`, `anchorY`): the image's proportions, its longest
/// side two inches, its top-left corner at the anchor. `InvalidImage` when
/// the bytes are no PNG/JPEG.
#[wasm_bindgen(js_name = stampPlacement)]
pub fn stamp_placement(image: &[u8], anchor_x: f64, anchor_y: f64) -> Result<Vec<f64>, JsValue> {
    crate::model::stamp_placement(image, (anchor_x, anchor_y))
        .map(rect_values)
        .map_err(js_error)
}
