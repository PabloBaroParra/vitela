//! The content clipboard through the FFI (B25 P4, T-209): CTRL+C, CTRL+X and
//! CTRL+V over what a marquee or SHIFT+click selected.
//!
//! The clip is built and written by `pdf_edit::clipboard`; it travels as an
//! [`FfiContentClip`] the shell holds on to — a one-page PDF and its box — so
//! the clipboard outlives the page, and later the document, it came from.
//! Nothing here keeps it: the core has no clipboard state to get out of step
//! with what the shell shows.
//!
//! What is pasted stays one pending entry until the save, and its items are
//! hit-testable through the pending overlay. Dragging all of them again
//! before a save moves the paste itself (one undo step); editing part of it
//! is refused until the save writes it, because only then do its items have
//! names the page can resolve.

use pdf_document::{Command, ContentClip, ContentItemId, Document, PageId, Rect};
use pdf_edit::Selection;

use crate::content_selection::{remove_content_selection, FfiContentSelection};
use crate::document::{
    apply_command, content_editing_is_allowed, text_extraction_is_allowed, DocumentHandle,
    DocumentState,
};
use crate::error::FfiError;
use crate::text_blocks::{target, Target};
use crate::types::FfiRect;

/// Content copied out of a page: a self-contained one-page PDF, and the box
/// the copied items covered on the page they came from (page points, origin
/// bottom-left). A paste lines that box's top-left corner up with the cursor.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiContentClip {
    pub pdf: Vec<u8>,
    pub bbox: FfiRect,
}

impl From<ContentClip> for FfiContentClip {
    fn from(clip: ContentClip) -> Self {
        Self {
            pdf: clip.pdf,
            bbox: clip.bbox.into(),
        }
    }
}

impl From<FfiContentClip> for ContentClip {
    fn from(clip: FfiContentClip) -> Self {
        Self {
            pdf: clip.pdf,
            bbox: clip.bbox.into(),
        }
    }
}

/// Copies every member of `selection` into a clip, leaving the page as it is.
///
/// Members with an unsaved edit are refused: the clip is copied from the
/// file, and the file does not hold that edit yet.
#[uniffi::export]
pub fn copy_content_selection(
    handle: &DocumentHandle,
    selection: FfiContentSelection,
) -> Result<FfiContentClip, FfiError> {
    let state = handle.lock();
    if !text_extraction_is_allowed(state.document()) {
        return Err(unsupported("copying content is not permitted"));
    }
    copy(&state, selection).map(Into::into)
}

/// Copies `selection` and removes it from the page, as one undo step for the
/// removal — the copy changes nothing to undo.
#[uniffi::export]
pub fn cut_content_selection(
    handle: &DocumentHandle,
    selection: FfiContentSelection,
) -> Result<FfiContentClip, FfiError> {
    let clip = {
        let state = handle.lock();
        if !text_extraction_is_allowed(state.document())
            || !content_editing_is_allowed(state.document())
        {
            return Err(unsupported("cutting content is not permitted"));
        }
        copy(&state, selection.clone())?
    };
    remove_content_selection(handle, selection)?;
    Ok(clip.into())
}

/// Writes `clip` onto `page`, its box's top-left corner at `(left, top)` in
/// page points — where the cursor is — as one undo step.
#[uniffi::export]
pub fn paste_content(
    handle: &DocumentHandle,
    page: u32,
    clip: FfiContentClip,
    left: f64,
    top: f64,
) -> Result<(), FfiError> {
    if !left.is_finite() || !top.is_finite() {
        return Err(unsupported("Paste coordinates must be finite."));
    }
    let mut state = handle.lock();
    if !content_editing_is_allowed(state.document()) {
        return Err(unsupported("content editing is not permitted"));
    }
    let clip = ContentClip::from(clip);
    let command = Command::PasteContent {
        page: state.page_id(page)?,
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
    state.with_imported_sources(|sources| {
        pdf_save::validate_content_command(state.document(), state.base(), sources, &command)
    })?;
    if !apply_command(state.document_mut(), command) {
        return Err(unsupported(
            "the paste was rejected against the open document",
        ));
    }
    Ok(())
}

/// What a selection edit does when its members include pasted items that are
/// not saved yet.
pub(crate) enum PastedMembers {
    /// No member came from a pending paste.
    None,
    /// Every item one pending paste added, and nothing else — at this entry.
    Whole(usize),
}

/// Sorts `selection`'s members against the pending pastes: none of them, all
/// of one, or a refusal for anything in between.
pub(crate) fn pasted_members(
    state: &DocumentState,
    selection: &FfiContentSelection,
) -> Result<PastedMembers, FfiError> {
    let ids: Vec<u64> = selection
        .runs
        .iter()
        .map(|run| run.id)
        .chain(selection.images.iter().map(|image| image.id))
        .chain(selection.paths.iter().map(|path| path.id))
        .collect();
    let entries = state.document().pending_edits.entries();
    let paste_of = |id: u64| {
        pdf_edit::pending_log_index(ContentItemId(id)).and_then(|index| {
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
    let painted = pdf_edit::pasted_content(clip, *to, *page)?;
    let count = painted.text_runs.len() + painted.images.len() + painted.paths.len();
    let mut distinct = ids.clone();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.len() != count {
        return Err(part_of_paste());
    }
    Ok(PastedMembers::Whole(index))
}

/// Moves the pending paste at `index` by `(dx, dy)`, in place: the pair is
/// one undo step, like a selection dragged twice.
pub(crate) fn move_pending_paste(
    state: &mut DocumentState,
    index: usize,
    dx: f64,
    dy: f64,
) -> Result<(), FfiError> {
    let Some(Command::PasteContent { page, clip, to }) =
        state.document().pending_edits.entries().get(index).cloned()
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
    if !state.document_mut().pending_edits.amend(index, moved) {
        return Err(unsupported("the pending paste is no longer available"));
    }
    Ok(())
}

fn copy(state: &DocumentState, selection: FfiContentSelection) -> Result<ContentClip, FfiError> {
    let selection = Selection {
        runs: collect(selection.runs, |run| state.text_run(run))?,
        images: collect(selection.images, |image| state.image_item(image))?,
        paths: collect(selection.paths, |path| state.path_item(path))?,
    };
    if !selection.is_on_one_page() {
        return Err(unsupported(
            "A selection is copied from one page at a time.",
        ));
    }
    let Some(page) = members(&selection).next().map(|(_, page)| page) else {
        return Err(unsupported("The selection is empty."));
    };
    if members(&selection).any(|(member, _)| is_edited(state.document(), member)) {
        return Err(unsupported(
            "Save the document before copying content you have edited.",
        ));
    }
    state.with_imported_sources(|sources| {
        match pdf_save::page_backing(state.document(), page, state.base(), sources)? {
            pdf_save::origin::PageBacking::Object { document, object } => {
                pdf_edit::copy_selection(document.as_lopdf(), object, &selection)
                    .map_err(FfiError::from)
            }
            pdf_save::origin::PageBacking::Empty => {
                Err(unsupported("The page has no saved content to copy."))
            }
        }
    })
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

fn collect<T, U>(
    items: Vec<T>,
    convert: impl Fn(T) -> Result<U, FfiError>,
) -> Result<Vec<U>, FfiError> {
    items.into_iter().map(convert).collect()
}

pub(crate) fn part_of_paste() -> FfiError {
    unsupported(
        "Save the document before editing part of what you pasted — or select all of it to move it.",
    )
}

fn unsupported(detail: &str) -> FfiError {
    FfiError::UnsupportedOperation {
        detail: detail.to_string(),
    }
}
