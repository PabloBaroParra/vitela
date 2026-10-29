//! Text blocks through the FFI (B25 P2, T-207): runs on one line read as the
//! block a reader sees, and moved, deleted or retyped as one undo step.
//!
//! Grouping and the batches themselves live in `pdf_edit::group`; this
//! module gates them like `apply_edit` gates a single content edit, and
//! decides what happens when a block's runs already have an edit queued.

use pdf_document::{Command, ContentItemId, Document, PageId, TextRun};

use crate::document::{apply_command, content_editing_is_allowed, DocumentHandle};
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
    let mut state = handle.lock();
    // One question, two answers, as in `apply_edit`: whether the document
    // permits it and whether a rewrite could ever keep it.
    if !content_editing_is_allowed(state.document()) {
        return Err(unsupported("content editing is not permitted"));
    }
    let runs = runs
        .into_iter()
        .map(|run| state.text_run(run))
        .collect::<Result<Vec<_>, _>>()?;
    let command = build(&runs).ok_or_else(|| unsupported("a text block has at least one run"))?;

    match queued_batch(state.document(), &command) {
        // Folded into the queued batch: it keeps the snapshots the file
        // holds, so the untouched page is what it is checked against.
        Ok(Some((index, folded))) => {
            state.with_imported_sources(|sources| {
                pdf_save::validate_content_command(state.document(), state.base(), sources, &folded)
            })?;
            if !state.document_mut().pending_edits.amend(index, folded) {
                return Err(unsupported("the pending text edit is no longer available"));
            }
            Ok(())
        }
        // A step of its own. Its runs are named as the reader sees them now —
        // where a pending edit moved them, as one retyped them, or as one
        // inserted them — so it is checked against the page as the save will
        // find it, every pending edit replayed first. A refused fold keeps
        // its own reason when that check fails too.
        queued => {
            let checked = state.with_imported_sources(|sources| {
                pdf_save::validate_content_command_after_pending(
                    state.document(),
                    state.base(),
                    sources,
                    &command,
                )
            });
            match (queued, checked) {
                (Err(refusal), Err(_)) => return Err(refusal),
                (_, Err(error)) => return Err(error.into()),
                _ => {}
            }
            if !apply_command(state.document_mut(), command) {
                return Err(unsupported(
                    "this edit command was rejected against the open document",
                ));
            }
            Ok(())
        }
    }
}

/// What a page-content command edits: the kind of item and which one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Text(ContentItemId, PageId),
    Image(ContentItemId, PageId),
    Path(ContentItemId, PageId),
}

fn target(command: &Command) -> Option<Target> {
    Some(match command {
        Command::ReplaceTextRunContent { item, .. }
        | Command::ReplaceTextRunWithInsertedFont { item, .. }
        | Command::MoveTextRun { item, .. }
        | Command::RemoveTextRun(item)
        | Command::InsertTextRun(item) => Target::Text(item.id, item.page),
        Command::InsertImage { item, .. }
        | Command::RemoveImage { item, .. }
        | Command::MoveImage { item, .. }
        | Command::ResizeImage { item, .. }
        | Command::ReplaceImageSource { item, .. } => Target::Image(item.id, item.page),
        Command::MovePath { item, .. }
        | Command::ResizePath { item, .. }
        | Command::RemovePath(item)
        | Command::RestorePath(item) => Target::Path(item.id, item.page),
        _ => return None,
    })
}

fn children(command: &Command) -> &[Command] {
    match command {
        Command::Batch(children) => children,
        single => std::slice::from_ref(single),
    }
}

