//! Import: adding another PDF's pages to the open document, across the
//! UniFFI boundary.
//!
//! The FFI twin of the Linux shell's "Add PDFs"
//! (`apps/linux-gtk/src/app/organize/import.rs`). That shell links
//! `pdf_manip`/`pdf_save` directly and keeps the imported sources in its own
//! session; a shell on the other side of this boundary cannot hold a
//! `LopdfDocument`, so the handle keeps them instead (see
//! `DocumentState::imported`) and every save, preview and content read made
//! through it resolves imported pages against them.
//!
//! ## Two phases, like the Linux shell
//!
//! The Linux shell prepares a whole multi-file pick off the main thread and
//! applies it as one undo step. Across this boundary that is two calls:
//!
//! 1. [`prepare_import`] opens one source — the parse, the decryption, the
//!    graft check — and hands back a [`PreparedImport`]. A shell calls it
//!    once per picked file, so it can show progress between files, ask a
//!    locked file for its own password, and stop by simply dropping what it
//!    prepared. Nothing about the open document changes.
//! 2. [`import_prepared`] adds every prepared source at once as ONE
//!    `ImportPages`: one undo takes the whole pick back. All or nothing — a
//!    refusal leaves the document and every prepared source as they were.
//!
//! [`import_pdf`] is the one-file case of the same two calls.
//!
//! ## The gates
//!
//! The same three the Linux shell asks before it opens its file chooser, in
//! the same order, asked here so no shell can skip one:
//!
//! 1. content editing (`/P` bit 4) — grafting pages rewrites the document's
//!    own structures, its form among them;
//! 2. page assembly (`/P` bit 11) — the pages the document has change;
//! 3. the full-rewrite blocker — a page-structure edit forces the full
//!    rewrite writer, and an encrypted file opened with one password cannot
//!    be re-encrypted by it.
//!
//! The *source's* own copy permission is `pdf_manip`'s rule and is asked
//! there, by `open_import_source_from_bytes`, before a single page is
//! decrypted.

use std::sync::{Arc, Mutex, PoisonError};

use pdf_manip::LopdfDocument;

use crate::document::{DocumentHandle, DocumentState};
use crate::error::FfiError;

/// What an import did, for the shell to report.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FfiImportReport {
    /// How many pages the import added — every page the source had.
    pub page_count: u32,
    /// What the pages could not bring across exactly as they were, one line
    /// each (a form field renamed on arrival, a link that pointed outside the
    /// imported pages, …). Empty when nothing was lost. The pages are already
    /// in the document; an undo takes them back out.
    pub warnings: Vec<String>,
    /// The id the added pages' block carries in `document_blocks`
    /// (`FfiBlockSource::Imported`) — how a shell tells which of the files it
    /// picked a block came from. Never reused within one handle, not even
    /// after an undo takes the pages back out.
    pub source_id: u64,
}

/// What [`import_prepared`] did, for the shell to report.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FfiBatchImportReport {
    /// How many pages the whole batch added.
    pub page_count: u32,
    /// The block id of each source, in the order the sources were given —
    /// see [`FfiImportReport::source_id`].
    pub source_ids: Vec<u64>,
}

/// One source PDF, opened and checked by [`prepare_import`] and waiting to be
/// added by [`import_prepared`]. Holds the decrypted document; dropping it is
/// how a shell abandons the import.
///
/// Spent once imported: the open document takes the source over, so a second
/// [`import_prepared`] with it is refused.
#[derive(uniffi::Object)]
pub struct PreparedImport {
    source: Mutex<Option<LopdfDocument>>,
    page_count: u32,
    warnings: Vec<String>,
}

impl std::fmt::Debug for PreparedImport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedImport")
            .field("page_count", &self.page_count)
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

#[uniffi::export]
impl PreparedImport {
    /// How many pages importing this source adds — every page it has.
    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    /// What this source's pages will not bring across exactly as they are
    /// (see [`FfiImportReport::warnings`]) — known before anything is added,
    /// so a shell can ask before importing, as the Linux shell does.
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }
}

/// Opens the PDF in `bytes` as an import source: decrypts it, checks its own
/// copy permission, and checks every page can be grafted. Changes nothing;
/// see the module docs for where this sits.
///
/// # Errors
///
/// - [`FfiError::PasswordRequired`] or [`FfiError::WrongPassword`] when the
///   source is encrypted and `password` does not open it — including when it
///   is `None`, which may come back as either. Both mean the same thing to a
///   shell: ask for the password and call again.
/// - [`FfiError::UnsupportedOperation`] when the source forbids copying its
///   pages, has none, or carries something an import cannot graft (a
///   signature, an XFA form, optional content, inline form fields).
#[uniffi::export]
pub fn prepare_import(
    bytes: Vec<u8>,
    password: Option<String>,
) -> Result<Arc<PreparedImport>, FfiError> {
    let (source, _) = pdf_manip::open_import_source_from_bytes(&bytes, password.as_deref())?;
    let every_page: Vec<usize> = (0..source.page_count()).collect();
    if every_page.is_empty() {
        return Err(unsupported("the PDF has no pages"));
    }
    let warnings = pdf_manip::graft_report(&source, &every_page)?
        .warnings()
        .iter()
        .map(ToString::to_string)
        .collect();
    let page_count =
        u32::try_from(every_page.len()).map_err(|_| unsupported("the PDF has too many pages"))?;
    Ok(Arc::new(PreparedImport {
        source: Mutex::new(Some(source)),
        page_count,
        warnings,
    }))
}

