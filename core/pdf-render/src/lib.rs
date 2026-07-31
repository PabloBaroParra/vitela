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

pub mod actor;
pub mod bitmap;
pub mod error;
pub mod inversion;
pub mod library;
pub mod options;
pub mod renderer;
pub mod selection;
pub mod state;
pub mod text;

pub use actor::CancellationHandle;
pub use bitmap::{Bitmap, BitmapHandle, BitmapRegistry};
pub use error::RenderError;
pub use options::{Priority, Rect, RenderOptions, Tile};
pub use renderer::{DocumentHandle, PdfiumActor, PdfiumRenderer, RenderHandle};
pub use selection::{
    caret_range, line_rects, place_rect, point_to_pdf, Caret, PageCharacters, PlacedRect,
};
pub use text::{TextMatch, TextRect, TextRun};
