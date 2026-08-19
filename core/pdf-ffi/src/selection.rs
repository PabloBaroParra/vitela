//! `FfiPageCharacters` (T-061 DELTA): a UniFFI interface object wrapping
//! `pdf_render::PageCharacters` — the caret hit-testing and line-union
//! geometry a drag-select needs on every pointer-move.
//!
//! The GTK shell links `pdf-render` directly and calls `PageCharacters`
//! in-process (see its `app/selection.rs` and T-046's module docs, which
//! warn every later shell off reimplementing this). Windows cannot: its
//! architecture contract (`apps/windows/CONTRACT.md`) keeps all
//! document/business decisions in Rust and routes every call through the
//! facade, so the same geometry needs a crossing here instead of a second,
//! drifting copy of the hit-test math in C#. An interface object, not a
//! record, because `PageCharacters` is built once per page and queried on
//! every pointer-move of a drag — the same lifecycle `BitmapHandle` already
//! uses this pattern for.

use crate::types::FfiTextRect;

/// One page's characters, flattened for repeated caret/selection queries.
/// Obtained via `DocumentHandle::page_characters` and held by the shell for
/// the life of a drag-select.
#[derive(uniffi::Object)]
pub struct FfiPageCharacters {
    inner: pdf_render::PageCharacters,
}

impl FfiPageCharacters {
    pub(crate) fn new(inner: pdf_render::PageCharacters) -> Self {
        FfiPageCharacters { inner }
    }
}

#[uniffi::export]
impl FfiPageCharacters {
    /// Character count on the page.
    pub fn len(&self) -> u32 {
        self.inner.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// The caret nearest a PDF-space point (bottom-left origin), or `None`
    /// on a page with no positioned text.
    pub fn caret_at(&self, x_pt: f32, y_pt: f32) -> Option<u32> {
        self.inner.caret_at(x_pt, y_pt).map(|caret| caret as u32)
    }

    /// The text between two carets, for the clipboard. `anchor`/`focus`
    /// need not be ordered — a drag started rightward or leftward reports
    /// the same text either way, matching `PageCharacters::text_in`.
    pub fn text_in(&self, anchor: u32, focus: u32) -> String {
        self.inner
            .text_in(pdf_render::caret_range(anchor as usize, focus as usize))
    }

    /// The rects a shell paints between two carets: one per visual line, not
    /// one per glyph.
    pub fn rects_in(&self, anchor: u32, focus: u32) -> Vec<FfiTextRect> {
        self.inner
            .rects_in(pdf_render::caret_range(anchor as usize, focus as usize))
            .into_iter()
            .map(Into::into)
            .collect()
    }
}
