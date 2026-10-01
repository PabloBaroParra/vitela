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
//! ## One PDF per call
//!
//! The Linux shell imports a multi-file pick as one undo step. Here each call
//! is one file and one `ImportPages`, so one undo takes back one file. A
//! shell that lets the user pick several calls this once per file; the
//! password a file needs is then asked about that file alone, instead of
//! threading a password map across the boundary.
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

use crate::document::DocumentHandle;
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

/// Inserts every page of the PDF in `bytes` at position `index` (the
/// document's page count appends), as one undoable edit.
///
/// A shell calls `refresh_preview` afterwards, exactly as after any other
/// page-structure edit: the new pages only render from the refreshed
/// preview.
///
/// # Errors
///
/// - [`FfiError::PasswordRequired`] or [`FfiError::WrongPassword`] when the
///   source is encrypted and `password` does not open it — including when it
///   is `None`, which may come back as either. Both mean the same thing to a
///   shell: ask for the password and call again.
/// - [`FfiError::UnsupportedOperation`] when one of the gates in the module
///   docs refuses, when the source forbids copying its pages, has none, or
///   carries something an import cannot graft (a signature, an XFA form,
///   optional content, inline form fields).
/// - [`FfiError::PageIndexOutOfBounds`] when `index` is past the end.
#[uniffi::export]
pub fn import_pdf(
    handle: &DocumentHandle,
    bytes: Vec<u8>,
    password: Option<String>,
    index: u32,
) -> Result<FfiImportReport, FfiError> {
    let mut state = handle.lock();
    let security = state.document().security.as_ref();
    if !pdf_manip::content_editing_is_allowed(security) {
        return Err(FfiError::UnsupportedOperation {
            detail: "this document does not permit changing its content".to_string(),
        });
    }
    if !pdf_manip::document_assembly_is_allowed(security) {
        return Err(FfiError::UnsupportedOperation {
            detail: "this document does not permit inserting, removing or rotating its pages"
                .to_string(),
        });
    }
    if let Some(blocker) = state.full_rewrite_blocker() {
        return Err(FfiError::UnsupportedOperation {
            detail: rewrite_refusal(blocker).to_string(),
        });
    }
    if index as usize > state.document().pages.len() {
        return Err(FfiError::PageIndexOutOfBounds { index });
    }

    let (source, _) = pdf_manip::open_import_source_from_bytes(&bytes, password.as_deref())?;
    let every_page: Vec<usize> = (0..source.page_count()).collect();
    if every_page.is_empty() {
        return Err(FfiError::UnsupportedOperation {
            detail: "the PDF has no pages".to_string(),
        });
    }
    let warnings = pdf_manip::graft_report(&source, &every_page)?
        .warnings()
        .iter()
        .map(ToString::to_string)
        .collect();

    let id = state.next_imported_id()?;
    // The document's own cursor, never one past the highest id it holds: see
    // `Document::next_page_id` for the delete that makes those two differ.
    let first_page_id =
        state
            .document()
            .next_page_id()
            .ok_or_else(|| FfiError::UnsupportedOperation {
                detail: "this document cannot hold any more pages".to_string(),
            })?;
    let pages = pdf_save::imported_pages_from_lopdf(&source, id, first_page_id)?;
    let page_count = u32::try_from(pages.len()).map_err(|_| FfiError::UnsupportedOperation {
        detail: "the PDF has too many pages".to_string(),
    })?;
    state.import_pages(id, source, index as usize, pages)?;

    Ok(FfiImportReport {
        page_count,
        warnings,
        source_id: id.0,
    })
}

/// The reader-facing words for a [`pdf_save::RewriteBlocker`] met by an
/// import — the same split between a log reason and a shell message the
/// extract module's own `rewrite_refusal` makes.
fn rewrite_refusal(blocker: pdf_save::RewriteBlocker) -> &'static str {
    match blocker {
        pdf_save::RewriteBlocker::IncompleteCredentials => {
            "Adding pages rewrites the whole file, which needs both this document's user and \
             owner passwords. Reopen it with both to continue."
        }
        _ => {
            "This document's encryption cannot yet be reproduced when the file is rewritten, \
             so pages cannot be added to it."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_incomplete_credentials_blocker_names_both_passwords() {
        let message = rewrite_refusal(pdf_save::RewriteBlocker::IncompleteCredentials);
        assert!(message.contains("user"));
        assert!(message.contains("owner"));
    }

    #[test]
    fn an_unsupported_handler_blocker_names_no_password_at_all() {
        let message = rewrite_refusal(pdf_save::RewriteBlocker::UnsupportedHandler);
        assert!(!message.contains("password"));
    }
}
