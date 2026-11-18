//! The values `Engine`'s annotation surface hands to JavaScript (B26 T-236).
//!
//! Flat on purpose: one class per value, every field present for every kind
//! (empty when the kind has none), rather than one class per kind.
//! wasm-bindgen has no tagged unions, and it mistypes an `Option` in the
//! `.d.ts` (T-227), so `@vitela/core` decodes these by `kind` into its own
//! typed union and never trusts an absent field. Geometry is in the page's
//! unrotated PDF space, bottom-left origin (see
//! [`crate::convert::annotation`]).

use pdf_annotate::FreeTextLayout;
use pdf_save::Comment;
use wasm_bindgen::prelude::*;

use crate::convert::annotation::{flatten_points, pack_rgb, rect_values};
use crate::model::{AnnotationShape, AnnotationView};

/// One annotation of the session, as `Engine.annotations` lists it.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct AnnotationEntry {
    id: u32,
    page: u32,
    kind: &'static str,
    rect: Vec<f64>,
    points: Vec<f64>,
    color: Option<u32>,
    contents: String,
    layout: Option<FreeTextLines>,
}

impl From<AnnotationView> for AnnotationEntry {
    fn from(view: AnnotationView) -> Self {
        let mut entry = AnnotationEntry {
            id: view.id,
            page: view.page,
            kind: "",
            rect: Vec::new(),
            points: Vec::new(),
            color: None,
            contents: String::new(),
            layout: None,
        };
        match view.shape {
            AnnotationShape::Highlight { rect, color } => {
                entry.kind = "highlight";
                entry.rect = rect_values(rect);
                entry.color = Some(pack_rgb(color));
            }
            AnnotationShape::Underline { rect, color } => {
                entry.kind = "underline";
                entry.rect = rect_values(rect);
                entry.color = Some(pack_rgb(color));
            }
            AnnotationShape::Strikeout { rect, color } => {
                entry.kind = "strikeout";
                entry.rect = rect_values(rect);
                entry.color = Some(pack_rgb(color));
            }
            AnnotationShape::Shape { rect, color } => {
                entry.kind = "shape";
                entry.rect = rect_values(rect);
                entry.color = Some(pack_rgb(color));
            }
            AnnotationShape::Ink { points, color } => {
                entry.kind = "ink";
                entry.points = flatten_points(&points);
                entry.color = Some(pack_rgb(color));
            }
            AnnotationShape::Note { rect, contents } => {
                entry.kind = "note";
                entry.rect = rect_values(rect);
                entry.contents = contents;
            }
            AnnotationShape::Stamp { rect } => {
                entry.kind = "stamp";
                entry.rect = rect_values(rect);
            }
            AnnotationShape::FreeText {
                rect,
                contents,
                layout,
            } => {
                entry.kind = "freeText";
                entry.rect = rect_values(rect);
                entry.contents = contents;
                entry.layout = Some(layout.into());
            }
        }
        entry
    }
}

#[wasm_bindgen]
impl AnnotationEntry {
    /// What `EditCommand`'s change and removal constructors name it by.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Position of its page now.
    #[wasm_bindgen(getter)]
    pub fn page(&self) -> u32 {
        self.page
    }

    /// `highlight`, `underline`, `strikeout`, `shape`, `ink`, `note`,
    /// `freeText` or `stamp`.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.to_string()
    }

    /// `x, y, width, height`; empty for ink.
    #[wasm_bindgen(getter)]
    pub fn rect(&self) -> Vec<f64> {
        self.rect.clone()
    }

    /// `x0, y0, x1, y1, ...` of an ink stroke; empty for every other kind.
    #[wasm_bindgen(getter)]
    pub fn points(&self) -> Vec<f64> {
        self.points.clone()
    }

    /// `0xRRGGBB` for the kinds that have a colour (markup, shape, ink).
    #[wasm_bindgen(getter)]
    pub fn color(&self) -> Option<u32> {
        self.color
    }

    /// The text of a note or a free-text box; empty for every other kind.
    #[wasm_bindgen(getter)]
    pub fn contents(&self) -> String {
        self.contents.clone()
    }

    /// How a free-text box's text lays out in its rect — the layout its
    /// saved appearance will have. Only for `freeText`.
    #[wasm_bindgen(getter)]
    pub fn layout(&self) -> Option<FreeTextLines> {
        self.layout.clone()
    }
}

