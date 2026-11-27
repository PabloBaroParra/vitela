//! The values `Engine`'s page-content surface hands to JavaScript (B26
//! T-258).
//!
//! Geometry is flattened like every other box here: `x, y, width, height`
//! in PDF points, the page's unrotated space, bottom-left origin. Ids cross
//! as numbers: the core keeps them below 2^53 (a synthetic id is
//! `2^40 + index + (part << 24)`), so no `BigInt` is needed. `@vitela/core`
//! copies these into plain objects and frees them.

use pdf_document::{FontKind, PathPaint};
use pdf_edit::Alignment;
use wasm_bindgen::prelude::*;

use crate::convert::annotation::rect_values;
use crate::model::{
    ContentImageView, ContentPathView, ContentRunView, PageContentView, TextBlockView,
    TextLineView, TextRunStyleView,
};

/// One text run, as `Engine.readPageContent` lists it.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct ContentRunEntry {
    id: f64,
    page: u32,
    bbox: Vec<f64>,
    font_name: String,
    font_kind: &'static str,
    text: String,
    editable: bool,
}

impl From<ContentRunView> for ContentRunEntry {
    fn from(run: ContentRunView) -> Self {
        ContentRunEntry {
            id: run.id as f64,
            page: run.page,
            bbox: rect_values(run.bbox),
            font_name: run.resource_font_name,
            font_kind: font_kind_name(run.font_kind),
            text: run.text,
            editable: run.editable,
        }
    }
}

#[wasm_bindgen]
impl ContentRunEntry {
    /// Valid against the read it came from (see `contentBaseGeneration`).
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> f64 {
        self.id
    }

    /// The position it was asked by.
    #[wasm_bindgen(getter)]
    pub fn page(&self) -> u32 {
        self.page
    }

    #[wasm_bindgen(getter)]
    pub fn bbox(&self) -> Vec<f64> {
        self.bbox.clone()
    }

    /// The font's key in the page's `/Resources /Font` (`F1`); its
    /// `/BaseFont` is in `pageFontFamilies`.
    #[wasm_bindgen(getter, js_name = fontName)]
    pub fn font_name(&self) -> String {
        self.font_name.clone()
    }

    /// `standard14`, `embeddedSimple` or `embeddedComposite`.
    #[wasm_bindgen(getter, js_name = fontKind)]
    pub fn font_kind(&self) -> String {
        self.font_kind.to_string()
    }

    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// Whether its text decoded fully, so it could be retyped.
    #[wasm_bindgen(getter)]
    pub fn editable(&self) -> bool {
        self.editable
    }
}

/// One image a page paints.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct ContentImageEntry {
    id: f64,
    page: u32,
    bbox: Vec<f64>,
    inline: bool,
}

impl From<ContentImageView> for ContentImageEntry {
    fn from(image: ContentImageView) -> Self {
        ContentImageEntry {
            id: image.id as f64,
            page: image.page,
            bbox: rect_values(image.bbox),
            inline: image.inline,
        }
    }
}

#[wasm_bindgen]
impl ContentImageEntry {
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> f64 {
        self.id
    }

    #[wasm_bindgen(getter)]
    pub fn page(&self) -> u32 {
        self.page
    }

    #[wasm_bindgen(getter)]
    pub fn bbox(&self) -> Vec<f64> {
        self.bbox.clone()
    }

    /// Its samples sit in the content stream, not in an XObject.
    #[wasm_bindgen(getter)]
    pub fn inline(&self) -> bool {
        self.inline
    }
}

/// One vector path a page paints.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct ContentPathEntry {
    id: f64,
    page: u32,
    bbox: Vec<f64>,
    paint: &'static str,
}

impl From<ContentPathView> for ContentPathEntry {
    fn from(path: ContentPathView) -> Self {
        ContentPathEntry {
            id: path.id as f64,
            page: path.page,
            bbox: rect_values(path.bbox),
            paint: match path.paint {
                PathPaint::Stroke => "stroke",
                PathPaint::Fill => "fill",
                PathPaint::FillAndStroke => "fillAndStroke",
            },
        }
    }
}

#[wasm_bindgen]
impl ContentPathEntry {
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> f64 {
        self.id
    }

    #[wasm_bindgen(getter)]
    pub fn page(&self) -> u32 {
        self.page
    }

    /// The geometry's box, without the stroke's width: a hairline rule's is
    /// zero high.
    #[wasm_bindgen(getter)]
    pub fn bbox(&self) -> Vec<f64> {
        self.bbox.clone()
    }

    /// `stroke`, `fill` or `fillAndStroke`.
    #[wasm_bindgen(getter)]
    pub fn paint(&self) -> String {
        self.paint.to_string()
    }
}

/// Everything one page paints, as `Engine.readPageContent` reads it.
#[wasm_bindgen]
pub struct PageContentEntry {
    runs: Vec<ContentRunEntry>,
    images: Vec<ContentImageEntry>,
    paths: Vec<ContentPathEntry>,
}

impl From<PageContentView> for PageContentEntry {
    fn from(content: PageContentView) -> Self {
        PageContentEntry {
            runs: content.runs.into_iter().map(Into::into).collect(),
            images: content.images.into_iter().map(Into::into).collect(),
            paths: content.paths.into_iter().map(Into::into).collect(),
        }
    }
}

#[wasm_bindgen]
impl PageContentEntry {
    /// Moves the runs out; a second call returns none.
    #[wasm_bindgen(js_name = takeRuns)]
    pub fn take_runs(&mut self) -> Vec<ContentRunEntry> {
        std::mem::take(&mut self.runs)
    }

