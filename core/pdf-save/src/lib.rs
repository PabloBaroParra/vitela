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
//! - [`rewrite`]: whether an encrypted document could be fully rewritten at
//!   all — the question a shell asks before letting an edit be recorded
//!   (batch PDF assembly section 5).
//! - [`clock`] (T-036): injectable clock + trailer-`/ID` generator hooks.
//! - [`content`] (T-156): replays page-content edits (Batch 21) at save
//!   time, and reports whether the rewrite invalidates existing signatures.
//! - [`metadata`] (T-170, Batch 22): applies `Command::SetDocumentInfo` to
//!   the `/Info` dict at save time.
//! - [`strategy`] (T-032, T-033): writer selection and
//!   the [`save_document`] auto-selection entry point.
//! - [`export`] (T-037): page export as PNG/JPEG at selectable DPI.
//! - [`imported_sources`]: session-lifetime registry of imported PDFs,
//!   feeding [`ImportedSources`] at save time (batch PDF assembly §1).
//! - [`origin`]: which document and object hold a model page's bytes — the
//!   read path's answer to a `PageId` that is no longer a position (batch PDF
//!   assembly §6).
//! - [`error`]: [`SaveError`], the shared error type across this crate.

pub mod annotations;
pub mod bridge;
pub mod clock;
pub mod content;
pub mod error;
pub mod export;
pub mod forms;
pub mod imported_sources;
pub mod metadata;
pub mod origin;
pub mod rewrite;
pub mod security;
pub mod strategy;

pub use annotations::{attach_annotations, ObjectSink};
pub use bridge::{
    document_from_lopdf, has_structural_page_changes, imported_pages_from_lopdf,
    page_annotation_objects, page_object_ids, populate_document, read_page_content,
    replay_page_ops, rotation_changes, ImportedSources, ReplayOutcome,
};
pub use clock::{
    Clock, FixedClock, IdGenerator, RandomIdGenerator, SequentialIdGenerator, SystemClock,
};
pub use content::{has_content_edits, replay_content_edits, validate_content_command};
pub use error::SaveError;
pub use export::{export_page_as_image, ExportFormat};
pub use forms::{ensure_acroform, write_form_fields};
pub use imported_sources::ImportedSourceRegistry;
pub use metadata::{apply_document_info, pending_document_info};
pub use origin::{page_backing, page_font_families_of, read_page_content_of, PageBacking};
pub use rewrite::{full_rewrite_blocker, RewriteBlocker};
pub use security::{apply_encryption_for_full_rewrite, build_encryption_state, SaveIntent};
pub use strategy::{
    append_incremental_update, save_document, save_document_with_options,
    will_invalidate_signatures, SaveInput, SaveOptions, SignatureAcknowledgement,
};
