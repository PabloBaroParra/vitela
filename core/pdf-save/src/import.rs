//! Adding other PDFs' pages to the open document as one undoable step — the
//! orchestration every boundary that imports runs.
//!
//! This used to live only in `pdf-ffi`'s `import_prepared`, so `pdf-wasm`
//! would have had to copy it. It needs nothing a boundary owns: the
//! [`Document`], the session's [`ImportedSourceRegistry`] and the sources,
//! already opened and checked (`pdf_manip::open_import_source_from_bytes`
//! asks each source's own copy permission before a page is decrypted).
//!
//! ## The gates
//!
//! [`import_refusal`] asks three questions of the *destination*, in this
//! order:
//!
//! 1. content editing (`/P` bit 4) — grafting pages rewrites the document's
//!    own structures, its form among them;
//! 2. page assembly (`/P` bit 11) — the pages the document has change;
//! 3. the full-rewrite blocker — a page-structure edit forces the full
//!    rewrite writer, and an encrypted file opened with one password cannot
//!    be re-encrypted by it.
//!
//! A boundary asks it before its file picker, so the reader is not sent to
//! pick files that can only be refused; [`import_sources`] asks it again.

use pdf_document::{Command, Document, ImportedDocumentId, PageId, SecurityContext};
use pdf_manip::LopdfDocument;

use crate::bridge::imported_pages_from_lopdf;
use crate::error::SaveError;
use crate::imported_sources::ImportedSourceRegistry;
use crate::rewrite::{full_rewrite_blocker, RewriteBlocker};

/// Why the destination refuses any import. Its `Display` is the
/// reader-facing message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportRefusal {
    /// The document withholds the modify-contents permission.
    ContentEditing,
    /// The document withholds the document-assembly permission.
    DocumentAssembly,
    /// Adding pages forces a full rewrite this document's encryption cannot
    /// survive.
    FullRewrite(RewriteBlocker),
}

impl std::fmt::Display for ImportRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ImportRefusal::ContentEditing => "this document does not permit changing its content",
            ImportRefusal::DocumentAssembly => {
                "this document does not permit inserting, removing or rotating its pages"
            }
            // The reader-facing words, not `RewriteBlocker::reason`, which is
            // a log reason naming API calls.
            ImportRefusal::FullRewrite(RewriteBlocker::IncompleteCredentials) => {
                "Adding pages rewrites the whole file, which needs both this document's user \
                 and owner passwords. Reopen it with both to continue."
            }
            ImportRefusal::FullRewrite(_) => {
                "This document's encryption cannot yet be reproduced when the file is \
                 rewritten, so pages cannot be added to it."
            }
        })
    }
}

impl std::error::Error for ImportRefusal {}

/// Why a document opened with `security` would refuse any import right now,
/// or `None` when it would not. Cheap: it reads the protection.
pub fn import_refusal(security: Option<&SecurityContext>) -> Option<ImportRefusal> {
    if !pdf_manip::content_editing_is_allowed(security) {
        return Some(ImportRefusal::ContentEditing);
    }
    if !pdf_manip::document_assembly_is_allowed(security) {
        return Some(ImportRefusal::DocumentAssembly);
    }
    full_rewrite_blocker(security).map(ImportRefusal::FullRewrite)
}

