//! `BitmapHandle` (T-039): a UniFFI *interface object* (Arc-based reference
//! type), never a record with an inline `Vec<u8>` field — see design.md's
//! "Bitmap Handle Lifecycle" correction: a record crosses the FFI boundary
//! by value, so an inline pixel buffer would be copied on every single
//! crossing (including just checking width/height). As an interface object,
//! the handle itself is a lightweight reference; the pixel buffer stays
//! Rust-side and is copied only when `get_pixels()` is explicitly called.
//!
//! Wraps `pdf_render::BitmapHandle` (which already implements the
//! release-on-drop registry contract, T-019) rather than reimplementing a
//! second registry — dropping the last `Arc` reference to this object drops
//! the wrapped `pdf_render::BitmapHandle`, which releases its registry entry
//! (spec "Bitmap Handle Lifecycle": "render → read → drop_bitmap() →
//! further access is attempted THEN access returns an error, no memory
//! leak").

use crate::error::FfiError;

/// Opaque, owning handle to a rendered page bitmap (RGBA8, row-major).
#[derive(uniffi::Object)]
pub struct BitmapHandle {
    inner: pdf_render::BitmapHandle,
}

impl BitmapHandle {
    pub(crate) fn new(inner: pdf_render::BitmapHandle) -> Self {
        BitmapHandle { inner }
    }
}

#[uniffi::export]
impl BitmapHandle {
    pub fn width(&self) -> Result<u32, FfiError> {
        self.inner.width().map_err(Into::into)
    }

    pub fn height(&self) -> Result<u32, FfiError> {
        self.inner.height().map_err(Into::into)
    }

    pub fn stride(&self) -> Result<u32, FfiError> {
        self.inner.stride().map_err(Into::into)
    }

    /// Copies the RGBA pixel buffer out of the registry — the one explicit,
    /// on-demand copy point (design.md's FFI-design rationale); width/
    /// height/stride never touch the pixel buffer.
    pub fn get_pixels(&self) -> Result<Vec<u8>, FfiError> {
        self.inner.get_pixels().map_err(Into::into)
    }
}
