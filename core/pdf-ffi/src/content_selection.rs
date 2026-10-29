//! Multi-item selections through the FFI (B25 P3, T-208): what a marquee or
//! SHIFT+click gathers on one page — text runs, images, vector paths —
//! moved or removed as one undo step.
//!
//! The batches are built by `pdf_edit::selection`; they are recorded through
//! the same gate and fold as a text block (`text_blocks::apply_batch_edit`),
//! so a selection dragged twice before a save is still one undo step.

use pdf_edit::Selection;

use crate::document::DocumentHandle;
use crate::error::FfiError;
use crate::text_blocks::apply_batch_edit;
use crate::types::{FfiContentImageItem, FfiContentPathItem, FfiContentTextRun};

/// The members of a selection, each exactly as `read_page_content` and
/// `read_page_paths` read it. A text block is sent as its runs.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct FfiContentSelection {
    pub runs: Vec<FfiContentTextRun>,
    pub images: Vec<FfiContentImageItem>,
    pub paths: Vec<FfiContentPathItem>,
}

/// Moves every member of `selection` by `(dx, dy)` points, as one undo step.
#[uniffi::export]
pub fn move_content_selection(
    handle: &DocumentHandle,
    selection: FfiContentSelection,
    dx: f64,
    dy: f64,
) -> Result<(), FfiError> {
    if !dx.is_finite() || !dy.is_finite() {
        return Err(unsupported("Selection offsets must be finite."));
    }
    apply_selection_edit(handle, selection, |selection| {
        pdf_edit::move_selection(selection, dx, dy)
    })
}

/// Removes every member of `selection`, as one undo step.
#[uniffi::export]
pub fn remove_content_selection(
    handle: &DocumentHandle,
    selection: FfiContentSelection,
) -> Result<(), FfiError> {
    apply_selection_edit(handle, selection, pdf_edit::remove_selection)
}

fn apply_selection_edit(
    handle: &DocumentHandle,
    selection: FfiContentSelection,
    build: impl FnOnce(&Selection) -> Option<pdf_document::Command>,
) -> Result<(), FfiError> {
    apply_batch_edit(handle, |state| {
        let selection = Selection {
            runs: collect(selection.runs, |run| state.text_run(run))?,
            images: collect(selection.images, |image| state.image_item(image))?,
            paths: collect(selection.paths, |path| state.path_item(path))?,
        };
        if !selection.is_on_one_page() {
            return Err(unsupported("A selection edits one page at a time."));
        }
        build(&selection).ok_or_else(|| unsupported("The selection is empty."))
    })
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