    /// Moves the images out; a second call returns none.
    #[wasm_bindgen(js_name = takeImages)]
    pub fn take_images(&mut self) -> Vec<ContentImageEntry> {
        std::mem::take(&mut self.images)
    }

    /// Moves the paths out; a second call returns none.
    #[wasm_bindgen(js_name = takePaths)]
    pub fn take_paths(&mut self) -> Vec<ContentPathEntry> {
        std::mem::take(&mut self.paths)
    }
}

/// One line of a [`TextBlockEntry`].
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct TextLineEntry {
    run_ids: Vec<f64>,
    bbox: Vec<f64>,
    baseline: f64,
    text: String,
}

impl From<TextLineView> for TextLineEntry {
    fn from(line: TextLineView) -> Self {
        TextLineEntry {
            run_ids: ids(&line.run_ids),
            bbox: rect_values(line.bbox),
            baseline: line.baseline,
            text: line.text,
        }
    }
}

#[wasm_bindgen]
impl TextLineEntry {
    /// The member runs' ids, left to right.
    #[wasm_bindgen(getter, js_name = runIds)]
    pub fn run_ids(&self) -> Vec<f64> {
        self.run_ids.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn bbox(&self) -> Vec<f64> {
        self.bbox.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn baseline(&self) -> f64 {
        self.baseline
    }

    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.text.clone()
    }
}

/// A text block: one line, or a paragraph of stacked lines.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct TextBlockEntry {
    run_ids: Vec<f64>,
    bbox: Vec<f64>,
    text: String,
    lines: Vec<TextLineEntry>,
    leading: Option<f64>,
    alignment: &'static str,
}

impl From<TextBlockView> for TextBlockEntry {
    fn from(block: TextBlockView) -> Self {
        TextBlockEntry {
            run_ids: ids(&block.run_ids),
            bbox: rect_values(block.bbox),
            text: block.text,
            lines: block.lines.into_iter().map(Into::into).collect(),
            leading: block.leading,
            alignment: match block.alignment {
                Alignment::Left => "left",
                Alignment::Center => "center",
                Alignment::Right => "right",
            },
        }
    }
}

#[wasm_bindgen]
impl TextBlockEntry {
    /// The member runs' ids, line by line from the top.
    #[wasm_bindgen(getter, js_name = runIds)]
    pub fn run_ids(&self) -> Vec<f64> {
        self.run_ids.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn bbox(&self) -> Vec<f64> {
        self.bbox.clone()
    }

    /// The lines' text joined with `'\n'`.
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn lines(&self) -> Vec<TextLineEntry> {
        self.lines.clone()
    }

    /// The mean baseline-to-baseline distance; absent for one line.
    #[wasm_bindgen(getter)]
    pub fn leading(&self) -> Option<f64> {
        self.leading
    }

    /// `left`, `center` or `right`.
    #[wasm_bindgen(getter)]
    pub fn alignment(&self) -> String {
        self.alignment.to_string()
    }
}

/// How a run looks on its page, as `Engine.textRunStyle` reads it.
#[wasm_bindgen]
pub struct TextRunStyleEntry {
    style: TextRunStyleView,
}

impl From<TextRunStyleView> for TextRunStyleEntry {
    fn from(style: TextRunStyleView) -> Self {
        TextRunStyleEntry { style }
    }
}

#[wasm_bindgen]
impl TextRunStyleEntry {
    /// The font's `/BaseFont`, raw (`ABCDEF+Vera-Bold`); empty when it names
    /// none.
    #[wasm_bindgen(getter, js_name = baseFont)]
    pub fn base_font(&self) -> String {
        self.style.base_font.clone()
    }

    /// The size a reader sees, in points.
    #[wasm_bindgen(getter, js_name = fontSize)]
    pub fn font_size(&self) -> f64 {
        self.style.font_size
    }

    /// The fill colour as `r, g, b`, each `0..=1`.
    #[wasm_bindgen(getter, js_name = fillRgb)]
    pub fn fill_rgb(&self) -> Vec<f64> {
        self.style.fill_rgb.to_vec()
    }

    /// `Tc` in page-space points.
    #[wasm_bindgen(getter, js_name = charSpacing)]
    pub fn char_spacing(&self) -> f64 {
        self.style.char_spacing
    }

    /// `Tz` as a factor: 1 is unscaled.
    #[wasm_bindgen(getter, js_name = horizontalScale)]
    pub fn horizontal_scale(&self) -> f64 {
        self.style.horizontal_scale
    }

    #[wasm_bindgen(getter)]
    pub fn bold(&self) -> bool {
        self.style.bold
    }

    #[wasm_bindgen(getter)]
    pub fn italic(&self) -> bool {
        self.style.italic
    }

    /// `Tr`: 0 fills, 2 fills and strokes (a faked bold).
    #[wasm_bindgen(getter, js_name = renderMode)]
    pub fn render_mode(&self) -> u8 {
        self.style.render_mode
    }
}

fn ids(ids: &[u64]) -> Vec<f64> {
    ids.iter().map(|&id| id as f64).collect()
}

fn font_kind_name(kind: FontKind) -> &'static str {
    match kind {
        FontKind::Standard14 => "standard14",
        FontKind::EmbeddedSimple => "embeddedSimple",
        FontKind::EmbeddedComposite => "embeddedComposite",
        // `FontKind` is non-exhaustive; a kind this boundary does not know
        // yet is one it cannot promise to retype in its own font.
        _ => "embeddedComposite",
    }
}
