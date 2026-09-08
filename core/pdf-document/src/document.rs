//! Document/Page data model — the center of the hexagon (T-011).
//!
//! Pure data: `Document { pages, annotations, pending_edits, audit_log, security }`.
//! No pdfium/lopdf types leak in here — see crate-level docs.

use crate::annotation::AnnotationSet;
use crate::audit_log::AuditLog;
use crate::edit_log::EditLog;
use crate::form::FormFieldSet;
use crate::security::SecurityContext;

/// Identifies a page within a `Document` — a stable identity assigned at
/// population/insertion time, NOT the page's current 0-based position
/// (pages keep their id across removals and future reorders). A newtype
/// mirroring [`crate::AnnotationId`], so an id and a positional index can
/// never be swapped silently at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageId(pub u32);

/// Identifies one imported PDF within the current editing session.
///
/// The corresponding bytes or parsed PDF belong to a backing store outside
/// this pure model; pages retain only this stable key and their source index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImportedDocumentId(pub u64);

/// Describes where a page's PDF content comes from in the current session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOrigin {
    /// A zero-based page in the immutable PDF opened for this session.
    Base { page_index: u32 },
    /// A page created without source PDF content.
    Blank,
    /// A zero-based page in a separately registered imported PDF.
    Imported {
        source: ImportedDocumentId,
        page_index: u32,
    },
}

/// A page's paper size. `Custom` carries explicit point dimensions for
/// arbitrary sizes; `A4`/`Letter` are the two MVP presets for
/// `create_blank_document` (spec "Create Blank Document").
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    A4,
    Letter,
    Custom { width_pt: f64, height_pt: f64 },
}

impl PageSize {
    /// Width/height in PDF points (1/72 inch), portrait orientation.
    pub fn dimensions_pt(&self) -> (f64, f64) {
        match self {
            PageSize::A4 => (595.0, 842.0),
            PageSize::Letter => (612.0, 792.0),
            PageSize::Custom {
                width_pt,
                height_pt,
            } => (*width_pt, *height_pt),
        }
    }
}

/// Page orientation, independent of the `/Rotate` viewer-rotation entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Viewer-rotation state for a page (the PDF `/Rotate` entry), always a
/// multiple of 90 degrees clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

impl Rotation {
    /// Returns the rotation obtained by rotating `self` clockwise by
    /// `delta_degrees` (may be negative, and need not be a multiple of 90 —
    /// it is normalized modulo 360 and rounded down to the nearest quarter
    /// turn boundary it lands on).
    pub fn rotated_by(self, delta_degrees: i32) -> Self {
        let current = self.degrees();
        let normalized = (current + delta_degrees).rem_euclid(360);
        Self::from_degrees(normalized)
    }

    fn degrees(self) -> i32 {
        match self {
            Rotation::None => 0,
            Rotation::Clockwise90 => 90,
            Rotation::Clockwise180 => 180,
            Rotation::Clockwise270 => 270,
        }
    }

    fn from_degrees(degrees: i32) -> Self {
        match degrees.rem_euclid(360) {
            90 => Rotation::Clockwise90,
            180 => Rotation::Clockwise180,
            270 => Rotation::Clockwise270,
            _ => Rotation::None,
        }
    }
}

/// A single page within a `Document`.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub id: PageId,
    pub origin: PageOrigin,
    pub size: PageSize,
    pub orientation: Orientation,
    pub rotation: Rotation,
}

impl Page {
    /// A blank page with no rotation applied, per `create_blank_document`.
    pub fn blank(id: PageId, size: PageSize, orientation: Orientation) -> Self {
        Page {
            id,
            origin: PageOrigin::Blank,
            size,
            orientation,
            rotation: Rotation::None,
        }
    }

    /// A page backed by the immutable PDF opened for this session.
    pub fn base(
        id: PageId,
        page_index: u32,
        size: PageSize,
        orientation: Orientation,
        rotation: Rotation,
    ) -> Self {
        Page {
            id,
            origin: PageOrigin::Base { page_index },
            size,
            orientation,
            rotation,
        }
    }

    /// A page backed by a separately registered PDF source.
    pub fn imported(
        id: PageId,
        source: ImportedDocumentId,
        page_index: u32,
        size: PageSize,
        orientation: Orientation,
        rotation: Rotation,
    ) -> Self {
        Page {
            id,
            origin: PageOrigin::Imported { source, page_index },
            size,
            orientation,
            rotation,
        }
    }
}

/// The pure document model: pages, annotations, the undoable edit log, the
/// non-undoable audit log, and optional security context.
///
/// `pending_edits` (EditLog) and `audit_log` (AuditLog) are intentionally
/// separate structures — see `audit_log` module docs for why undo must never
/// reach security/consent events.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub pages: Vec<Page>,
    pub annotations: AnnotationSet,
    pub form_fields: FormFieldSet,
    pub pending_edits: EditLog,
    pub audit_log: AuditLog,
    pub security: Option<SecurityContext>,
}

impl Document {
    /// An empty document with no pages, annotations, edits, or security
    /// context — the starting point for `create_blank_document`.
    pub fn blank() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_page_records_blank_origin() {
        let page = Page::blank(PageId(7), PageSize::A4, Orientation::Portrait);

        assert_eq!(page.origin, PageOrigin::Blank);
    }

    #[test]
    fn base_page_records_source_page_index() {
        let page = Page::base(
            PageId(7),
            3,
            PageSize::A4,
            Orientation::Portrait,
            Rotation::Clockwise90,
        );

        assert_eq!(page.origin, PageOrigin::Base { page_index: 3 });
    }

    #[test]
    fn imported_page_records_source_and_page_index() {
        let page = Page::imported(
            PageId(7),
            ImportedDocumentId(11),
            3,
            PageSize::A4,
            Orientation::Portrait,
            Rotation::None,
        );

        assert_eq!(
            page.origin,
            PageOrigin::Imported {
                source: ImportedDocumentId(11),
                page_index: 3,
            }
        );
    }
}
