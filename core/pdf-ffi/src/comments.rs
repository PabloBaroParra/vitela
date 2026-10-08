//! Read-only comment vocabulary, separate from editable session annotations.

use crate::types::FfiRect;

/// A saved comment or a pending session note, addressed by current page position.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiComment {
    pub page: u32,
    pub rect: FfiRect,
    pub contents: String,
    pub author: Option<String>,
    pub date: Option<String>,
    /// Only pending notes have an editable session annotation id.
    pub annotation_id: Option<u64>,
}