/// A free-text box's text laid out in its rect (`Engine.freeTextLayout`, and
/// a stored box's `AnnotationEntry.layout`). Each line is placed in the
/// box's own frame: `x` from its left edge, `baseline` down from its top
/// edge, before the page's turn.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct FreeTextLines {
    font_size_pt: f64,
    texts: Vec<String>,
    xs: Vec<f64>,
    baselines: Vec<f64>,
    overflow: bool,
}

impl From<FreeTextLayout> for FreeTextLines {
    fn from(layout: FreeTextLayout) -> Self {
        let mut lines = FreeTextLines {
            font_size_pt: layout.font_size_pt,
            texts: Vec::with_capacity(layout.lines.len()),
            xs: Vec::with_capacity(layout.lines.len()),
            baselines: Vec::with_capacity(layout.lines.len()),
            overflow: layout.overflow,
        };
        for line in layout.lines {
            lines.texts.push(line.text);
            lines.xs.push(line.x_pt);
            lines.baselines.push(line.baseline_from_top_pt);
        }
        lines
    }
}

#[wasm_bindgen]
impl FreeTextLines {
    #[wasm_bindgen(getter, js_name = fontSizePt)]
    pub fn font_size_pt(&self) -> f64 {
        self.font_size_pt
    }

    /// Each line's text, top to bottom.
    #[wasm_bindgen(getter)]
    pub fn texts(&self) -> Vec<String> {
        self.texts.clone()
    }

    /// Each line's left edge, from the box's left edge.
    #[wasm_bindgen(getter)]
    pub fn xs(&self) -> Vec<f64> {
        self.xs.clone()
    }

    /// Each line's baseline, down from the box's top edge.
    #[wasm_bindgen(getter)]
    pub fn baselines(&self) -> Vec<f64> {
        self.baselines.clone()
    }

    /// Whether the lines run past the box's height. They are all there; the
    /// painter clips.
    #[wasm_bindgen(getter)]
    pub fn overflow(&self) -> bool {
        self.overflow
    }
}

/// One comment, as `Engine.comments` lists it.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct CommentEntry {
    comment: Comment,
}

impl From<Comment> for CommentEntry {
    fn from(comment: Comment) -> Self {
        CommentEntry { comment }
    }
}

#[wasm_bindgen]
impl CommentEntry {
    /// Position of its page now.
    #[wasm_bindgen(getter)]
    pub fn page(&self) -> u32 {
        // Positions crossed in as `u32`, so they fit going out.
        self.comment.page as u32
    }

    /// `x, y, width, height`.
    #[wasm_bindgen(getter)]
    pub fn rect(&self) -> Vec<f64> {
        rect_values(self.comment.rect)
    }

    #[wasm_bindgen(getter)]
    pub fn contents(&self) -> String {
        self.comment.contents.clone()
    }

    /// The file's `/T`; absent for a session note and a file comment without
    /// one.
    #[wasm_bindgen(getter)]
    pub fn author(&self) -> Option<String> {
        self.comment.author.clone()
    }

    /// The file's own date string, unparsed; absent for a session note.
    #[wasm_bindgen(getter)]
    pub fn date(&self) -> Option<String> {
        self.comment.date.clone()
    }

    /// The session note's annotation id; absent for a comment the file
    /// carries, which is read-only.
    #[wasm_bindgen(getter, js_name = annotationId)]
    pub fn annotation_id(&self) -> Option<u32> {
        self.comment
            .annotation
            .and_then(|id| u32::try_from(id.0).ok())
    }
}
