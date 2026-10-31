//! The content clipboard's queue rules (B25 P4, T-209): copying a selection
//! out of a page, pasting a clip onto one, and what a selection edit does
//! when it holds pasted items that are not saved yet.
//!
//! The clip itself is built and written by `pdf_edit::clipboard`. What lives
//! here is everything that depends on the pending queue — which is why it
//! moved out of `pdf-ffi`: a shell that calls the core directly
//! (`apps/linux-gtk`) needs the same rules. Permission gates stay with the
//! callers, as they do for [`crate::record`].
//!
//! What is pasted stays one pending entry until the save. Dragging all of it
//! again before a save moves the paste itself, one undo step; editing part
//! of it is refused until the save writes it, because only then do its items
//! have names the page can resolve.

use pdf_document::{Command, ContentClip, ContentItemId, Document, PageId, Rect};
use pdf_edit::Selection;
use pdf_manip::LopdfDocument;

use crate::bridge::ImportedSources;
use crate::content::validate_content_command;
use crate::origin::{page_backing, PageBacking};
use crate::record::{target, RecordError, Target};

/// Copies every member of `selection` into a clip, leaving the page as it
/// is. Members with an unsaved edit are refused: the clip is copied from the
/// file, and the file does not hold that edit yet.
pub fn copy_selection_content(
    document: &Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    selection: &Selection,
) -> Result<ContentClip, RecordError> {
    if !selection.is_on_one_page() {
        return Err(refused("A selection is copied from one page at a time."));
    }
    let Some(page) = members(selection).next().map(|(_, page)| page) else {
        return Err(refused("The selection is empty."));
    };
    if members(selection).any(|(member, _)| is_edited(document, member)) {
        return Err(refused(
            "Save the document before copying content you have edited.",
        ));
    }
    match page_backing(document, page, base, sources)? {
        PageBacking::Object { document, object } => {
            pdf_edit::copy_selection(document.as_lopdf(), object, selection)
                .map_err(|error| RecordError::Invalid(error.into()))
        }
        PageBacking::Empty => Err(refused("The page has no saved content to copy.")),
    }
}

/// Writes `clip` onto `page`, its box's top-left corner at `(left, top)` in
/// page points — where the cursor is — as one undo step.
pub fn record_paste(
    document: &mut Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    page: PageId,
    clip: ContentClip,
    left: f64,
    top: f64,
) -> Result<(), RecordError> {
    if !left.is_finite() || !top.is_finite() {
        return Err(refused("Paste coordinates must be finite."));
    }
    let command = Command::PasteContent {
        page,
        to: Rect {
            x: left,
            y: top - clip.bbox.height,
            ..clip.bbox
        },
        clip,
    };
    // A paste appends after everything on the page and names nothing a
    // pending edit can have moved, so the untouched page is what it is
    // checked against — the probe still writes it, renames and all.
    validate_content_command(document, base, sources, &command)?;
    let mut log = std::mem::take(&mut document.pending_edits);
    let applied = log.apply(document, command);
    document.pending_edits = log;
    if applied {
        Ok(())
    } else {
        Err(refused("the paste was rejected against the open document"))
    }
}

/// What a selection edit does when its members include pasted items that are
/// not saved yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PastedMembers {
    /// No member came from a pending paste.
    None,
    /// Every item one pending paste added, and nothing else — at this entry.
    Whole(usize),
}

/// Sorts a selection's member `ids` against the pending pastes: none of
/// them, all of one, or a refusal for anything in between.
pub fn pasted_members(
    document: &Document,
    ids: &[ContentItemId],
) -> Result<PastedMembers, RecordError> {
    let entries = document.pending_edits.entries();
    let paste_of = |id: ContentItemId| {
        pdf_edit::pending_log_index(id).and_then(|index| {
            matches!(entries.get(index), Some(Command::PasteContent { .. })).then_some(index)
        })
    };
    let pasted: Vec<Option<usize>> = ids.iter().map(|&id| paste_of(id)).collect();
    if pasted.iter().all(Option::is_none) {
        return Ok(PastedMembers::None);
    }
    let Some(Some(index)) = pasted.first().copied() else {
        return Err(part_of_paste());
    };
    if pasted.iter().any(|entry| *entry != Some(index)) {
        return Err(part_of_paste());
    }
    let Some(Command::PasteContent { page, clip, to }) = entries.get(index) else {
        return Err(part_of_paste());
    };
    let painted = pdf_edit::pasted_content(clip, *to, *page)
        .map_err(|error| RecordError::Invalid(error.into()))?;
    let count = painted.text_runs.len() + painted.images.len() + painted.paths.len();
    let mut distinct = ids.to_vec();
    distinct.sort_unstable_by_key(|id| id.0);
    distinct.dedup();
    if distinct.len() != count {
        return Err(part_of_paste());
    }
    Ok(PastedMembers::Whole(index))
}

/// The ids of every member of `selection`, for [`pasted_members`].
pub fn selection_ids(selection: &Selection) -> Vec<ContentItemId> {
    members(selection)
        .map(|(member, _)| match member {
            Target::Text(id, _) | Target::Image(id, _) | Target::Path(id, _) => id,
        })
        .collect()
}

/// Moves the pending paste at `index` by `(dx, dy)`, in place: the pair is
/// one undo step, like a selection dragged twice.
pub fn move_pending_paste(
    document: &mut Document,
    index: usize,
    dx: f64,
    dy: f64,
) -> Result<(), RecordError> {
    let Some(Command::PasteContent { page, clip, to }) =
        document.pending_edits.entries().get(index).cloned()
    else {
        return Err(part_of_paste());
    };
    let moved = Command::PasteContent {
        page,
        clip,
        to: Rect {
            x: to.x + dx,
            y: to.y + dy,
            ..to
        },
    };
    if !document.pending_edits.amend(index, moved) {
        return Err(refused("the pending paste is no longer available"));
    }
    Ok(())
}

/// The refusal for editing part of a pending paste.
pub fn part_of_paste() -> RecordError {
    refused(
        "Save the document before editing part of what you pasted — or select all of it to move it.",
    )
}

/// The refusal for deleting a pending paste: undo takes it back, and a
/// removal of it would have nothing in the file to resolve.
pub fn paste_not_deletable() -> RecordError {
    refused("Undo the paste to take it back, or save the document before deleting it.")
}

/// Every member as the edit target it would be, with its page.
fn members(selection: &Selection) -> impl Iterator<Item = (Target, PageId)> + '_ {
    let runs = selection
        .runs
        .iter()
        .map(|run| (Target::Text(run.id, run.page), run.page));
    let images = selection
        .images
        .iter()
        .map(|image| (Target::Image(image.id, image.page), image.page));
    let paths = selection
        .paths
        .iter()
        .map(|path| (Target::Path(path.id, path.page), path.page));
    runs.chain(images).chain(paths)
}

/// Whether a pending edit already changed `member`, or it exists only because
/// of one (a synthetic id).
fn is_edited(document: &Document, member: Target) -> bool {
    let id = match member {
        Target::Text(id, _) | Target::Image(id, _) | Target::Path(id, _) => id,
    };
    if pdf_edit::pending_log_index(id).is_some() {
        return true;
    }
    document.pending_edits.entries().iter().any(|entry| {
        let children = match entry {
            Command::Batch(children) => children.as_slice(),
            single => std::slice::from_ref(single),
        };
        children
            .iter()
            .filter_map(target)
            .any(|queued| queued == member)
    })
}

fn refused(detail: &str) -> RecordError {
    RecordError::Refused(detail.to_string())
}
