//! How a page-content edit joins the pending queue (B25): folded into the
//! queued edit of the same item, or validated and appended — the one policy
//! every shell records through.
//!
//! This used to live in `pdf-ffi`, so a shell that calls the core directly
//! (`apps/linux-gtk`) had no way to reach it and kept rules of its own. It
//! needs nothing the FFI owns: the `Document`, the untouched bytes the queue
//! was recorded against, and the imported PDFs whose pages it may name.
//!
//! Each entry point leaves the document untouched when it refuses, so a
//! caller can retry after rewriting the base (`pdf-ffi`'s checkpoint).

use pdf_document::{Command, ContentItemId, Document, PageId, TextRun};
use pdf_manip::LopdfDocument;

use crate::bridge::ImportedSources;
use crate::content::{validate_content_command, validate_content_command_after_pending};
use crate::error::SaveError;

/// Why an edit was not recorded.
#[derive(Debug)]
pub enum RecordError {
    /// The queue cannot take this edit as it stands; the reason is worded
    /// for the reader.
    Refused(String),
    /// The edit itself does not apply to the page it names.
    Invalid(SaveError),
}

impl From<SaveError> for RecordError {
    fn from(error: SaveError) -> Self {
        Self::Invalid(error)
    }
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(detail) => f.write_str(detail),
            Self::Invalid(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for RecordError {}

/// Records one page-content edit: folded into the queued edit of the same
/// item when there is one, validated against `base`, then applied.
pub fn record_content_command(
    document: &mut Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    mut core_command: Command,
) -> Result<(), RecordError> {
    // An item inside a queued batch (a block edit, B25 T-207) is edited
    // again through that batch, which keeps the snapshot the file holds.
    let folded = match fold_into_queued_batch(document, &core_command) {
        Ok(folded) => folded,
        // Not the same kind of edit as the batch's child for this item —
        // retyping a run of a moved block, say. It can still follow the
        // batch as its own step if the save could replay it after it.
        Err(refusal) => {
            validate_content_command_after_pending(document, base, sources, &core_command)
                .map_err(|_| refusal)?;
            return append(document, core_command);
        }
    };
    if let Some((index, batch)) = folded {
        validate_content_command(document, base, sources, &batch)?;
        return amend(document, index, batch);
    }
    let queued = pending_text_amendment_index(document, &core_command)?;
    if let Some(index) = queued {
        // What a caller holds is the run as `read_page_content` reads it
        // now — the queued text edit already applied. The save replays
        // against the untouched bytes, so the fold keeps the snapshot the
        // queued command was built from, including repeated moves.
        core_command = with_queued_item(&document.pending_edits.entries()[index], core_command);
    }
    // Validate before recording, never only at save time — see
    // `validate_content_command` for why a content command recorded
    // unchecked can only fail later, and take every other queued edit down
    // with it.
    validate_content_command(document, base, sources, &core_command)?;

    match queued {
        // Retyping, deleting a retyped run or moving again amends its queued
        // command instead of appending another. `EditLog::amend`'s own docs
        // carry the reasoning: a save replays content commands in order
        // against a document it mutates as it goes, so a second command
        // against the same run would resolve against nothing and take the
        // whole save down.
        Some(index) => amend(document, index, core_command),
        None => append(document, core_command),
    }
}

/// Records one batch (a text block, a selection) as one undo step: folded
/// into the queued batch for the same items, or checked against every
/// pending edit and appended.
pub fn record_content_batch(
    document: &mut Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    command: Command,
) -> Result<(), RecordError> {
    match queued_batch(document, &command) {
        // Folded into the queued batch: it keeps the snapshots the file
        // holds, so the untouched page is what it is checked against.
        Ok(Some((index, folded))) => {
            validate_content_command(document, base, sources, &folded)?;
            amend(document, index, folded)
        }
        // A step of its own. Its runs are named as the reader sees them now —
        // where a pending edit moved them, as one retyped them, or as one
        // inserted them — so it is checked against the page as the save will
        // find it, every pending edit replayed first. A refused fold keeps
        // its own reason when that check fails too.
        queued => {
            let checked = validate_content_command_after_pending(document, base, sources, &command);
            match (queued, checked) {
                (Err(refusal), Err(_)) => return Err(refusal),
                (_, Err(error)) => return Err(error.into()),
                _ => {}
            }
            append(document, command)
        }
    }
}

fn append(document: &mut Document, command: Command) -> Result<(), RecordError> {
    // `EditLog::apply` needs `&mut EditLog` and `&mut Document` at once.
    let mut log = std::mem::take(&mut document.pending_edits);
    let applied = log.apply(document, command);
    document.pending_edits = log;
    if applied {
        Ok(())
    } else {
        Err(unsupported(
            "this edit command was rejected against the open document",
        ))
    }
}

fn amend(document: &mut Document, index: usize, command: Command) -> Result<(), RecordError> {
    if document.pending_edits.amend(index, command) {
        Ok(())
    } else {
        Err(unsupported("the pending text edit is no longer available"))
    }
}

/// The queued command `command` should replace rather than follow, if any.
///
/// A replacement or removal of a retyped run folds into that run's pending
/// replacement. The original snapshot is retained for validation and save.
/// Repeated moves retain their original snapshot and only change destination.
/// Synthetic insertion targets amend the insertion itself when retyping or moving.
/// An image moved again folds into its queued move the same way: what the
/// caller holds is the image already at its new place, which the untouched
/// bytes a save replays against do not contain.
/// A path moved, resized or removed after a queued move or resize folds into
/// that geometry edit for the same reason (B25 T-206).
fn pending_text_amendment_index(
    document: &Document,
    command: &Command,
) -> Result<Option<usize>, RecordError> {
    if let Command::MovePath { item, .. }
    | Command::ResizePath { item, .. }
    | Command::RemovePath(item) = command
    {
        return Ok(document.pending_edits.entries().iter().position(|queued| {
            matches!(queued,
                Command::MovePath { item: target, .. } | Command::ResizePath { item: target, .. }
                    if target.id == item.id && target.page == item.page)
        }));
    }
    if let Command::MoveImage { to, .. } | Command::ResizeImage { to, .. } = command {
        if !to.x.is_finite() || !to.y.is_finite() || !to.width.is_finite() || !to.height.is_finite()
        {
            return Err(RecordError::Refused(
                "Image coordinates must be finite.".to_string(),
            ));
        }
    }
    if let Command::MoveImage { item, .. }
    | Command::ResizeImage { item, .. }
    | Command::RemoveImage { item, .. } = command
    {
        return Ok(document.pending_edits.entries().iter().position(|queued| {
            matches!(queued,
                Command::MoveImage { item: target, .. } | Command::ResizeImage { item: target, .. }
                    if target.id == item.id && target.page == item.page)
        }));
    }
    if let Command::MoveTextRun { item, to } = command {
        if !to.x.is_finite() || !to.y.is_finite() {
            return Err(RecordError::Refused(
                "Text coordinates must be finite.".to_string(),
            ));
        }
        if pdf_edit::pending_log_index(item.id).is_some() {
            return pending_insertion_amendment_index(document, item);
        }
        // A replacement's estimated box is not a safe save-time move target.
        // Refused even when a move precedes that replacement.
        if document.pending_edits.entries().iter().any(|queued| {
            matches!(queued,
                Command::ReplaceTextRunContent { item: target, .. }
                    | Command::ReplaceTextRunWithInsertedFont { item: target, .. }
                    if target.id == item.id && target.page == item.page)
        }) {
            return Err(RecordError::Refused(
                "This text has an unsaved edit — save the document before moving it.".to_string(),
            ));
        }
        return Ok(document.pending_edits.entries().iter().position(|queued| {
            matches!(queued, Command::MoveTextRun { item: target, .. }
                if target.id == item.id && target.page == item.page)
        }));
    }
    let item = match command {
        Command::ReplaceTextRunContent { item, .. }
        | Command::ReplaceTextRunWithInsertedFont { item, .. }
        | Command::RemoveTextRun(item) => item,
        _ => return Ok(None),
    };

    if pdf_edit::pending_log_index(item.id).is_some() {
        if matches!(command, Command::ReplaceTextRunContent { .. }) {
            return pending_insertion_amendment_index(document, item);
        }
        // Only reaches the reader when the session cannot checkpoint (see
        // `pdf-ffi`'s `document::checkpoint`); then a save is the way out.
        if matches!(command, Command::RemoveTextRun(_)) {
            return Err(RecordError::Refused(
                "This inserted text is not saved yet — save the document before deleting it."
                    .to_string(),
            ));
        }
        return Err(RecordError::Refused(
            "this pending insertion cannot be edited with that text target".to_string(),
        ));
    }
    if matches!(
        command,
        Command::ReplaceTextRunContent { .. } | Command::ReplaceTextRunWithInsertedFont { .. }
    ) && document.pending_edits.entries().iter().any(|queued| {
        matches!(queued, Command::MoveTextRun { item: target, .. }
                if target.id == item.id && target.page == item.page)
    }) {
        return Err(RecordError::Refused(
            "This text was moved and not yet saved — save and reopen before retyping it."
                .to_string(),
        ));
    }
    Ok(document.pending_edits.entries().iter().position(|queued| {
        matches!(
            queued,
            Command::ReplaceTextRunContent { item: queued_item, .. }
                | Command::ReplaceTextRunWithInsertedFont { item: queued_item, .. }
                if queued_item.id == item.id && queued_item.page == item.page
        )
    }))
}

/// Resolves synthetic targets without letting a reused log slot redirect an edit.
fn pending_insertion_amendment_index(
    document: &Document,
    item: &TextRun,
) -> Result<Option<usize>, RecordError> {
    let index = pdf_edit::pending_log_index(item.id);
    // Text and measured width can differ during typing, but page, resource and
    // placement identify the insertion that the caller originally targeted.
    if let Some(index) = index {
        if matches!(document.pending_edits.entries().get(index),
            Some(Command::InsertTextRun(run))
                if run.page == item.page
                    && run.resource_font_name == item.resource_font_name
                    && run.font_kind == item.font_kind
                    && run.bbox.x == item.bbox.x && run.bbox.y == item.bbox.y
                    && run.bbox.height == item.bbox.height)
        {
            return Ok(Some(index));
        }
    }
    Err(RecordError::Refused(
        "this pending insertion cannot be edited with that text target".to_string(),
    ))
}

/// A path edit folded into the queued geometry edit of the same path, keyed
/// to the snapshot the file still holds.
///
/// The caller's item is the path at its pending box, so the destination is
/// read off it: a move keeps that box's size. Two moves stay a move; any
/// resize in the pair makes the fold a resize from the original box straight
/// to the final one, which a move is a special case of. A removal simply
/// replaces the queued edit. Successive edits coalesce into one undo step —
/// the cost `EditLog::amend` documents.
fn with_queued_path(queued: &Command, command: &Command) -> Option<Command> {
    let (Command::MovePath { item: original, .. } | Command::ResizePath { item: original, .. }) =
        queued
    else {
        return None;
    };
    let original = original.clone();
    Some(match command {
        Command::RemovePath(_) => Command::RemovePath(original),
        Command::MovePath { item, to } => {
            let to = pdf_document::annotation::Rect {
                x: to.x,
                y: to.y,
                ..item.bbox
            };
            if matches!(queued, Command::MovePath { .. }) {
                Command::MovePath { item: original, to }
            } else {
                Command::ResizePath { item: original, to }
            }
        }
        Command::ResizePath { to, .. } => Command::ResizePath {
            item: original,
            to: *to,
        },
        _ => return None,
    })
}

/// An image edit folded into the queued geometry edit of the same image,
/// keyed to the snapshot the file still holds — the image twin of
/// [`with_queued_path`].
///
/// A move or a resize carries its whole destination box, so the fold takes
/// it as is. Two moves stay a move; any resize in the pair makes the fold a
/// resize, which a move is a special case of. A removal replaces the queued
/// edit outright, painting operation and all.
fn with_queued_image(queued: &Command, command: &Command) -> Option<Command> {
    let (Command::MoveImage { item: original, .. } | Command::ResizeImage { item: original, .. }) =
        queued
    else {
        return None;
    };
    let item = original.clone();
    Some(match command {
        Command::RemoveImage { source, .. } => Command::RemoveImage {
            item,
            source: source.clone(),
        },
        Command::MoveImage { to, .. } if matches!(queued, Command::MoveImage { .. }) => {
            Command::MoveImage { item, to: *to }
        }
        Command::MoveImage { to, .. } | Command::ResizeImage { to, .. } => {
            Command::ResizeImage { item, to: *to }
        }
        _ => return None,
    })
}

/// `command` carrying `queued`'s run snapshot in place of its own.
///
/// Only called for a pair [`pending_text_amendment_index`] matched: retyping
/// an insertion, replacing/removing a run with a queued replacement, or moving
/// a run or an image with a queued move.
fn with_queued_item(queued: &Command, command: Command) -> Command {
    if let Some(folded) = with_queued_path(queued, &command) {
        return folded;
    }
    if let Some(folded) = with_queued_image(queued, &command) {
        return folded;
    }
    if let (Command::InsertTextRun(run), Command::MoveTextRun { to, .. }) = (queued, &command) {
        return Command::InsertTextRun(TextRun {
            bbox: pdf_document::annotation::Rect {
                x: to.x,
                y: to.y,
                ..run.bbox
            },
            ..run.clone()
        });
    }
    if let (Command::MoveTextRun { item, .. }, Command::MoveTextRun { to, .. }) = (queued, &command)
    {
        return Command::MoveTextRun {
            item: item.clone(),
            to: *to,
        };
    }
    if let (Command::InsertTextRun(run), Command::ReplaceTextRunContent { after, .. }) =
        (queued, &command)
    {
        return Command::InsertTextRun(TextRun {
            text: after.clone(),
            ..run.clone()
        });
    }
    let original = match queued {
        Command::ReplaceTextRunContent { item, .. }
        | Command::ReplaceTextRunWithInsertedFont { item, .. } => item.clone(),
        _ => return command,
    };
    match command {
        Command::ReplaceTextRunContent { after, .. } => Command::ReplaceTextRunContent {
            item: original,
            after,
        },
        Command::ReplaceTextRunWithInsertedFont { after, .. } => {
            Command::ReplaceTextRunWithInsertedFont {
                item: original,
                after,
            }
        }
        Command::RemoveTextRun(_) => Command::RemoveTextRun(original),
        other => other,
    }
}

/// What a page-content command edits: the kind of item and which one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Text(ContentItemId, PageId),
    Image(ContentItemId, PageId),
    Path(ContentItemId, PageId),
}

pub fn target(command: &Command) -> Option<Target> {
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
) -> Result<Option<(usize, Command)>, RecordError> {
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
    if let Some(folded) = with_original_geometry(next, earlier) {
        return Some(folded);
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

/// The image and path half of [`with_original`], for a selection moved or
/// removed again (T-208): the queued original with the new destination, a
/// resize staying a resize, or the item removed outright.
fn with_original_geometry(next: &Command, earlier: &Command) -> Option<Command> {
    Some(match (next, earlier) {
        (
            Command::RemoveImage { source, .. },
            Command::MoveImage { item, .. }
            | Command::ResizeImage { item, .. }
            | Command::RemoveImage { item, .. },
        ) => Command::RemoveImage {
            item: item.clone(),
            source: source.clone(),
        },
        (Command::MoveImage { to, .. }, Command::MoveImage { item, .. }) => Command::MoveImage {
            item: item.clone(),
            to: *to,
        },
        (Command::MoveImage { to, .. }, Command::ResizeImage { item, .. }) => {
            Command::ResizeImage {
                item: item.clone(),
                to: *to,
            }
        }
        (
            Command::RemovePath(_),
            Command::MovePath { item, .. }
            | Command::ResizePath { item, .. }
            | Command::RemovePath(item),
        ) => Command::RemovePath(item.clone()),
        (Command::MovePath { to, .. }, Command::MovePath { item, .. }) => Command::MovePath {
            item: item.clone(),
            to: *to,
        },
        (Command::MovePath { to, .. }, Command::ResizePath { item, .. }) => Command::ResizePath {
            item: item.clone(),
            to: *to,
        },
        _ => return None,
    })
}

/// Where a single content edit goes when its item sits inside a queued
/// batch: folded into that batch's child for the same item, when it is the
/// same kind of edit — the first run of a retyped block retyped again, one
/// run of a moved block moved on its own — or refused.
///
/// [`record_content_command`] folds a repeated edit into the queued *single* command; it
/// cannot otherwise reach inside a batch, and without this the edit would
/// fail the base-bytes validation with a reason that says nothing useful.
/// `None` when no batch holds the item.
fn fold_into_queued_batch(
    document: &Document,
    command: &Command,
) -> Result<Option<(usize, Command)>, RecordError> {
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

fn save_first() -> RecordError {
    unsupported("This text has an unsaved edit — save the document before editing it again.")
}

fn unsupported(detail: &str) -> RecordError {
    RecordError::Refused(detail.to_string())
}
