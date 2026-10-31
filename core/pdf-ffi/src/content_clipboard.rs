//! The content clipboard through the FFI (B25 P4, T-209): CTRL+C, CTRL+X and
//! CTRL+V over what a marquee or SHIFT+click selected.
//!
//! The clip is built and written by `pdf_edit::clipboard`, and the queue rules
//! around it are `pdf_save::clipboard`'s; it travels as an
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

use pdf_document::{ContentClip, ContentItemId};
use pdf_edit::Selection;
use pdf_save::PastedMembers;

use crate::content_selection::{remove_content_selection, FfiContentSelection};
use crate::document::{
    content_editing_is_allowed, text_extraction_is_allowed, DocumentHandle, DocumentState,
};
use crate::error::FfiError;
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
    let page = state.page_id(page)?;
    state.record(|document, base, sources| {
        pdf_save::record_paste(document, base, sources, page, clip.into(), left, top)
    })
}

/// Sorts `selection`'s members against the pending pastes —
/// `pdf_save::pasted_members` over the FFI's item ids.
pub(crate) fn pasted_members(
    state: &DocumentState,
    selection: &FfiContentSelection,
) -> Result<PastedMembers, FfiError> {
    let ids: Vec<ContentItemId> = selection
        .runs
        .iter()
        .map(|run| run.id)
        .chain(selection.images.iter().map(|image| image.id))
        .chain(selection.paths.iter().map(|path| path.id))
        .map(ContentItemId)
        .collect();
    pdf_save::pasted_members(state.document(), &ids).map_err(Into::into)
}

/// Moves the pending paste at `index` by `(dx, dy)`, in place.
pub(crate) fn move_pending_paste(
    state: &mut DocumentState,
    index: usize,
    dx: f64,
    dy: f64,
) -> Result<(), FfiError> {
    pdf_save::move_pending_paste(state.document_mut(), index, dx, dy).map_err(Into::into)
}

fn copy(state: &DocumentState, selection: FfiContentSelection) -> Result<ContentClip, FfiError> {
    let selection = Selection {
        runs: collect(selection.runs, |run| state.text_run(run))?,
        images: collect(selection.images, |image| state.image_item(image))?,
        paths: collect(selection.paths, |path| state.path_item(path))?,
    };
    state
        .with_imported_sources(|sources| {
            pdf_save::copy_selection_content(state.document(), state.base(), sources, &selection)
        })
        .map_err(Into::into)
}

fn collect<T, U>(
    items: Vec<T>,
    convert: impl Fn(T) -> Result<U, FfiError>,
) -> Result<Vec<U>, FfiError> {
    items.into_iter().map(convert).collect()
}

fn unsupported(detail: &str) -> FfiError {
    FfiError::UnsupportedOperation {
        detail: detail.to_string(),
    }
}