/// Why [`import_sources`] added nothing. Its `Display` is the reader-facing
/// message.
#[derive(Debug)]
pub enum ImportError {
    /// See [`import_refusal`].
    Refused(ImportRefusal),
    /// There was no source to import.
    NothingToImport,
    /// The insertion position is past the end of the document.
    IndexOutOfRange { index: usize },
    /// A source was already taken by an earlier import.
    AlreadyImported,
    /// The session has handed out every imported-document id.
    ImportIdsExhausted,
    /// The document has handed out every page id.
    PageIdsExhausted,
    /// The sources add more pages than a page count can name.
    TooManyPages,
    /// The model refused the `ImportPages` built from the sources.
    Rejected,
    /// A source's pages could not be read.
    Source(SaveError),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Refused(refusal) => refusal.fmt(f),
            ImportError::NothingToImport => f.write_str("there is no PDF to import"),
            ImportError::IndexOutOfRange { index } => {
                write!(f, "page position {index} is out of range")
            }
            ImportError::AlreadyImported => {
                f.write_str("one of these PDFs has already been imported")
            }
            ImportError::ImportIdsExhausted => {
                f.write_str("this document cannot import any more PDFs")
            }
            ImportError::PageIdsExhausted => {
                f.write_str("this document cannot hold any more pages")
            }
            ImportError::TooManyPages => f.write_str("these PDFs have too many pages"),
            ImportError::Rejected => {
                f.write_str("the imported pages were rejected against the open document")
            }
            ImportError::Source(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ImportError {}

/// What [`import_sources`] added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    /// How many pages the whole import added.
    pub page_count: u32,
    /// The id each source was registered under, in the order given — the id
    /// its pages' `PageOrigin::Imported` names.
    pub ids: Vec<ImportedDocumentId>,
}

/// Adds every page of every source in `sources`, in that order, at position
/// `index` (the document's page count appends), as ONE `ImportPages` — one
/// undo takes the whole pick back — and registers each source in `registry`
/// so every later save, preview and content read can materialize its pages.
///
/// A source is a slot so a caller can keep it until the import succeeds:
/// each one is taken (left `None`) only once the command applied. All or
/// nothing — on any error `document`, `registry` and every slot are as they
/// were. A slot that is already `None` was spent by an earlier import.
///
/// Ids are never reused within a session, not even after an undo took an
/// import's pages back out: the registry keeps every source, and a redo
/// needs them under the same id.
pub fn import_sources(
    document: &mut Document,
    registry: &mut ImportedSourceRegistry,
    index: usize,
    sources: &mut [&mut Option<LopdfDocument>],
) -> Result<ImportOutcome, ImportError> {
    if let Some(refusal) = import_refusal(document.security.as_ref()) {
        return Err(ImportError::Refused(refusal));
    }
    if sources.is_empty() {
        return Err(ImportError::NothingToImport);
    }
    if index > document.pages.len() {
        return Err(ImportError::IndexOutOfRange { index });
    }
    if sources.iter().any(|source| source.is_none()) {
        return Err(ImportError::AlreadyImported);
    }

    let first_id = registry.next_id().ok_or(ImportError::ImportIdsExhausted)?;
    // The document's own cursor, never one past the highest id it holds: see
    // `Document::next_page_id` for the delete that makes those two differ.
    let mut next_page_id = document
        .next_page_id()
        .ok_or(ImportError::PageIdsExhausted)?;
    let mut ids = Vec::with_capacity(sources.len());
    let mut pages = Vec::new();
    for (offset, source) in sources.iter().enumerate() {
        let id = first_id
            .0
            .checked_add(offset as u64)
            .map(ImportedDocumentId)
            .ok_or(ImportError::ImportIdsExhausted)?;
        let source = source
            .as_ref()
            .expect("checked above: every source is unspent");
        let imported =
            imported_pages_from_lopdf(source, id, next_page_id).map_err(ImportError::Source)?;
        next_page_id = u32::try_from(imported.len())
            .ok()
            .and_then(|count| next_page_id.0.checked_add(count))
            .map(PageId)
            .ok_or(ImportError::PageIdsExhausted)?;
        ids.push(id);
        pages.extend(imported);
    }
    let page_count = u32::try_from(pages.len()).map_err(|_| ImportError::TooManyPages)?;

    // `EditLog::apply` needs the log and the document at once.
    let mut log = std::mem::take(&mut document.pending_edits);
    let applied = log.apply(document, Command::ImportPages { index, pages });
    document.pending_edits = log;
    if !applied {
        return Err(ImportError::Rejected);
    }
    for (id, source) in ids.iter().zip(sources.iter_mut()) {
        registry.insert(
            *id,
            source
                .take()
                .expect("checked above: every source is unspent"),
        );
    }
    Ok(ImportOutcome { page_count, ids })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::document_from_lopdf;
    use pdf_document::{
        Credential, EncryptionCredentials, Orientation, PageOrigin, PageSize, Permissions,
        SecurityHandler,
    };

    const MODIFY_CONTENTS: u32 = 1 << 3;
    const ASSEMBLE: u32 = 1 << 10;

    /// A one-page lopdf document.
    fn one_page() -> LopdfDocument {
        pdf_manip::insert_blank_page(
            &pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait),
            0,
            PageSize::A4,
            Orientation::Portrait,
        )
        .expect("one blank page")
    }

    fn destination() -> Document {
        document_from_lopdf(&one_page(), None).expect("blank model")
    }

    fn user_open(permissions: u32) -> SecurityContext {
        SecurityContext {
            handler: SecurityHandler::Rc4_128,
            credential: Credential::User,
            credentials: EncryptionCredentials::default(),
            permissions: Permissions(permissions),
        }
    }

    #[test]
    fn an_unencrypted_document_refuses_nothing() {
        assert_eq!(import_refusal(None), None);
    }

    #[test]
    fn the_refusals_come_in_order_content_assembly_rewrite() {
        assert_eq!(
            import_refusal(Some(&user_open(ASSEMBLE))),
            Some(ImportRefusal::ContentEditing)
        );
        // Bit 4 carries assembly with it (`document_assembly_is_allowed`),
        // so a document that grants only bit 4 is refused by the rewrite.
        assert_eq!(
            import_refusal(Some(&user_open(MODIFY_CONTENTS))),
            Some(ImportRefusal::FullRewrite(
                RewriteBlocker::IncompleteCredentials
            ))
        );
        let mut owner = user_open(0);
        owner.credential = Credential::Owner;
        assert_eq!(
            import_refusal(Some(&owner)),
            Some(ImportRefusal::FullRewrite(
                RewriteBlocker::IncompleteCredentials
            ))
        );
    }

    #[test]
    fn an_incomplete_credentials_blocker_names_both_passwords() {
        let message = ImportRefusal::FullRewrite(RewriteBlocker::IncompleteCredentials).to_string();
        assert!(message.contains("user"));
        assert!(message.contains("owner"));
    }

