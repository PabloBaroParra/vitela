//! FFI-facing data types (T-040): UniFFI `Record`/`Enum` shapes for the
//! command surface, plus their conversions to/from the real
//! `pdf_document`/`pdf_render`/`pdf_save` types. Kept as thin, explicit
//! mapping structs/enums rather than re-exporting the core crates' own types
//! directly — those crates are not UniFFI-aware (by design, per each
//! crate's "never depends on uniffi" boundary), and a stable FFI shape must
//! not break every time an internal core type gains a field.

use pdf_document::{Orientation, PageSize, Rect};

/// Mirrors `pdf_document::PageSize`.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum FfiPageSize {
    A4,
    Letter,
    Custom { width_pt: f64, height_pt: f64 },
}

impl From<FfiPageSize> for PageSize {
    fn from(size: FfiPageSize) -> Self {
        match size {
            FfiPageSize::A4 => PageSize::A4,
            FfiPageSize::Letter => PageSize::Letter,
            FfiPageSize::Custom {
                width_pt,
                height_pt,
            } => PageSize::Custom {
                width_pt,
                height_pt,
            },
        }
    }
}

/// Mirrors `pdf_document::Orientation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiOrientation {
    Portrait,
    Landscape,
}

impl From<FfiOrientation> for Orientation {
    fn from(orientation: FfiOrientation) -> Self {
        match orientation {
            FfiOrientation::Portrait => Orientation::Portrait,
            FfiOrientation::Landscape => Orientation::Landscape,
        }
    }
}

/// Mirrors `pdf_document::Rect` (page-space rectangle, points, origin
/// bottom-left) — distinct from `pdf_render::Rect` (a render-time clip
/// region in the same units but a different field shape); this FFI type
/// only ever crosses at the `pdf_document`/`pdf_annotate` annotation-rect
/// meaning.
/// One page's layout size in PDF points (`/Rotate`-aware — 90/270 swap the
/// axes), read from the same bytes `render_page` draws so viewers can size
/// placeholders that match the rendered output.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiPageDimensions {
    pub width_pt: f64,
    pub height_pt: f64,
}

impl From<pdf_manip::PageDimensions> for FfiPageDimensions {
    fn from(dimensions: pdf_manip::PageDimensions) -> Self {
        FfiPageDimensions {
            width_pt: dimensions.width_pt,
            height_pt: dimensions.height_pt,
        }
    }
}

/// A page-space rectangle in PDF points with a bottom-left origin, suitable
/// for mapping directly onto a rendered page.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiTextRect {
    pub x_pt: f64,
    pub y_pt: f64,
    pub width_pt: f64,
    pub height_pt: f64,
}

impl From<pdf_render::TextRect> for FfiTextRect {
    fn from(rect: pdf_render::TextRect) -> Self {
        Self {
            x_pt: f64::from(rect.x_pt),
            y_pt: f64::from(rect.y_pt),
            width_pt: f64::from(rect.width_pt),
            height_pt: f64::from(rect.height_pt),
        }
    }
}

/// Extracted text from one font run, with one PDF-space rectangle per Unicode
/// scalar in `text`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiTextRun {
    pub text: String,
    pub character_bounds: Vec<FfiTextRect>,
}

impl From<pdf_render::TextRun> for FfiTextRun {
    fn from(run: pdf_render::TextRun) -> Self {
        Self {
            text: run.text,
            character_bounds: run.character_bounds.into_iter().map(Into::into).collect(),
        }
    }
}

/// One exact-text match with its 0-indexed page and per-character PDF-space
/// rectangles. Empty queries return no matches.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiSearchResult {
    pub page_index: u32,
    pub text: String,
    pub character_bounds: Vec<FfiTextRect>,
}

