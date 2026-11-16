//! How a text run looks *now* — what a shell's bold / italic / underline
//! toggles show before the reader presses one.
//!
//! The file says how a run is painted ([`crate::text_run_look_of`]); the
//! pending queue may already hold a restyle of it that no save has written.
//! The toggles have to show that queued look, and whether a style can come
//! off depends on who put it there: one this session added folds back out of
//! the queue, one the file had needs the file's answer.

use pdf_document::{Command, Document, TextRun, TextRunLook};
use pdf_edit::RunLook;
use pdf_manip::LopdfDocument;

use crate::bridge::ImportedSources;
use crate::error::SaveError;
use crate::origin::text_run_look_of;

/// How `run` looks with every pending edit in place: its last queued
/// restyle (on its own or inside a batch) over the file's look.
///
/// A run only a pending edit holds — an insertion, a paste — is in no file
/// yet: it reads plain, with every style removable. `None` when the run's
/// page has no content, or its look cannot be read at all.
pub fn pending_text_run_look_of(
    document: &Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    run: &TextRun,
) -> Result<Option<RunLook>, SaveError> {
    let file = match text_run_look_of(document, base, sources, run) {
        Ok(Some(file)) => file,
        // A pending run is not in the file, so reading it there fails or
        // finds nothing; that is expected, not an error.
        Ok(None) | Err(_) if pdf_edit::pending_log_index(run.id).is_some() => RunLook {
            look: TextRunLook::default(),
            can_remove_bold: true,
            can_remove_italic: true,
        },
        Ok(None) => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(Some(match queued_restyle(document, run) {
        // A style this session put on comes off by folding the restyle
        // back; one the file had needs the file's answer.
        Some((from, to)) => RunLook {
            look: to,
            can_remove_bold: !from.bold || file.can_remove_bold,
            can_remove_italic: !from.italic || file.can_remove_italic,
        },
        None => file,
    }))
}

/// The look before and after the last restyle queued for `run`.
fn queued_restyle(document: &Document, run: &TextRun) -> Option<(TextRunLook, TextRunLook)> {
    document
        .pending_edits
        .entries()
        .iter()
        .rev()
        .flat_map(|entry| match entry {
            Command::Batch(children) => children.iter().rev().collect::<Vec<_>>(),
            single => vec![single],
        })
        .find_map(|command| match command {
            Command::RestyleTextRun { item, from, to }
                if item.id == run.id && item.page == run.page =>
            {
                Some((*from, *to))
            }
            _ => None,
        })
}
