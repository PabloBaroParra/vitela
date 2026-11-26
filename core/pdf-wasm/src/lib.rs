//! `pdf-wasm`: the web shell's boundary to the Rust core (Batch 26, T-219).
//!
//! What `pdf-ffi` is to the native shells, this crate is to `apps/web`:
//! wasm-bindgen instead of UniFFI (UniFFI has no wasm target), and the same
//! core rules underneath — opening with credentials, page sizes and the text
//! permission come from `pdf-manip`, pixels and text from `pdf-render`'s
//! synchronous wasm32 renderer.
//!
//! ## Layout
//!
//! - [`error`], [`model`], [`convert`] and [`placement`] are plain Rust and
//!   build on every target, so the workspace's native `cargo test` covers
//!   them. `model` holds the edit session too (page edits, undo/redo, save,
//!   the preview snapshot, import and the Documents view's blocks, and the
//!   session's annotations and comments, the form fields and filling them
//!   in, the compressed save, image export, document properties and
//!   watermarks).
//! - `engine`, `dto` and `geometry` are the `#[wasm_bindgen]` surface and
//!   exist only on wasm32; they are tested by `wasm-bindgen-test` in a
//!   headless browser (`tests/engine.rs`).
//!
//! ## Loading
//!
//! pdfium is a separate Emscripten module. JavaScript must load it, apply
//! the `_PDFium_Init` alias, and call `initialize_pdfium_render(pdfium,
//! thisModuleExports, debug)` — exported from this module by pdfium-render —
//! before constructing an `Engine`. See
//! `packages/vitela-core/src/worker/pdfium.ts`.

pub mod convert;
pub mod error;
pub mod model;
pub mod placement;

#[cfg(target_arch = "wasm32")]
mod dto;
#[cfg(target_arch = "wasm32")]
mod engine;
#[cfg(target_arch = "wasm32")]
mod geometry;

#[cfg(target_arch = "wasm32")]
pub use dto::{
    AnnotationEntry, CommentEntry, CompressRefusalEntry, CompressedSave, DocumentBlock,
    DocumentPropertiesEntry, FormFieldEntry, FreeTextLines, ImportedPages, OpenedImageExport,
    PageCharacters, PageDimensions, PreparedImport, RenderedPage, SearchMatch, SplitPart, TextRun,
    WatermarkLook,
};
#[cfg(target_arch = "wasm32")]
pub use engine::{
    compress_presets, free_text_layout, image_export_formats, page_image_file_name,
    parse_page_selection, plan_split, stamp_placement, watermark_defaults, EditCommand, Engine,
};
#[cfg(target_arch = "wasm32")]
pub use geometry::{place_rects, point_to_pdf};
