//! `pdf-document`: the center of the hexagonal core.
//!
//! Holds the pure PDF document data model (`Document`, `Page`, `AnnotationSet`,
//! `EditLog`, `SecurityContext` — landing in Batch 2, see `design.md`), plus
//! the page-content model (`PageContent` — Batch 21), which is modelled here
//! but deliberately not owned by `Document`; see the `content` module docs.
//!
//! This crate MUST NOT depend on pdfium, lopdf, or any I/O-performing crate —
//! it stays pure data so the render/manip/save backends remain swappable (e.g.
//! pdfium -> hayro) without touching this crate or its callers.

pub mod annotation;
pub mod audit_log;
pub mod content;
pub mod document;
pub mod edit_log;
pub mod form;
pub mod security;

pub use annotation::{Annotation, AnnotationId, AnnotationKind, AnnotationSet, Color, Popup, Rect};
pub use audit_log::{AuditActor, AuditEntry, AuditEvent, AuditLog};
pub use content::{ContentItemId, FontKind, ImageItem, PageContent, TextRun};
pub use document::{Document, Orientation, Page, PageId, PageSize, Rotation};
pub use edit_log::{Command, EditLog};
pub use form::{
    FieldOrigin, FieldValue, FontFamily, FormField, FormFieldId, FormFieldKind, FormFieldSet,
    RadioOption, TextStyle,
};
pub use security::{
    Credential, EncryptionCredentials, Permissions, SecurityContext, SecurityHandler,
};
