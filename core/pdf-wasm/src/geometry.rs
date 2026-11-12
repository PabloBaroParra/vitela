//! The wasm-bindgen exports of [`crate::placement`]: free functions, since
//! they need no document — only the page as `Engine::pageDimensions`
//! reported it.

use wasm_bindgen::prelude::*;

use crate::placement::{drawn_point_to_pdf, place_flat_rects, DrawnPage};

fn page(width_pt: f32, height_pt: f32, rotation_degrees: u32) -> DrawnPage {
    DrawnPage {
        width_pt,
        height_pt,
        rotation_degrees,
    }
}

/// Places flattened PDF-space rects on the page as drawn: `left, top, width,
/// height` per rect, in drawn points with a top-left origin. `widthPt`,
/// `heightPt` and `rotationDegrees` are the page's `pageDimensions`.
#[wasm_bindgen(js_name = placeRects)]
pub fn place_rects(
    rects: Vec<f32>,
    width_pt: f32,
    height_pt: f32,
    rotation_degrees: u32,
) -> Vec<f32> {
    place_flat_rects(&rects, page(width_pt, height_pt, rotation_degrees))
}

/// A point on the drawn page (drawn points, top-left origin) as `[x, y]` in
/// the PDF space `PageCharacters` hit-tests in.
#[wasm_bindgen(js_name = pointToPdf)]
pub fn point_to_pdf(
    x: f64,
    y: f64,
    width_pt: f32,
    height_pt: f32,
    rotation_degrees: u32,
) -> Vec<f32> {
    let (pdf_x, pdf_y) = drawn_point_to_pdf(x, y, page(width_pt, height_pt, rotation_degrees));
    vec![pdf_x, pdf_y]
}
