//! `LopdfDocument`: the lopdf-backed document handle used across
//! `pdf-manip`'s public API (T-022..T-026).
//!
//! This wraps `lopdf::Document` rather than re-exporting it directly so that
//! `pdf-manip`'s own callers depend on a type this crate owns — matching
//! design.md's "callers never see lopdf types" intent. `pdf_document` itself
//! never depends on lopdf at all (see crate-level docs); this wrapper is
//! `pdf-manip`'s own boundary, one layer further out.

/// Opaque handle wrapping an in-memory `lopdf::Document`.
///
/// Used by every public manipulation function rather than leaking
/// `lopdf::Document` through this crate's API.
#[derive(Debug, Clone)]
pub struct LopdfDocument(pub(crate) lopdf::Document);

impl LopdfDocument {
    /// Wraps an existing `lopdf::Document`. Exposed (rather than
    /// crate-private) so sibling crates (e.g. the future `pdf-save`, Batch 6)
    /// and this crate's own integration tests can construct/inspect handles
    /// directly without duplicating lopdf-level document construction.
    pub fn from_lopdf(document: lopdf::Document) -> Self {
        Self(document)
    }

    /// Unwraps into the underlying `lopdf::Document`.
    pub fn into_lopdf(self) -> lopdf::Document {
        self.0
    }

    /// Borrows the underlying `lopdf::Document`.
    pub fn as_lopdf(&self) -> &lopdf::Document {
        &self.0
    }

    /// Mutably borrows the underlying `lopdf::Document`.
    pub fn as_lopdf_mut(&mut self) -> &mut lopdf::Document {
        &mut self.0
    }

    /// Number of pages currently in the document.
    pub fn page_count(&self) -> usize {
        self.0.get_pages().len()
    }
}

/// Width/height in PDF points for `size`, swapped for `Landscape`
/// orientation. Shared by `create_blank_document` and `insert_blank_page`.
pub(crate) fn oriented_dimensions(
    size: pdf_document::PageSize,
    orientation: pdf_document::Orientation,
) -> (f64, f64) {
    let (width, height) = size.dimensions_pt();
    match orientation {
        pdf_document::Orientation::Portrait => (width, height),
        pdf_document::Orientation::Landscape => (height, width),
    }
}
