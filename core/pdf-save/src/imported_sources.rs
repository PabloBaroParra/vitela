//! Session-lifetime registry of imported PDFs (phase 1 of batch PDF assembly,
//! `docs/batch-pdf-assembly.md` §1).
//!
//! [`crate::bridge::ImportedSources`] only borrows a slice for the duration of one
//! save — it has to, since `pdf_document` cannot own `LopdfDocument`s. This
//! module is what a session holds *between* saves: every PDF a user imports
//! is registered here exactly once, so a page imported today can still be
//! materialized by a save tomorrow, and every registration gets a fresh
//! [`ImportedDocumentId`] nothing else in the session could already be using.

use pdf_document::ImportedDocumentId;
use pdf_manip::LopdfDocument;

/// Owns every PDF imported into the current editing session, keyed by a
/// freshly assigned, unique [`ImportedDocumentId`].
#[derive(Debug, Clone, Default)]
pub struct ImportedSourceRegistry {
    sources: Vec<(ImportedDocumentId, LopdfDocument)>,
}

impl ImportedSourceRegistry {
    /// An empty registry, for a session that has not imported anything yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `document` as a newly imported source and returns the id it
    /// was assigned. One call registers exactly one source — a PDF imported
    /// for several of its pages is registered once here, not once per page.
    pub fn register(&mut self, document: LopdfDocument) -> ImportedDocumentId {
        let id = self.next_id();
        self.sources.push((id, document));
        id
    }

    /// One past the highest id already registered, or `0` for an empty
    /// registry — same convention as this codebase's other id allocators
    /// (e.g. the Linux shell's `next_form_field_id`), so a fresh registry
    /// only ever grows.
    fn next_id(&self) -> ImportedDocumentId {
        self.sources
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .map_or(ImportedDocumentId(0), |max| ImportedDocumentId(max + 1))
    }

    /// Every registered source as `(id, &document)` pairs, ready for the
    /// caller to build an [`crate::bridge::ImportedSources`] slice on the
    /// stack — the same pattern
    /// [`ImportedSources::new`](crate::bridge::ImportedSources::new) itself
    /// documents.
    pub fn pairs(&self) -> Vec<(ImportedDocumentId, &LopdfDocument)> {
        self.sources.iter().map(|(id, doc)| (*id, doc)).collect()
    }

    /// Whether no PDF has been registered yet.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::ImportedSources;
    use pdf_document::{Orientation, PageSize};
    use pdf_manip::create_blank_document;

    fn a_document() -> LopdfDocument {
        create_blank_document(PageSize::A4, Orientation::Portrait)
    }

    #[test]
    fn empty_registry_resolves_nothing() {
        let registry = ImportedSourceRegistry::new();

        assert!(registry.is_empty());
        let pairs = registry.pairs();
        assert!(ImportedSources::new(&pairs)
            .get(ImportedDocumentId(0))
            .is_none());
    }

    #[test]
    fn register_assigns_sequential_ids_starting_at_zero() {
        let mut registry = ImportedSourceRegistry::new();

        let first = registry.register(a_document());
        let second = registry.register(a_document());

        assert_eq!(first, ImportedDocumentId(0));
        assert_eq!(second, ImportedDocumentId(1));
    }

    #[test]
    fn register_never_reassigns_an_id_already_in_use() {
        let mut registry = ImportedSourceRegistry::new();
        for _ in 0..5 {
            registry.register(a_document());
        }

        let ids: std::collections::HashSet<_> =
            registry.pairs().into_iter().map(|(id, _)| id).collect();

        assert_eq!(ids.len(), 5);
    }

    #[test]
    fn pairs_resolve_through_imported_sources_by_id() {
        let mut registry = ImportedSourceRegistry::new();
        let first = registry.register(a_document());
        let second = registry.register(a_document());

        let pairs = registry.pairs();
        let sources = ImportedSources::new(&pairs);

        assert!(sources.get(first).is_some());
        assert!(sources.get(second).is_some());
        assert!(sources.get(ImportedDocumentId(99)).is_none());
    }
}
