//! Session-lifetime registry of imported PDFs (phase 1 of batch PDF assembly,
//! `docs/batch-pdf-assembly.md` §1).
//!
//! [`crate::bridge::ImportedSources`] only borrows a slice for the duration of one
//! save — it has to, since `pdf_document` cannot own `LopdfDocument`s. This
//! module is what a session holds *between* saves: every PDF a user imports
//! is registered here exactly once, so a page imported today can still be
//! materialized by a save tomorrow, and every registration gets a fresh
//! [`ImportedDocumentId`] nothing else in the session could already be using.

use pdf_document::{ImportedDocumentId, SecurityContext};
use pdf_manip::LopdfDocument;

use crate::error::SaveError;

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
    ///
    /// `security` is what `pdf_manip::open_document_with_passwords` returned
    /// for *this* PDF, not for the document being edited. Every source is
    /// checked on its own (checklist "Seguridad y firmas",
    /// `docs/batch-pdf-assembly.md` section 5): permission travels with the
    /// file, so opening five PDFs asks the question five times, and the
    /// destination's own permission — the assembly bit,
    /// `pdf_manip::document_assembly_is_allowed` — is a separate question
    /// asked of the destination.
    ///
    /// The bit that governs a source is `/P` bit 5, the one
    /// `pdf_manip::text_extraction_is_allowed` reads. Its spec wording is
    /// "copy or otherwise extract text **and graphics** from the document",
    /// and lifting a page's content into another file is exactly that — the
    /// name says text because text extraction was its first caller, not
    /// because the bit is narrower than the operation.
    ///
    /// This is the choke point for the check because it is the one place a
    /// source enters the session: refusing here means an unimportable PDF
    /// never reaches the page list, instead of being arranged into a
    /// document that later refuses to save.
    pub fn register(
        &mut self,
        document: LopdfDocument,
        security: Option<&SecurityContext>,
    ) -> Result<ImportedDocumentId, SaveError> {
        if !pdf_manip::text_extraction_is_allowed(security) {
            return Err(SaveError::SourceForbidsImport);
        }
        let id = self.next_id();
        self.sources.push((id, document));
        Ok(id)
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
    use pdf_document::{
        Credential, EncryptionCredentials, Orientation, PageSize, Permissions, SecurityHandler,
    };
    use pdf_manip::create_blank_document;

    fn a_document() -> LopdfDocument {
        create_blank_document(PageSize::A4, Orientation::Portrait)
    }

    /// An encrypted source opened with the user password and the `/P` bits
    /// the caller names.
    fn restricted(permissions: u32) -> SecurityContext {
        SecurityContext {
            handler: SecurityHandler::Rc4_128,
            credential: Credential::User,
            credentials: EncryptionCredentials::default(),
            permissions: Permissions(permissions),
        }
    }

    fn register(registry: &mut ImportedSourceRegistry) -> ImportedDocumentId {
        registry
            .register(a_document(), None)
            .expect("an unencrypted source is importable")
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

        let first = register(&mut registry);
        let second = register(&mut registry);

        assert_eq!(first, ImportedDocumentId(0));
        assert_eq!(second, ImportedDocumentId(1));
    }

    #[test]
    fn register_never_reassigns_an_id_already_in_use() {
        let mut registry = ImportedSourceRegistry::new();
        for _ in 0..5 {
            register(&mut registry);
        }

        let ids: std::collections::HashSet<_> =
            registry.pairs().into_iter().map(|(id, _)| id).collect();

        assert_eq!(ids.len(), 5);
    }

    #[test]
    fn pairs_resolve_through_imported_sources_by_id() {
        let mut registry = ImportedSourceRegistry::new();
        let first = register(&mut registry);
        let second = register(&mut registry);

        let pairs = registry.pairs();
        let sources = ImportedSources::new(&pairs);

        assert!(sources.get(first).is_some());
        assert!(sources.get(second).is_some());
        assert!(sources.get(ImportedDocumentId(99)).is_none());
    }

    /// `/P` bit 5 withheld: the PDF may be read, and its pages may not be
    /// lifted into another file.
    #[test]
    fn a_source_that_forbids_extraction_is_refused_and_never_registered() {
        let mut registry = ImportedSourceRegistry::new();
        const PRINTABLE: u32 = 1 << 2;

        let refused = registry.register(a_document(), Some(&restricted(PRINTABLE)));

        assert!(matches!(refused, Err(SaveError::SourceForbidsImport)));
        assert!(
            registry.is_empty(),
            "a refused source must not occupy an id or a slot"
        );
    }

    #[test]
    fn a_source_that_permits_extraction_registers() {
        let mut registry = ImportedSourceRegistry::new();
        const COPY_OR_EXTRACT: u32 = 1 << 4;

        assert!(registry
            .register(a_document(), Some(&restricted(COPY_OR_EXTRACT)))
            .is_ok());
    }

    /// The owner password authenticates the party that set the restriction,
    /// so it bypasses the bitmask here exactly as it does everywhere else.
    #[test]
    fn an_owner_open_may_import_a_source_that_grants_nothing() {
        let mut registry = ImportedSourceRegistry::new();
        let mut owner = restricted(0);
        owner.credential = Credential::Owner;

        assert!(registry.register(a_document(), Some(&owner)).is_ok());
    }
}