/// Where a block edit goes when its runs already have edits queued.
///
/// Every edit is validated, and saved, against the untouched file, so an
/// item a queued edit already changed cannot be named again by what the
/// reader sees now (the T-206 trap). A block edited again **the same way**
/// folds into the queued batch — moved again, or retyped again while the
/// reader keeps typing — keeping the queued originals and taking the new
/// destinations or text, so the pair is one undo step. Anything else
/// touching a queued item is refused with a reason a shell can show; `None`
/// means nothing is queued and the edit is appended.
fn queued_batch(
    document: &Document,
    command: &Command,
) -> Result<Option<(usize, Command)>, FfiError> {
    let wanted: Vec<Target> = children(command).iter().filter_map(target).collect();
    let entries = document.pending_edits.entries();
    let touched: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            children(entry)
                .iter()
                .filter_map(target)
                .any(|queued| wanted.contains(&queued))
        })
        .map(|(index, _)| index)
        .collect();
    let [index] = touched[..] else {
        return if touched.is_empty() {
            Ok(None)
        } else {
            Err(save_first())
        };
    };
    let (Command::Batch(queued), Command::Batch(next)) = (&entries[index], command) else {
        return Err(save_first());
    };
    // Every new child has to land on a queued child for the same item; the
    // queued children it does not name stay as they were — a retyped block's
    // removed runs stay removed when the run left behind is deleted too.
    let mut folded = queued.clone();
    for child in next {
        let at = queued
            .iter()
            .position(|earlier| target(earlier) == target(child))
            .ok_or_else(save_first)?;
        folded[at] = with_original(child, &queued[at]).ok_or_else(save_first)?;
    }
    Ok(Some((index, Command::Batch(folded))))
}

/// `next` carrying the item snapshot `earlier` was built from, when both are
/// the same kind of text edit or `next` removes the run; `None` otherwise.
///
/// The snapshot is what the untouched file holds, so it is what validation
/// and the save can find. What changes is only what the caller asked for
/// now: the destination of a move, the text of a retype — or, for a removal,
/// that the run is gone, whatever was queued for it before.
fn with_original(next: &Command, earlier: &Command) -> Option<Command> {
    if let Command::RemoveTextRun(_) = next {
        return match earlier {
            Command::MoveTextRun { item, .. }
            | Command::RemoveTextRun(item)
            | Command::ReplaceTextRunContent { item, .. }
            | Command::ReplaceTextRunWithInsertedFont { item, .. } => {
                Some(Command::RemoveTextRun(item.clone()))
            }
            _ => None,
        };
    }
    Some(match (next, earlier) {
        (Command::MoveTextRun { to, .. }, Command::MoveTextRun { item, .. }) => {
            Command::MoveTextRun {
                item: item.clone(),
                to: *to,
            }
        }
        (
            Command::ReplaceTextRunContent { after, .. },
            Command::ReplaceTextRunContent { item, .. },
        ) => Command::ReplaceTextRunContent {
            item: item.clone(),
            after: after.clone(),
        },
        (
            Command::ReplaceTextRunWithInsertedFont { after, .. },
            Command::ReplaceTextRunWithInsertedFont { item, .. },
        ) => Command::ReplaceTextRunWithInsertedFont {
            item: item.clone(),
            after: after.clone(),
        },
        _ => return None,
    })
}

/// Where a single content edit goes when its item sits inside a queued
/// batch: folded into that batch's child for the same item, when it is the
/// same kind of edit — the first run of a retyped block retyped again, one
/// run of a moved block moved on its own — or refused.
///
/// `apply_edit` folds a repeated edit into the queued *single* command; it
/// cannot otherwise reach inside a batch, and without this the edit would
/// fail the base-bytes validation with a reason that says nothing useful.
/// `None` when no batch holds the item.
pub(crate) fn fold_into_queued_batch(
    document: &Document,
    command: &Command,
) -> Result<Option<(usize, Command)>, FfiError> {
    let Some(wanted) = target(command) else {
        return Ok(None);
    };
    for (index, entry) in document.pending_edits.entries().iter().enumerate() {
        let Command::Batch(queued) = entry else {
            continue;
        };
        let Some(at) = queued
            .iter()
            .position(|child| target(child) == Some(wanted))
        else {
            continue;
        };
        let child = with_original(command, &queued[at]).ok_or_else(save_first)?;
        let mut folded = queued.clone();
        folded[at] = child;
        return Ok(Some((index, Command::Batch(folded))));
    }
    Ok(None)
}

fn save_first() -> FfiError {
    unsupported("This text has an unsaved edit — save the document before editing it again.")
}

fn unsupported(detail: &str) -> FfiError {
    FfiError::UnsupportedOperation {
        detail: detail.to_string(),
    }
}
