//! Where page-space geometry lands on a page as it is drawn — the
//! `pdf_render::selection` transform, carried across for shells that cannot
//! link `pdf-render` directly.
//!
//! A page carrying `/Rotate` is rasterized turned, and `page_dimensions`
//! reports its size with the turn already applied. Everything *on* the page —
//! text runs, search hits, selection rects, annotations — stays in the page's
//! unrotated space, because a `/Rotate` is a viewing instruction and moves
//! nothing in the file. A shell that places overlays with a bare y-flip puts
//! every one of them in the wrong place the moment a page is turned.
//!
//! The GTK shell calls `pdf_render::{place_rect, place_point, point_to_pdf}`
//! in-process. Windows keeps geometry in Rust (`apps/windows/CONTRACT.md`),
//! so the same three functions cross here instead of a second copy of the
//! per-turn arithmetic drifting in C#. They are free functions, not methods
//! on `DocumentHandle`: pure geometry over values the shell already holds.

use pdf_document::Rotation;
use pdf_render::{PagePlacement, PageRotation, TextRect};

use crate::types::{FfiPoint, FfiRect};

/// The clockwise quarter-turn a page is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiPageRotation {
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

impl From<Rotation> for FfiPageRotation {
    fn from(rotation: Rotation) -> Self {
        match rotation {
            Rotation::None => FfiPageRotation::None,
            Rotation::Clockwise90 => FfiPageRotation::Clockwise90,
            Rotation::Clockwise180 => FfiPageRotation::Clockwise180,
            Rotation::Clockwise270 => FfiPageRotation::Clockwise270,
        }
    }
}

impl From<FfiPageRotation> for PageRotation {
    fn from(rotation: FfiPageRotation) -> Self {
        match rotation {
            FfiPageRotation::None => PageRotation::None,
            FfiPageRotation::Clockwise90 => PageRotation::Clockwise90,
            FfiPageRotation::Clockwise180 => PageRotation::Clockwise180,
            FfiPageRotation::Clockwise270 => PageRotation::Clockwise270,
        }
    }
}

/// One page as drawn: its size **with the turn applied** — exactly what
/// `page_dimensions` reports — the turn itself, and the display units per
/// point it is shown at.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiPagePlacement {
    pub width_pt: f64,
    pub height_pt: f64,
    pub rotation: FfiPageRotation,
    pub scale: f64,
}

impl From<FfiPagePlacement> for PagePlacement {
    fn from(page: FfiPagePlacement) -> Self {
        PagePlacement {
            width_pt: page.width_pt as f32,
            height_pt: page.height_pt as f32,
            rotation: page.rotation.into(),
            scale: page.scale,
        }
    }
}

/// A rect in a shell's drawing space: top-left origin, y growing downwards,
/// in display units.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiPlacedRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// Places a page-space rect (PDF points, bottom-left origin, unrotated) on
/// the page as drawn. On a quarter-turned page the placed width and height
/// are the rect's height and width.
#[uniffi::export]
pub fn place_rect(rect: FfiRect, page: FfiPagePlacement) -> FfiPlacedRect {
    let placed = pdf_render::place_rect(
        TextRect {
            x_pt: rect.x as f32,
            y_pt: rect.y as f32,
            width_pt: rect.width as f32,
            height_pt: rect.height as f32,
        },
        page.into(),
    );
    FfiPlacedRect {
        left: placed.left,
        top: placed.top,
        width: placed.width,
        height: placed.height,
    }
}

/// [`place_rect`] for a bare point — an ink vertex, a resize handle, or the
/// corner an upright frame is anchored at.
#[uniffi::export]
pub fn place_point(point: FfiPoint, page: FfiPagePlacement) -> FfiPoint {
    let (x, y) = pdf_render::place_point((point.x, point.y), page.into());
    FfiPoint { x, y }
}

/// The inverse of [`place_point`]: a pointer position on the drawn page,
/// back into page space for hit-testing and edits.
#[uniffi::export]
pub fn point_to_pdf(point: FfiPoint, page: FfiPagePlacement) -> FfiPoint {
    let (x, y) = pdf_render::point_to_pdf(point.x, point.y, page.into());
    FfiPoint {
        x: f64::from(x),
        y: f64::from(y),
    }
}