    #[test]
    fn an_unsupported_handler_blocker_names_no_password_at_all() {
        let message = ImportRefusal::FullRewrite(RewriteBlocker::UnsupportedHandler).to_string();
        assert!(!message.contains("password"));
    }

    #[test]
    fn the_permission_refusals_read_as_before() {
        assert_eq!(
            ImportRefusal::ContentEditing.to_string(),
            "this document does not permit changing its content"
        );
        assert_eq!(
            ImportRefusal::DocumentAssembly.to_string(),
            "this document does not permit inserting, removing or rotating its pages"
        );
    }

    #[test]
    fn a_batch_is_one_undo_step_and_registers_every_source() {
        let mut document = destination();
        let mut registry = ImportedSourceRegistry::new();
        let (mut first, mut second) = (Some(one_page()), Some(one_page()));

        let outcome = import_sources(
            &mut document,
            &mut registry,
            1,
            &mut [&mut first, &mut second],
        )
        .expect("imports");

        assert_eq!(
            outcome,
            ImportOutcome {
                page_count: 2,
                ids: vec![ImportedDocumentId(0), ImportedDocumentId(1)],
            }
        );
        assert!(first.is_none() && second.is_none(), "both sources taken");
        assert_eq!(registry.pairs().len(), 2);
        let origins: Vec<_> = document.pages.iter().map(|page| page.origin).collect();
        assert_eq!(
            origins[1..],
            [
                PageOrigin::Imported {
                    source: ImportedDocumentId(0),
                    page_index: 0
                },
                PageOrigin::Imported {
                    source: ImportedDocumentId(1),
                    page_index: 0
                },
            ]
        );
        // Fresh page ids, minted from the document's cursor.
        assert_eq!(
            document
                .pages
                .iter()
                .map(|page| page.id)
                .collect::<Vec<_>>(),
            [PageId(0), PageId(1), PageId(2)]
        );

        let mut log = std::mem::take(&mut document.pending_edits);
        assert!(log.undo(&mut document));
        assert!(!log.can_undo(), "one step took the whole batch back");
        document.pending_edits = log;
        assert_eq!(document.pages.len(), 1);
    }

    #[test]
    fn an_id_is_never_reused_after_an_undo() {
        let mut document = destination();
        let mut registry = ImportedSourceRegistry::new();
        import_sources(
            &mut document,
            &mut registry,
            1,
            &mut [&mut Some(one_page())],
        )
        .expect("imports");
        let mut log = std::mem::take(&mut document.pending_edits);
        log.undo(&mut document);
        document.pending_edits = log;

        let outcome = import_sources(
            &mut document,
            &mut registry,
            1,
            &mut [&mut Some(one_page())],
        )
        .expect("imports again");

        assert_eq!(outcome.ids, [ImportedDocumentId(1)]);
    }

    #[test]
    fn nothing_to_import_is_refused() {
        let mut document = destination();

        let error = import_sources(
            &mut document,
            &mut ImportedSourceRegistry::new(),
            0,
            &mut [],
        )
        .expect_err("refused");

        assert!(matches!(error, ImportError::NothingToImport));
        assert_eq!(error.to_string(), "there is no PDF to import");
    }

    #[test]
    fn a_position_past_the_end_is_refused_and_takes_nothing() {
        let mut document = destination();
        let mut source = Some(one_page());

        let error = import_sources(
            &mut document,
            &mut ImportedSourceRegistry::new(),
            2,
            &mut [&mut source],
        )
        .expect_err("refused");

        assert!(matches!(error, ImportError::IndexOutOfRange { index: 2 }));
        assert!(source.is_some());
    }

    #[test]
    fn a_spent_source_refuses_the_whole_batch() {
        let mut document = destination();
        let mut registry = ImportedSourceRegistry::new();
        let (mut fresh, mut spent) = (Some(one_page()), None);

        let error = import_sources(
            &mut document,
            &mut registry,
            0,
            &mut [&mut fresh, &mut spent],
        )
        .expect_err("refused");

        assert!(matches!(error, ImportError::AlreadyImported));
        assert_eq!(
            error.to_string(),
            "one of these PDFs has already been imported"
        );
        assert!(fresh.is_some(), "the unspent source is left to the caller");
        assert!(registry.is_empty());
        assert_eq!(document.pages.len(), 1);
        assert!(!document.pending_edits.can_undo());
    }

    /// The destination's refusal comes before anything about the sources.
    #[test]
    fn a_refusing_document_takes_nothing() {
        let mut document = destination();
        document.security = Some(user_open(ASSEMBLE));
        let mut source = Some(one_page());

        let error = import_sources(
            &mut document,
            &mut ImportedSourceRegistry::new(),
            9,
            &mut [&mut source],
        )
        .expect_err("refused");

        assert!(matches!(
            error,
            ImportError::Refused(ImportRefusal::ContentEditing)
        ));
        assert!(source.is_some());
    }
}
