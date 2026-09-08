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

    /// The zero-based position `id` occupies in the current page order, or
    /// `None` when the document does not hold that page.
    ///
    /// # Two indices, and which one this is
    ///
    /// A `PageId` is a stable identity: handed out once, never renumbered, so
    /// it survives reorder, delete and insert. Positions are the opposite —
    /// every insert or move shifts the ones after it. Two different positions
    /// exist for one `PageId`, and they are not interchangeable:
    ///
    /// - the **logical** position, the index into [`Document::pages`], which
    ///   is what this returns. It is the order `pdf_save` materializes, the
    ///   one `pdf_save::bridge::page_object_ids` zips against the written PDF;
    /// - the **backend** position, the page index of whatever PDF the render
    ///   backend currently has open.
    ///
    /// They agree right after a document is opened — `populate_document`
    /// assigns `PageId(i)` to page `i` — and again after every save/reopen,
    /// because the reopened bytes were written in [`Document::pages`] order
    /// while the model (and its ids) is preserved across the refresh. They
    /// diverge in between, for exactly as long as a page op sits unsaved: the
    /// backend still holds the pre-op order.
    ///
    /// So do not reach for this to address the render backend while page ops
    /// are pending, and do not read `PageId.0` as a logical index — neither
    /// mistake fails loudly. Each one silently names a different page, which
    /// is how an annotation, a form widget or a selection ends up drawn on a
    /// neighbour.
    ///
    /// Once pages can be imported, `PageId.0` stops being a position of any
    /// kind: a page grafted from another PDF gets a fresh id that was never
    /// an index into anything. Resolving through this method is what keeps
    /// working when that lands.
    pub fn render_index(&self, id: PageId) -> Option<usize> {
        self.pages.iter().position(|page| page.id == id)
    }

    /// The stable `PageId` at a zero-based logical position, or `None` past
    /// the last page.
    ///
    /// The inverse of [`Document::render_index`], and the one a canvas wants:
    /// resolve the position **once** outside a per-item loop, then compare
    /// ids, instead of resolving an index for every annotation or field.
    pub fn page_id_at(&self, index: usize) -> Option<PageId> {
        self.pages.get(index).map(|page| page.id)
    }

    /// The page at a zero-based logical position, or `None` past the last
    /// page.
    pub fn page_at(&self, index: usize) -> Option<&Page> {
        self.pages.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document whose `PageId`s deliberately do not match their positions,
    /// so a test that passes by accident on `PageId.0 == index` cannot.
    fn document_with_page_ids(ids: &[u32]) -> Document {
        Document {
            pages: ids
                .iter()
                .map(|id| Page::blank(PageId(*id), PageSize::A4, Orientation::Portrait))
                .collect(),
            ..Document::default()
        }
    }

    #[test]
    fn render_index_is_the_position_in_page_order() {
        let document = document_with_page_ids(&[9, 4, 7]);

        assert_eq!(document.render_index(PageId(9)), Some(0));
        assert_eq!(document.render_index(PageId(4)), Some(1));
        assert_eq!(document.render_index(PageId(7)), Some(2));
    }

    #[test]
    fn render_index_is_none_for_a_page_the_document_does_not_hold() {
        let document = document_with_page_ids(&[9, 4, 7]);

        assert_eq!(document.render_index(PageId(0)), None);
    }

    #[test]
    fn render_index_follows_a_reorder() {
        let mut document = document_with_page_ids(&[9, 4, 7]);
        document.pages.swap(0, 2);

        assert_eq!(document.render_index(PageId(9)), Some(2));
        assert_eq!(document.render_index(PageId(7)), Some(0));
    }

    #[test]
    fn render_index_follows_an_insert_in_the_middle() {
        let mut document = document_with_page_ids(&[9, 4, 7]);
        document.pages.insert(
            1,
            Page::blank(PageId(12), PageSize::A4, Orientation::Portrait),
        );

        assert_eq!(document.render_index(PageId(9)), Some(0));
        assert_eq!(document.render_index(PageId(12)), Some(1));
        assert_eq!(document.render_index(PageId(4)), Some(2));
        assert_eq!(document.render_index(PageId(7)), Some(3));
    }

    #[test]
    fn page_id_at_resolves_a_render_position() {
        let document = document_with_page_ids(&[9, 4, 7]);

        assert_eq!(document.page_id_at(0), Some(PageId(9)));
        assert_eq!(document.page_id_at(2), Some(PageId(7)));
    }

    #[test]
    fn page_id_at_is_none_past_the_last_page() {
        let document = document_with_page_ids(&[9, 4, 7]);

        assert_eq!(document.page_id_at(3), None);
    }

    #[test]
    fn page_at_returns_the_page_in_that_render_position() {
        let document = document_with_page_ids(&[9, 4, 7]);

        assert_eq!(document.page_at(1).map(|page| page.id), Some(PageId(4)));
        assert_eq!(document.page_at(3), None);
    }

    #[test]
    fn render_index_and_page_id_at_round_trip() {
        let document = document_with_page_ids(&[9, 4, 7]);

        for (position, page) in document.pages.iter().enumerate() {
            assert_eq!(document.render_index(page.id), Some(position));
            assert_eq!(document.page_id_at(position), Some(page.id));
        }
    }

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