/// Why `handle` would refuse any import right now, as a lower-case clause, or
/// `None` when it would not. Cheap: it reads the protection. A shell asks
/// before its file picker, so the reader is not sent to pick files that can
/// only be refused.
#[uniffi::export]
pub fn import_refusal(handle: &DocumentHandle) -> Option<String> {
    refusal(&handle.lock())
}

/// Adds every page of every source in `sources`, in that order, at position
/// `index` (the document's page count appends), as ONE undoable edit.
///
/// All or nothing: on any error the document is unchanged and every source
/// is still unspent, so a shell can fix the cause and call again. A shell
/// calls `refresh_preview` afterwards, as after any page-structure edit.
///
/// # Errors
///
/// - [`FfiError::UnsupportedOperation`] when [`import_refusal`] refuses (with
///   the same detail), when `sources` is empty, names the same source twice,
///   or holds one already imported.
/// - [`FfiError::PageIndexOutOfBounds`] when `index` is past the end.
#[uniffi::export]
pub fn import_prepared(
    handle: &DocumentHandle,
    sources: Vec<Arc<PreparedImport>>,
    index: u32,
) -> Result<FfiBatchImportReport, FfiError> {
    let mut state = handle.lock();
    if let Some(detail) = refusal(&state) {
        return Err(FfiError::UnsupportedOperation { detail });
    }
    // Checked before any lock is taken: locking one source's mutex twice
    // would deadlock this thread. Only this boundary can be handed the same
    // source twice — the core takes each as a distinct `&mut` slot. Asked
    // ahead of the core's own "no source" check, which it cannot disagree
    // with: an empty pick names no source twice.
    for (position, source) in sources.iter().enumerate() {
        if sources[..position]
            .iter()
            .any(|earlier| Arc::ptr_eq(earlier, source))
        {
            return Err(unsupported("the same PDF was picked twice"));
        }
    }

    // Different handles can race to consume overlapping batches. Acquire the
    // source locks in one global order, then restore pick order for grafting.
    let mut lock_order: Vec<_> = sources.iter().enumerate().collect();
    lock_order.sort_unstable_by_key(|(_, source)| Arc::as_ptr(source));
    let mut held: Vec<_> = lock_order
        .into_iter()
        .map(|(position, source)| {
            (
                position,
                source.source.lock().unwrap_or_else(PoisonError::into_inner),
            )
        })
        .collect();
    held.sort_unstable_by_key(|(position, _)| *position);
    let mut slots: Vec<&mut Option<LopdfDocument>> =
        held.iter_mut().map(|(_, source)| &mut **source).collect();

    // The rest — no source, the position, a spent source, the id cursors,
    // the one `ImportPages` and keeping the sources — is the core's
    // (`pdf_save::import`), and so are its words.
    let outcome = state.import_sources(index as usize, &mut slots)?;
    Ok(FfiBatchImportReport {
        page_count: outcome.page_count,
        source_ids: outcome.ids.iter().map(|id| id.0).collect(),
    })
}

/// Inserts every page of the PDF in `bytes` at position `index` (the
/// document's page count appends), as one undoable edit: [`prepare_import`]
/// and [`import_prepared`] for one file.
///
/// The document's own gates and `index` are asked before the source is even
/// parsed, so a refusal never costs a decryption.
///
/// # Errors
///
/// Those of [`prepare_import`] and [`import_prepared`].
#[uniffi::export]
pub fn import_pdf(
    handle: &DocumentHandle,
    bytes: Vec<u8>,
    password: Option<String>,
    index: u32,
) -> Result<FfiImportReport, FfiError> {
    {
        let state = handle.lock();
        if let Some(detail) = refusal(&state) {
            return Err(FfiError::UnsupportedOperation { detail });
        }
        if index as usize > state.document().pages.len() {
            return Err(FfiError::PageIndexOutOfBounds { index });
        }
    }

    let prepared = prepare_import(bytes, password)?;
    let warnings = prepared.warnings();
    let report = import_prepared(handle, vec![prepared], index)?;
    Ok(FfiImportReport {
        page_count: report.page_count,
        warnings,
        source_id: report.source_ids[0],
    })
}

/// The three gates in the module docs, in that order — the core's
/// `pdf_save::import_refusal`, in its words.
fn refusal(state: &DocumentState) -> Option<String> {
    pdf_save::import_refusal(state.document().security.as_ref()).map(|refusal| refusal.to_string())
}

fn unsupported(detail: &str) -> FfiError {
    FfiError::UnsupportedOperation {
        detail: detail.to_string(),
    }
}
