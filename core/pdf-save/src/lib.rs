//! `pdf-save`: two writer modes — incremental-update (primary/default) and
//! full-rewrite (structural operations) — plus encrypted-save re-encryption, explicit
//! strip-protection audit logging, deterministic clock/ID-generator hooks,
//! and image export. See Batch 6 (T-032..T-038) and `design.md` "Save
//! Pipeline".
//!
//! ## Module map
//! - [`bridge`] (T-032a): population-on-open + replay-on-save between
//!   `pdf_document::Document` and `pdf_manip::LopdfDocument` — the mandatory
//!   verify-checkpoint gate this batch had to resolve before anything else.
//! - [`annotations`] (part of T-032): writes `Annotation`s into a real lopdf
//!   object graph, assigning the real object ids `pdf-annotate`'s appearance
//!   builders leave as placeholders.
//! - [`security`] (T-034, T-035): re-encrypt-by-default / explicit-strip save
//!   intent for the full-rewrite writer.
//! - [`clock`] (T-036): injectable clock + trailer-`/ID` generator hooks.
//! - [`content`] (T-156): replays page-content edits (Batch 21) at save
//!   time, and reports whether the rewrite invalidates existing signatures.
//! - [`strategy`] (T-032, T-033): writer selection and
//!   the [`save_document`] auto-selection entry point.
//! - [`export`] (T-037): page export as PNG/JPEG at selectable DPI.
//! - [`error`]: [`SaveError`], the shared error type across this crate.

pub mod annotations;
pub mod bridge;
pub mod clock;
pub mod content;
pub mod error;
pub mod export;
pub mod forms;
pub mod security;
pub mod strategy;

pub use annotations::{attach_annotations, ObjectSink};
pub use bridge::{
    document_from_lopdf, has_structural_page_changes, page_annotation_objects, page_object_ids,
    populate_document, read_page_content, replay_page_ops, rotation_changes,
};
pub use clock::{
    Clock, FixedClock, IdGenerator, RandomIdGenerator, SequentialIdGenerator, SystemClock,
};
pub use content::{
    has_content_edits, has_signatures, replay_content_edits, validate_content_command,
};
pub use error::SaveError;
pub use export::{export_page_as_image, ExportFormat};
pub use forms::{ensure_acroform, write_form_fields};
pub use security::{apply_encryption_for_full_rewrite, build_encryption_state, SaveIntent};
pub use strategy::{
    append_incremental_update, save_document, save_document_with_options,
    will_invalidate_signatures, SaveInput, SaveOptions, SignatureAcknowledgement,
};
