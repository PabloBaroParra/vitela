//! `pdf-ffi`: the only crate depending on `uniffi`. Exposes `DocumentHandle`
//! and `BitmapHandle` as UniFFI interface objects (Arc-based reference
//! types, never inline `Vec<u8>` records), FFI commands (open/
//! open_from_bytes/open_with_passwords/open_with_passwords_from_bytes/
//! create_blank_document/render_page/apply_edit/insert_image_stamp/
//! save_to_bytes/save_to_path/undo/redo), and `FfiError`
//! mirroring the core crates' error types. See Batch 7 (T-039..T-043,
//! T-068 DELTA) and `design.md` "FFI Design (pdf-ffi / UniFFI)".
//!
//! ## Module map
//! - [`error`] (T-041): [`FfiError`], the single error type crossing the
//!   boundary, discriminated by variant (never a raw string), mirroring
//!   `pdf_manip`/`pdf_render`/`pdf_save`/`pdf_annotate`'s own error enums.
//! - [`types`] (part of T-040): UniFFI `Record`/`Enum` shapes for the
//!   command surface (`FfiEditCommand`, `FfiRect`, `FfiColor`, ...) and
//!   their conversions to/from the real core types.
//! - [`bitmap`] (T-039): [`BitmapHandle`], wrapping `pdf_render::BitmapHandle`
//!   with release-on-drop lifecycle (spec "Bitmap Handle Lifecycle").
//! - [`document`] (T-039, T-040, T-068 DELTA): [`DocumentHandle`] and every
//!   FFI command function.
//!
//! ## Why this crate builds the FFI surface from real APIs, not port traits
//!
//! The Batch 0 port trait stubs (`RenderPort`/`ManipulationPort`/
//! `SaveStrategy` placeholders in `pdf_document`) were deleted before this
//! batch — their signatures didn't match what `pdf-render`/`pdf-manip`/
//! `pdf-save` actually ended up needing (region/options-aware rendering,
//! lopdf-backed manipulation, the incremental/full-rewrite writer split).
//! This crate calls each core crate's real, already-tested public API
//! directly (`pdf_manip::open_document*`, `pdf_save::{document_from_lopdf,
//! save_document}`, `pdf_render::PdfiumRenderer`, `pdf_annotate`'s builders)
//! rather than reintroducing a trait layer.

mod bitmap;
mod document;
mod error;
mod types;

pub use bitmap::BitmapHandle;
pub use document::{
    apply_edit, create_blank_document, insert_image_stamp, open, open_from_bytes,
    open_with_passwords, open_with_passwords_from_bytes, redo, render_page, render_page_tiles,
    save_to_bytes, save_to_path, stamp_placement, undo, will_invalidate_signatures, DocumentHandle,
};
pub use error::FfiError;
pub use types::{
    FfiAnnotation, FfiAnnotationKind, FfiColor, FfiEditCommand, FfiOrientation, FfiPageDimensions,
    FfiPageSize, FfiPoint, FfiRect, FfiRenderOptions, FfiRenderTile, FfiSaveIntent,
    FfiSearchResult, FfiSignatureAcknowledgement, FfiTextRect, FfiTextRun,
};

uniffi::setup_scaffolding!();
