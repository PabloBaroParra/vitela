//! The output snapshot: the document as a save would write it right now,
//! reopened as a throwaway handle for printing or exporting from.
//!
//! The handle a shell renders from never carries the session's annotations:
//! [`crate::refresh_preview`] leaves them out on purpose, because every shell
//! draws them as its own overlay. Print and export have no overlay pass —
//! they rasterize straight to paper or to a file — so rendering the live
//! handle silently drops every annotation the reader added. A snapshot is
//! the bytes a real save would write, annotations included, opened again.
//!
//! Here in the core, not in each shell, because reopening those bytes needs
//! the password the session was opened with. The core already keeps it for
//! [`crate::refresh_preview`]; a shell that rebuilt the snapshot itself had
//! to keep a second copy of it, or refuse encrypted documents outright.

use std::sync::Arc;

use crate::document::{open_from_bytes, DocumentHandle};
use crate::error::FfiError;
use crate::types::{FfiSaveIntent, FfiSignatureAcknowledgement};

/// Saves `handle` in memory and reopens the bytes under the session's own
/// password, for a shell to [`crate::render_page`] or
/// [`crate::export_page_image`] from. The live handle is not changed.
///
/// Signatures are acknowledged silently: the snapshot is never written
/// anywhere, and a real save still returns
/// [`FfiError::SignaturesWouldBeInvalidated`] until the reader agrees.
///
/// The snapshot grants what the session grants, because it is opened with
/// the same credential — and its own annotation list starts empty: what the
/// session added is part of its pages now, not a second editable copy.
///
/// # Errors
///
/// Whatever a save of `handle` returns — notably
/// [`FfiError::InvalidSaveRequest`] for a page-structure edit on an
/// encrypted document opened with only one of its passwords — and whatever
/// reopening the bytes returns.
#[uniffi::export]
pub fn output_snapshot(handle: &DocumentHandle) -> Result<Arc<DocumentHandle>, FfiError> {
    let (bytes, password) = {
        let state = handle.lock();
        let bytes = state.with_save_input(
            FfiSaveIntent::Default,
            FfiSignatureAcknowledgement::ProceedAndInvalidate,
            pdf_save::save_document,
        )?;
        (bytes, state.render_password().map(str::to_owned))
    };
    open_from_bytes(bytes, password)
}
