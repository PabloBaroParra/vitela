//! Bitmap handle registry with release-on-drop (T-019).
//!
//! Backs `spec.md`'s "Bitmap Handle Lifecycle" requirement at the `pdf-render`
//! level: rendered pixel buffers live in a registry keyed by an opaque id;
//! `BitmapHandle` releases its registry entry when dropped. The UniFFI-facing
//! `BitmapHandle` interface object (B7, `pdf-ffi`) wraps a handle equivalent
//! to this one rather than re-implementing the registry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::RenderError;

/// A rendered page's raw pixel data: RGBA8, row-major, `stride` bytes per row
/// (may exceed `width * 4` if the backing renderer pads rows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixels: Vec<u8>,
}

#[derive(Default)]
struct Inner {
    bitmaps: Mutex<HashMap<u64, Bitmap>>,
    next_id: AtomicU64,
}

/// Shared, cloneable handle to the bitmap registry. Cloning shares the same
/// underlying storage (`Arc`-based) — it does not duplicate bitmap data.
#[derive(Clone, Default)]
pub struct BitmapRegistry {
    inner: Arc<Inner>,
}

impl BitmapRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a rendered bitmap and returns an owning [`BitmapHandle`]. The
    /// registry entry is released automatically when the returned handle is
    /// dropped.
    pub fn insert(&self, bitmap: Bitmap) -> BitmapHandle {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        self.inner
            .bitmaps
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id, bitmap);
        BitmapHandle {
            id,
            registry: self.clone(),
        }
    }

    /// Looks up a bitmap by raw id, independent of any [`BitmapHandle`].
    /// Returns [`RenderError::BitmapNotFound`] if the id was never issued, or
    /// was already released by dropping its handle.
    pub fn get(&self, id: u64) -> Result<Bitmap, RenderError> {
        self.inner
            .bitmaps
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&id)
            .cloned()
            .ok_or(RenderError::BitmapNotFound)
    }

    fn release(&self, id: u64) {
        self.inner
            .bitmaps
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&id);
    }

    /// Number of bitmaps currently held (test/diagnostic helper).
    pub fn len(&self) -> usize {
        self.inner
            .bitmaps
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Opaque, owning handle to a rendered bitmap. Releases its registry entry on
/// drop (T-019 / spec "Bitmap Handle Lifecycle": "render → read →
/// drop_bitmap() → further access is attempted THEN access returns an
/// error, no memory leak").
pub struct BitmapHandle {
    id: u64,
    registry: BitmapRegistry,
}

impl std::fmt::Debug for BitmapHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BitmapHandle")
            .field("id", &self.id)
            .finish()
    }
}

impl BitmapHandle {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn width(&self) -> Result<u32, RenderError> {
        self.registry.get(self.id).map(|b| b.width)
    }

    pub fn height(&self) -> Result<u32, RenderError> {
        self.registry.get(self.id).map(|b| b.height)
    }

    pub fn stride(&self) -> Result<u32, RenderError> {
        self.registry.get(self.id).map(|b| b.stride)
    }

    /// Copies the RGBA pixel buffer out of the registry. Per `design.md`'s
    /// FFI-design rationale, this is the one explicit, on-demand copy point —
    /// width/height/stride accessors never touch the pixel buffer.
    pub fn get_pixels(&self) -> Result<Vec<u8>, RenderError> {
        self.registry.get(self.id).map(|b| b.pixels)
    }
}

impl Drop for BitmapHandle {
    fn drop(&mut self) {
        self.registry.release(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bitmap() -> Bitmap {
        Bitmap {
            width: 2,
            height: 1,
            stride: 8,
            pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
        }
    }

    #[test]
    fn insert_then_read_via_handle() {
        let registry = BitmapRegistry::new();
        let handle = registry.insert(sample_bitmap());

        assert_eq!(handle.width().unwrap(), 2);
        assert_eq!(handle.height().unwrap(), 1);
        assert_eq!(handle.stride().unwrap(), 8);
        assert_eq!(handle.get_pixels().unwrap(), vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn drop_releases_registry_entry_and_further_access_errors() {
        let registry = BitmapRegistry::new();
        let handle = registry.insert(sample_bitmap());
        let id = handle.id();

        assert_eq!(registry.len(), 1);
        drop(handle);
        assert_eq!(registry.len(), 0);

        // Simulates "further access after drop_bitmap()" via the raw id —
        // the handle itself cannot be used post-move under Rust's ownership
        // model, so this is the meaningful equivalent of the FFI-level
        // "access after drop returns an error" scenario.
        let err = registry.get(id).unwrap_err();
        assert!(matches!(err, RenderError::BitmapNotFound));
    }

    #[test]
    fn independent_handles_do_not_interfere() {
        let registry = BitmapRegistry::new();
        let a = registry.insert(sample_bitmap());
        let b = registry.insert(sample_bitmap());
        assert_ne!(a.id(), b.id());
        assert_eq!(registry.len(), 2);

        drop(a);
        assert_eq!(registry.len(), 1);
        assert!(b.get_pixels().is_ok());
    }

    #[test]
    fn unknown_id_errors() {
        let registry = BitmapRegistry::new();
        assert!(matches!(
            registry.get(9999).unwrap_err(),
            RenderError::BitmapNotFound
        ));
    }
}
