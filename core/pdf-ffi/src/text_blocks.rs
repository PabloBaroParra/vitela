//! Text blocks through the FFI (B25 P2, T-207): runs on one line read as the
//! block a reader sees, and moved, deleted or retyped as one undo step.
//!
//! Grouping and the batches themselves live in `pdf_edit::group`; this
//! module gates them like `apply_edit` gates a single content edit; what
//! happens when a block's runs already have an edit queued is
//! `pdf_save::record`'s to decide.

use pdf_document::{Command, TextRun};

use crate::document::{content_editing_is_allowed, DocumentHandle, DocumentState};
use crate::error::FfiError;
use crate::types::{FfiContentTextRun, FfiTextBlock};

/// The text blocks on `page`, as the reader sees them: pending edits are
/// layered on first, exactly as `read_page_content` does, so a block's runs
/// can be sent straight back into the block edits.
#[uniffi::export]
pub fn read_page_text_blocks(
    handle: &DocumentHandle,
    page: u32,
) -> Result<Vec<FfiTextBlock>, FfiError> {
    let runs = handle.read_page_content(page)?.text_runs;
    let core: Vec<TextRun> = runs.iter().cloned().map(TextRun::from).collect();
    Ok(pdf_edit::group_text_runs(&core)
        .into_iter()
        .map(|block| FfiTextBlock {
            runs: block
                .runs
                .iter()
                .filter_map(|id| runs.iter().find(|run| run.id == id.0).cloned())
                .collect(),
            bbox: block.bbox.into(),
            text: block.text,
        })
        .collect())
}

/// Moves every run of a block by `(dx, dy)` points, as one undo step. A block
/// moved again before a save keeps one undo step for both moves.
#[uniffi::export]
pub fn move_text_block(
    handle: &DocumentHandle,
    runs: Vec<FfiContentTextRun>,
    dx: f64,
    dy: f64,
) -> Result<(), FfiError> {
    if !dx.is_finite() || !dy.is_finite() {
        return Err(unsupported("Text coordinates must be finite."));
    }
    apply_block_edit(handle, runs, |runs| pdf_edit::move_block(runs, dx, dy))
}

/// Removes every run of a block, as one undo step.
#[uniffi::export]
pub fn remove_text_block(
    handle: &DocumentHandle,
    runs: Vec<FfiContentTextRun>,
) -> Result<(), FfiError> {
    apply_block_edit(handle, runs, pdf_edit::remove_block)
}

/// Puts all of `text` in the block's first run and removes the others, as
/// one undo step. `with_inserted_font` paints the first run in a standard
/// font instead of its own, as `ReplaceTextRunWithInsertedFont` does for a
/// single composite-font run.
#[uniffi::export]
pub fn retype_text_block(
    handle: &DocumentHandle,
    runs: Vec<FfiContentTextRun>,
    text: String,
    with_inserted_font: bool,
) -> Result<(), FfiError> {
    apply_block_edit(handle, runs, |runs| {
        pdf_edit::retype_block(runs, &text, with_inserted_font)
    })
}

fn apply_block_edit(
    handle: &DocumentHandle,
    runs: Vec<FfiContentTextRun>,
    build: impl FnOnce(&[TextRun]) -> Option<Command>,
) -> Result<(), FfiError> {
    apply_batch_edit(handle, |state| {
        let runs = runs
            .into_iter()
            .map(|run| state.text_run(run))
            .collect::<Result<Vec<_>, _>>()?;
        build(&runs).ok_or_else(|| unsupported("a text block has at least one run"))
    })
}

/// Records the batch `build` makes as one undo step, gated like
/// `apply_edit` gates a single content edit. Shared by text blocks and by
/// multi-item selections (`content_selection`, T-208).
pub(crate) fn apply_batch_edit(
    handle: &DocumentHandle,
    build: impl FnOnce(&DocumentState) -> Result<Command, FfiError>,
) -> Result<(), FfiError> {
    let mut state = handle.lock();
    // One question, two answers, as in `apply_edit`: whether the document
    // permits it and whether a rewrite could ever keep it.
    if !content_editing_is_allowed(state.document()) {
        return Err(unsupported("content editing is not permitted"));
    }
    let command = build(&state)?;
    // Tried as it stands first, as `apply_edit` does: a checkpoint costs a
    // rewrite, and most batches never need one.
    state.record_with_checkpoint(command, apply_batch_command)
}

/// Records one batch through `pdf_save::record_content_batch`: folded into
/// the queued batch for the same items, or checked against every pending
/// edit and appended. Leaves the session untouched when it refuses.
fn apply_batch_command(state: &mut DocumentState, command: Command) -> Result<(), FfiError> {
    state.record(|document, base, sources| {
        pdf_save::record_content_batch(document, base, sources, command)
    })
}

fn unsupported(detail: &str) -> FfiError {
    FfiError::UnsupportedOperation {
        detail: detail.to_string(),
    }
}