impl From<pdf_render::TextMatch> for FfiSearchResult {
    fn from(found: pdf_render::TextMatch) -> Self {
        Self {
            page_index: found.page_index,
            text: found.text,
            character_bounds: found.character_bounds.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<FfiRect> for Rect {
    fn from(rect: FfiRect) -> Self {
        Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

impl From<Rect> for FfiRect {
    fn from(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

/// Mirrors `pdf_document::Color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FfiColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<FfiColor> for pdf_document::Color {
    fn from(color: FfiColor) -> Self {
        pdf_document::Color {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

impl From<pdf_document::Color> for FfiColor {
    fn from(color: pdf_document::Color) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

/// A single point of an ink/freehand stroke (mirrors the `(f64, f64)` tuple
/// `pdf_document::AnnotationKind::Ink` stores — UniFFI records need named
/// fields, tuples don't cross the boundary directly).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiPoint {
    pub x: f64,
    pub y: f64,
}

/// A persisted annotation exposed to shells for hit testing and local preview.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiAnnotation {
    pub id: u64,
    pub page: u32,
    pub kind: FfiAnnotationKind,
}

/// The editable annotation shapes supported by the cross-platform shell API.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum FfiAnnotationKind {
    Highlight {
        rect: FfiRect,
        color: FfiColor,
    },
    Underline {
        rect: FfiRect,
        color: FfiColor,
    },
    Strikeout {
        rect: FfiRect,
        color: FfiColor,
    },
    Ink {
        points: Vec<FfiPoint>,
        color: FfiColor,
    },
    Shape {
        rect: FfiRect,
        color: FfiColor,
    },
    TextNote {
        rect: FfiRect,
        contents: String,
    },
    Stamp {
        rect: FfiRect,
    },
}

/// Mirrors `pdf_render::RenderOptions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Record)]
pub struct FfiRenderOptions {
    pub invert_content_colors: bool,
}

/// A bounded page tile in the output pixel coordinate space at the requested DPI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FfiRenderTile {
    pub left_px: u32,
    pub top_px: u32,
    pub width_px: u32,
    pub height_px: u32,
}

impl From<FfiRenderTile> for pdf_render::Tile {
    fn from(tile: FfiRenderTile) -> Self {
        Self {
            left: tile.left_px,
            top: tile.top_px,
            width: tile.width_px,
            height: tile.height_px,
        }
    }
}

impl From<FfiRenderOptions> for pdf_render::RenderOptions {
    fn from(options: FfiRenderOptions) -> Self {
        pdf_render::RenderOptions {
            invert_content_colors: options.invert_content_colors,
        }
    }
}

/// Mirrors `pdf_save::SaveIntent`: the caller's declared intent for how
/// `save`/`save_to_bytes`/`save_to_path` should treat an existing
/// `SecurityContext` (spec "Encrypted Document Save Behavior"). Choosing
/// `StripProtection` from a shell UI is exactly the "explicit, user-consented
/// removal of protection" the spec requires — see `document::save_to_bytes`,
/// which records the audit-log consent event when this variant is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum FfiSaveIntent {
    #[default]
    Default,
    StripProtection,
}

impl From<FfiSaveIntent> for pdf_save::SaveIntent {
    fn from(intent: FfiSaveIntent) -> Self {
        match intent {
            FfiSaveIntent::Default => pdf_save::SaveIntent::Default,
            FfiSaveIntent::StripProtection => pdf_save::SaveIntent::StripProtection,
        }
    }
}

/// The FFI-facing shape of `pdf_document::Command` (T-040's `apply_edit`
/// surface). One variant per real `Command`/annotation-kind combination
/// this workspace supports as of Batch 7 — `move`/`resize`/`restyle` are
/// deliberately absent (documented Batch 5 gap: not yet `EditLog` commands,
/// see `pdf-annotate::ops` module docs).
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum FfiEditCommand {
    RotatePage {
        page: u32,
        delta_degrees: i32,
    },
    InsertBlankPage {
        index: u32,
        size: FfiPageSize,
        orientation: FfiOrientation,
    },
    RemovePage {
        index: u32,
    },
    AddHighlight {
        page: u32,
        rect: FfiRect,
        color: FfiColor,
    },
    AddUnderline {
        page: u32,
        rect: FfiRect,
        color: FfiColor,
    },
    AddStrikeout {
        page: u32,
        rect: FfiRect,
        color: FfiColor,
    },
    AddShape {
        page: u32,
        rect: FfiRect,
        color: FfiColor,
    },
    AddInk {
        page: u32,
        points: Vec<FfiPoint>,
        color: FfiColor,
    },
    AddTextNote {
        page: u32,
        rect: FfiRect,
        contents: String,
    },
    RemoveAnnotation {
        annotation_id: u64,
    },
    MoveAnnotation {
        annotation_id: u64,
        dx: f64,
        dy: f64,
    },
    ResizeAnnotation {
        annotation_id: u64,
        rect: FfiRect,
    },
    RestyleAnnotation {
        annotation_id: u64,
        color: FfiColor,
    },
}
