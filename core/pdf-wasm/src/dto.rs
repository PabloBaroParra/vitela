//! The values `Engine` hands to JavaScript.
//!
//! Rects are flattened (see [`crate::convert`]): every `bounds`/`rects`
//! buffer holds `x, y, width, height` per rect, in PDF points with a
//! bottom-left origin — the same space `pdf-ffi`'s `FfiTextRect` uses.

use wasm_bindgen::prelude::*;

use crate::convert::flatten_rects;
use crate::model::PageSize;

/// One page's layout size in PDF points and the clockwise turn (0, 90, 180
/// or 270) it is drawn at.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct PageDimensions {
    #[wasm_bindgen(readonly, js_name = widthPt)]
    pub width_pt: f64,
    #[wasm_bindgen(readonly, js_name = heightPt)]
    pub height_pt: f64,
    #[wasm_bindgen(readonly, js_name = rotationDegrees)]
    pub rotation_degrees: u32,
}

impl From<PageSize> for PageDimensions {
    fn from(size: PageSize) -> Self {
        PageDimensions {
            width_pt: size.width_pt,
            height_pt: size.height_pt,
            rotation_degrees: size.rotation_degrees,
        }
    }
}

/// A rendered page: tightly packed RGBA, `width * height * 4` bytes.
#[wasm_bindgen]
pub struct RenderedPage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl RenderedPage {
    pub(crate) fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        RenderedPage {
            width,
            height,
            rgba,
        }
    }
}

#[wasm_bindgen]
impl RenderedPage {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Moves the pixels out (one copy into a JS `Uint8Array`); a second call
    /// returns an empty array.
    #[wasm_bindgen(js_name = takeRgba)]
    pub fn take_rgba(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.rgba)
    }
}

/// One run of text with one rect per Unicode scalar of `text`.
#[wasm_bindgen]
pub struct TextRun {
    text: String,
    font_name: String,
    font_size_pt: f32,
    bounds: Vec<f32>,
}

impl From<pdf_render::TextRun> for TextRun {
    fn from(run: pdf_render::TextRun) -> Self {
        TextRun {
            bounds: flatten_rects(&run.character_bounds),
            text: run.text,
            font_name: run.font_name,
            font_size_pt: run.font_size_pt,
        }
    }
}

#[wasm_bindgen]
impl TextRun {
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    #[wasm_bindgen(getter, js_name = fontName)]
    pub fn font_name(&self) -> String {
        self.font_name.clone()
    }

    #[wasm_bindgen(getter, js_name = fontSizePt)]
    pub fn font_size_pt(&self) -> f32 {
        self.font_size_pt
    }

    /// One flattened rect per character of `text`.
    #[wasm_bindgen(getter)]
    pub fn bounds(&self) -> Vec<f32> {
        self.bounds.clone()
    }
}

/// One exact match of a search: the page it is on and one flattened rect per
/// character of the matched text.
#[wasm_bindgen]
pub struct SearchMatch {
    page_index: u32,
    text: String,
    bounds: Vec<f32>,
}

impl From<pdf_render::TextMatch> for SearchMatch {
    fn from(found: pdf_render::TextMatch) -> Self {
        SearchMatch {
            page_index: found.page_index,
            bounds: flatten_rects(&found.character_bounds),
            text: found.text,
        }
    }
}

#[wasm_bindgen]
impl SearchMatch {
    #[wasm_bindgen(getter, js_name = pageIndex)]
    pub fn page_index(&self) -> u32 {
        self.page_index
    }

    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn bounds(&self) -> Vec<f32> {
        self.bounds.clone()
    }
}

/// One page's characters, flattened for repeated caret and selection
/// queries — built once per page and held for the life of a drag-select,
/// like `pdf-ffi`'s `FfiPageCharacters`. Points are PDF points with a
/// bottom-left origin; carets index characters (0..=len).
#[wasm_bindgen]
pub struct PageCharacters {
    inner: pdf_render::PageCharacters,
}

impl PageCharacters {
    pub(crate) fn new(inner: pdf_render::PageCharacters) -> Self {
        PageCharacters { inner }
    }
}

#[wasm_bindgen]
impl PageCharacters {
    /// Character count on the page.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> u32 {
        self.inner.len() as u32
    }

    /// The caret nearest a point, or `undefined` on a page with no
    /// positioned text.
    #[wasm_bindgen(js_name = caretAt)]
    pub fn caret_at(&self, x_pt: f32, y_pt: f32) -> Option<u32> {
        self.inner.caret_at(x_pt, y_pt).map(|caret| caret as u32)
    }

    /// `[start, end]` carets around the word under a point, or `undefined`
    /// on whitespace or a page with no positioned text.
    #[wasm_bindgen(js_name = wordAt)]
    pub fn word_at(&self, x_pt: f32, y_pt: f32) -> Option<Vec<u32>> {
        self.inner
            .word_at(x_pt, y_pt)
            .map(|word| vec![word.start as u32, word.end as u32])
    }

    /// The text between two carets, in either order.
    #[wasm_bindgen(js_name = textIn)]
    pub fn text_in(&self, anchor: u32, focus: u32) -> String {
        self.inner
            .text_in(pdf_render::caret_range(anchor as usize, focus as usize))
    }

    /// The flattened rects to paint between two carets: one per visual line.
    #[wasm_bindgen(js_name = rectsIn)]
    pub fn rects_in(&self, anchor: u32, focus: u32) -> Vec<f32> {
        flatten_rects(
            &self
                .inner
                .rects_in(pdf_render::caret_range(anchor as usize, focus as usize)),
        )
    }
}
