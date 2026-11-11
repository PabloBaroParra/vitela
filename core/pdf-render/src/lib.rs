//! `pdf-render`: single-threaded pdfium actor with a priority queue and
//! cancel-at-dequeue semantics (Batch 3, T-015..T-021).
//!
//! See `design.md` "Threading — pdfium single-actor model" and `spec.md`
//! "Serialized pdfium Access" / "Dark-Mode Render Option" / "Text-Run Data
//! Exposure" / "Bitmap Handle Lifecycle" / "Large-File Performance".
//!
//! `PdfiumRenderer` exposes the full rendering contract directly. The Batch 0
//! port trait was removed before B7 because its placeholder signature did not
//! match `RenderOptions` or region rendering.
//!
//! ## pdfium binary sourcing
//!
//! The pdfium dynamic library is never committed to this repository (see
//! `vendor/pdfium/README.md` and the root `.gitignore`). Production shells
//! set `PDFIUM_DYNAMIC_LIB_PATH` to their bundled copy; local dev/test
//! resolves `vendor/pdfium/bin/` first (see `library.rs`).
//!
//! ## Targets
//!
//! Native builds run every pdfium call on one actor thread
//! (`PdfiumRenderer`). `wasm32-unknown-unknown` has no threads, no
//! filesystem and no dynamic loader, so the actor, `library.rs` and
//! path-based opens are not built there; `SyncRenderer` instead runs the
//! same work (the private `jobs` module) synchronously on the calling
//! thread, against a pdfium Emscripten module bound from JavaScript.

#[cfg(not(target_arch = "wasm32"))]
pub mod actor;
pub mod bitmap;
pub mod error;
pub mod inversion;
mod jobs;
#[cfg(not(target_arch = "wasm32"))]
pub mod library;
pub mod options;
#[cfg(not(target_arch = "wasm32"))]
pub mod renderer;
pub mod selection;
pub mod state;
#[cfg(target_arch = "wasm32")]
pub mod sync_renderer;
pub mod text;

#[cfg(not(target_arch = "wasm32"))]
pub use actor::CancellationHandle;
pub use bitmap::{Bitmap, BitmapHandle, BitmapRegistry};
pub use error::RenderError;
pub use jobs::full_page_raster_fits;
pub use options::{Priority, Rect, RenderOptions, Tile};
#[cfg(not(target_arch = "wasm32"))]
pub use renderer::{PdfiumActor, PdfiumRenderer, RenderHandle};
pub use selection::{
    caret_range, line_rects, place_point, place_rect, point_to_pdf, Caret, PageCharacters,
    PageGeometry, PagePlacement, PageRotation, PlacedRect,
};
pub use state::DocHandle as DocumentHandle;
#[cfg(target_arch = "wasm32")]
pub use sync_renderer::SyncRenderer;
pub use text::{TextMatch, TextRect, TextRun};
