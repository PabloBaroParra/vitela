//! `DocumentHandle` (T-039) and the FFI command surface (T-040): open/
//! create_blank/render_page/apply_edit/insert_image_stamp/save, plus the
//! bytes-based `open_from_bytes`/`save_to_bytes` canonical entrypoints
//! (T-068 DELTA).
//!
//! `DocumentHandle` is a UniFFI interface object (Arc-based). Unlike
//! `BitmapHandle`, its state is genuinely mutable (`apply_edit` mutates the
//! pending `Document`), so it wraps `Mutex<DocumentState>` — every exported
//! method locks internally rather than requiring `&mut self`, matching how
//! every other UniFFI interface-object method receives `&self`.
//!
//! ## Path vs. bytes: which is canonical
//!
//! Per spec delta "FileAccessPort" and design.md §3 ("Android shell"):
//! `open_from_bytes`/`save_to_bytes` are the **canonical** cross-platform
//! contract — Android (Storage Access Framework) and iOS (security-scoped
//! bookmarks) never get a plain filesystem path, only a byte stream. `open`/
//! `save_to_path` remain as thin conveniences layered on top (read/write the
//! file, then delegate) — useful for the GTK4 shell even though it actually
//! bypasses this crate entirely (direct crate dependency, per design.md's
//! "GTK4 FFI bypass" decision) and for any future desktop shell that already
//! has a path in hand.
//!
//! ## Known limitation: `render_page` renders the last-opened/saved bytes
//!
//! The render-side handle (`pdf_render::DocumentHandle`) is opened once, at
//! `open`/`open_from_bytes`/`create_blank_document` time, from a real byte
//! buffer pdfium can parse. It is **not** re-derived after `apply_edit`/
//! `insert_image_stamp` — those mutate the pure `pdf_document::Document`
//! model only. Rendering a page that reflects newly-applied-but-unsaved
//! edits requires `save_to_bytes` followed by opening a *new* `DocumentHandle`
//! from the returned bytes. This mirrors the architecture Batches 3-6 already
//! established (no live pdfium-sync mechanism exists yet) and is deferred as
//! a follow-up, not a Batch 7 regression.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use pdf_document::{AnnotationId, Command, Document, PageId};
use pdf_manip::LopdfDocument;

use crate::error::FfiError;
use crate::types::{
    FfiEditCommand, FfiOrientation, FfiPageSize, FfiRect, FfiRenderOptions, FfiSaveIntent,
};
use crate::BitmapHandle;

struct DocumentState {
    document: Document,
    base: LopdfDocument,
    /// `None` for a freshly created document with nothing yet saved (forces
    /// a full rewrite on first save — mirrors `pdf_save::SaveInput`'s own
    /// contract).
    original_bytes: Option<Vec<u8>>,
    /// `None` when the render-side pdfium document could not be opened
    /// (e.g. a brand-new blank document with zero pages) — `render_page`
    /// reports `FfiError::DocumentNotFound` in that case.
    render_doc: Option<pdf_render::DocumentHandle>,
    next_page_id: PageId,
    next_annotation_id: u64,
}

impl DocumentState {
    fn allocate_page_id(&mut self) -> PageId {
        let id = self.next_page_id;
        self.next_page_id = PageId(id.0 + 1);
        id
    }

    fn allocate_annotation_id(&mut self) -> AnnotationId {
        let id = AnnotationId(self.next_annotation_id);
        self.next_annotation_id += 1;
        id
    }

    /// Translates an FFI-facing edit command into the real
    /// `pdf_document::Command`, allocating fresh page/annotation ids and
    /// resolving `RemoveAnnotation`'s referenced annotation from the current
    /// `AnnotationSet` (the real `Command::RemoveAnnotation` variant carries
    /// the removed value itself — see `edit_log.rs` module docs — so it must
    /// be looked up before the command is built, not after).
    fn build_core_command(&mut self, command: FfiEditCommand) -> Result<Command, FfiError> {
        use pdf_document::Page;

        Ok(match command {
            FfiEditCommand::RotatePage {
                page,
                delta_degrees,
            } => Command::RotatePage {
                page: PageId(page),
                delta_degrees,
            },
            FfiEditCommand::InsertBlankPage {
                index,
                size,
                orientation,
            } => {
                let id = self.allocate_page_id();
                let page = Page::blank(id, size.into(), orientation.into());
                Command::InsertPage {
                    index: index as usize,
                    page,
                }
            }
            FfiEditCommand::RemovePage { index } => {
                let page = self
                    .document
                    .pages
                    .get(index as usize)
                    .cloned()
                    .ok_or(FfiError::PageIndexOutOfBounds { index })?;
                Command::RemovePage {
                    index: index as usize,
                    page,
                }
            }
            FfiEditCommand::AddHighlight { page, rect, color } => {
                let id = self.allocate_annotation_id();
                Command::AddAnnotation(pdf_annotate::highlight(
                    id,
                    PageId(page),
                    rect.into(),
                    color.into(),
                ))
            }
            FfiEditCommand::AddUnderline { page, rect, color } => {
                let id = self.allocate_annotation_id();
                Command::AddAnnotation(pdf_annotate::underline(
                    id,
                    PageId(page),
                    rect.into(),
                    color.into(),
                ))
            }
            FfiEditCommand::AddStrikeout { page, rect, color } => {
                let id = self.allocate_annotation_id();
                Command::AddAnnotation(pdf_annotate::strikeout(
                    id,
                    PageId(page),
                    rect.into(),
                    color.into(),
                ))
            }
            FfiEditCommand::AddShape { page, rect, color } => {
                let id = self.allocate_annotation_id();
                Command::AddAnnotation(pdf_annotate::shape(
                    id,
                    PageId(page),
                    rect.into(),
                    color.into(),
                ))
            }
            FfiEditCommand::AddInk {
                page,
                points,
                color,
            } => {
                let id = self.allocate_annotation_id();
                let points = points.into_iter().map(|p| (p.x, p.y)).collect();
                Command::AddAnnotation(pdf_annotate::ink(id, PageId(page), points, color.into()))
            }
            FfiEditCommand::AddTextNote {
                page,
                rect,
                contents,
            } => {
                let id = self.allocate_annotation_id();
                Command::AddAnnotation(pdf_annotate::text_note(
                    id,
                    PageId(page),
                    rect.into(),
                    contents,
                ))
            }
            FfiEditCommand::RemoveAnnotation { annotation_id } => {
                let id = AnnotationId(annotation_id);
                let annotation = self
                    .document
                    .annotations
                    .get(id)
                    .cloned()
                    .ok_or(FfiError::AnnotationNotFound { annotation_id })?;
                Command::RemoveAnnotation(annotation)
            }
        })
    }
}

impl Drop for DocumentState {
    fn drop(&mut self) {
        if let Some(doc) = self.render_doc.take() {
            // Best-effort: the pdfium actor may already be shutting down at
            // process exit, in which case there is nothing left to leak.
            let _ = pdf_render::PdfiumRenderer::new().close_document(doc);
        }
    }
}

/// Opaque handle to an open (or freshly created) document. See module docs
/// for the mutability model and the `render_page` staleness limitation.
#[derive(uniffi::Object)]
pub struct DocumentHandle {
    state: Mutex<DocumentState>,
}

impl DocumentHandle {
    fn new(state: DocumentState) -> Arc<Self> {
        Arc::new(DocumentHandle {
            state: Mutex::new(state),
        })
    }

    fn lock(&self) -> MutexGuard<'_, DocumentState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[uniffi::export]
impl DocumentHandle {
    /// Current page count of the in-memory document model (reflects
    /// `apply_edit`'s page-structural commands immediately, unlike
    /// rendering — see module docs).
    pub fn page_count(&self) -> u32 {
        self.lock().document.pages.len() as u32
    }
}

fn apply_command(document: &mut Document, command: Command) {
    // `EditLog::apply` needs `&mut EditLog` and `&mut Document` at once,
    // which can't both be reached as `document.pending_edits.apply(&mut
    // document, ..)` — same take/apply/restore dance used throughout
    // `pdf-save`/`pdf-document`'s own tests (see e.g. `strategy.rs`).
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

fn open_render_doc_from_bytes(
    bytes: Vec<u8>,
    password: Option<&str>,
) -> Option<pdf_render::DocumentHandle> {
    pdf_render::PdfiumRenderer::new()
        .open_document_from_bytes(bytes, password)
        .ok()
}

/// Opens the PDF at `path` (GTK4-style convenience — reads the file, then
/// delegates to [`open_from_bytes`], the canonical entrypoint).
#[uniffi::export]
pub fn open(path: String, password: Option<String>) -> Result<Arc<DocumentHandle>, FfiError> {
    let bytes = std::fs::read(Path::new(&path))?;
    open_from_bytes(bytes, password)
}

/// Opens a PDF from an in-memory byte buffer, optionally decrypting it with
/// `password` — the **canonical** cross-platform open entrypoint (spec delta
/// "FileAccessPort").
#[uniffi::export]
pub fn open_from_bytes(
    bytes: Vec<u8>,
    password: Option<String>,
) -> Result<Arc<DocumentHandle>, FfiError> {
    let (base, security) = pdf_manip::open_document_from_bytes(&bytes, password.as_deref())?;
    let document = pdf_save::document_from_lopdf(&base, security)?;
    let next_page_id = PageId(document.pages.len() as u32);
    let render_doc = open_render_doc_from_bytes(bytes.clone(), password.as_deref());

    Ok(DocumentHandle::new(DocumentState {
        document,
        base,
        original_bytes: Some(bytes),
        render_doc,
        next_page_id,
        next_annotation_id: 0,
    }))
}

/// Opens the encrypted PDF at `path` after verifying both its user and owner
/// passwords (dual-password convenience — reads the file, then delegates to
/// [`open_with_passwords_from_bytes`]).
#[uniffi::export]
pub fn open_with_passwords(
    path: String,
    user_password: String,
    owner_password: String,
) -> Result<Arc<DocumentHandle>, FfiError> {
    let bytes = std::fs::read(Path::new(&path))?;
    open_with_passwords_from_bytes(bytes, user_password, owner_password)
}

/// Opens an encrypted PDF from an in-memory byte buffer after independently
/// verifying both its user and owner passwords — required before an
/// encrypted **full-rewrite** save (any structural page edit), which must
/// re-apply both password roles and fails with
/// [`FfiError::InvalidSaveRequest`] on a single-password handle (spec
/// "Encrypted Document Save Behavior"). Single-password [`open_from_bytes`]
/// remains sufficient for incremental saves. An unencrypted document opens
/// normally, ignoring the passwords.
#[uniffi::export]
pub fn open_with_passwords_from_bytes(
    bytes: Vec<u8>,
    user_password: String,
    owner_password: String,
) -> Result<Arc<DocumentHandle>, FfiError> {
    let (base, security) = pdf_manip::open_document_with_passwords_from_bytes(
        &bytes,
        &user_password,
        &owner_password,
    )?;
    let document = pdf_save::document_from_lopdf(&base, security)?;
    let next_page_id = PageId(document.pages.len() as u32);
    let render_doc = open_render_doc_from_bytes(bytes.clone(), Some(&owner_password));

    Ok(DocumentHandle::new(DocumentState {
        document,
        base,
        original_bytes: Some(bytes),
        render_doc,
        next_page_id,
        next_annotation_id: 0,
    }))
}

/// Creates a new, blank PDF (zero pages) with the given default page size/
/// orientation (spec "Create Blank Document"). Authored with the same
/// `apply_edit`/`insert_image_stamp` tools as any opened PDF — there is no
/// separate "authoring mode" API surface (design.md).
#[uniffi::export]
pub fn create_blank_document(
    page_size: FfiPageSize,
    orientation: FfiOrientation,
) -> Result<Arc<DocumentHandle>, FfiError> {
    let base = pdf_manip::create_blank_document(page_size.into(), orientation.into());
    let document = pdf_save::document_from_lopdf(&base, None)?;

    let mut bytes = Vec::new();
    base.as_lopdf().clone().save_to(&mut bytes)?;
    // A zero-page document may not be renderable by pdfium; that's fine —
    // `render_page` reports `DocumentNotFound` until a page exists (see
    // module docs). `.ok()` avoids failing document creation itself over it.
    let render_doc = open_render_doc_from_bytes(bytes, None);

    Ok(DocumentHandle::new(DocumentState {
        document,
        base,
        original_bytes: None,
        render_doc,
        next_page_id: PageId(0),
        next_annotation_id: 0,
    }))
}

/// Renders `page_index` at `dpi`, returning an opaque, explicitly-releasable
/// [`BitmapHandle`] (spec "Bitmap Handle Lifecycle"). See module docs for
/// the staleness limitation relative to unsaved `apply_edit` changes.
#[uniffi::export]
pub fn render_page(
    handle: &DocumentHandle,
    page_index: u32,
    dpi: u32,
    options: FfiRenderOptions,
) -> Result<Arc<BitmapHandle>, FfiError> {
    let render_doc = handle.lock().render_doc.ok_or(FfiError::DocumentNotFound)?;

    let bitmap = pdf_render::PdfiumRenderer::new()
        .render_page(
            render_doc,
            page_index,
            dpi,
            None,
            options.into(),
            pdf_render::Priority::Visible,
        )
        .wait()?;

    Ok(Arc::new(BitmapHandle::new(bitmap)))
}

/// Applies a single undoable edit command to `handle`'s in-memory document
/// model (spec "Undo/Redo via EditLog"). Persisting the change to disk
/// requires a subsequent `save`/`save_to_bytes`/`save_to_path` call.
#[uniffi::export]
pub fn apply_edit(handle: &DocumentHandle, command: FfiEditCommand) -> Result<(), FfiError> {
    let mut state = handle.lock();
    let core_command = state.build_core_command(command)?;
    apply_command(&mut state.document, core_command);
    Ok(())
}

/// Undoes the most recently applied edit command, if any. Returns `true` if
/// a command was undone (spec "Undo/Redo via EditLog").
#[uniffi::export]
pub fn undo(handle: &DocumentHandle) -> bool {
    let mut state = handle.lock();
    let mut log = std::mem::take(&mut state.document.pending_edits);
    let undone = log.undo(&mut state.document);
    state.document.pending_edits = log;
    undone
}

/// Re-applies the most recently undone edit command, if any. Returns `true`
/// if a command was redone.
#[uniffi::export]
pub fn redo(handle: &DocumentHandle) -> bool {
    let mut state = handle.lock();
    let mut log = std::mem::take(&mut state.document.pending_edits);
    let redone = log.redo(&mut state.document);
    state.document.pending_edits = log;
    redone
}

/// Inserts a Stamp annotation built from raw image bytes (PNG/JPEG) at
/// `rect` on `page_index` (spec "Insert Image from Bytes" / "Image Stamp
/// Annotations") — the same building block clipboard-paste and drag-and-drop
/// use shell-side (design.md "Image Insertion & Clipboard Ownership").
#[uniffi::export]
pub fn insert_image_stamp(
    handle: &DocumentHandle,
    page_index: u32,
    image_bytes: Vec<u8>,
    rect: FfiRect,
) -> Result<(), FfiError> {
    let mut state = handle.lock();
    let id = state.allocate_annotation_id();
    let annotation =
        pdf_annotate::stamp_from_image_bytes(id, PageId(page_index), &image_bytes, rect.into())?;
    apply_command(&mut state.document, Command::AddAnnotation(annotation));
    Ok(())
}

/// Records the explicit-strip audit event (spec "Explicit strip with
/// consent") when `intent` is `StripProtection` — `pdf-save` itself never
/// touches `EditLog`/`AuditLog`; the FFI boundary is exactly where the
/// shell's confirmed user consent becomes a durable record, per
/// `pdf_save::security` module docs ("Callers MUST record
/// `AuditEvent::StripProtectionConsent` ... themselves before calling
/// save").
fn record_strip_consent_if_requested(document: &mut Document, intent: FfiSaveIntent) {
    if intent == FfiSaveIntent::StripProtection {
        document.audit_log.record(
            pdf_document::AuditEvent::StripProtectionConsent,
            pdf_document::AuditActor::User,
        );
    }
}

/// Saves `handle`'s current document state, producing a complete, valid PDF
/// byte buffer — the **canonical** cross-platform save entrypoint (spec
/// delta "FileAccessPort"). Each call recomputes a full, self-contained
/// snapshot from the originally opened bytes plus every edit applied so far
/// (an idempotent "recompute" model, not a chained-revisions-on-disk model —
/// callers are free to discard any earlier call's returned bytes).
#[uniffi::export]
pub fn save_to_bytes(handle: &DocumentHandle, intent: FfiSaveIntent) -> Result<Vec<u8>, FfiError> {
    let mut state = handle.lock();
    record_strip_consent_if_requested(&mut state.document, intent);

    let input = pdf_save::SaveInput {
        document: state.document.clone(),
        base: state.base.clone(),
        original_bytes: state.original_bytes.clone(),
        intent: intent.into(),
    };
    pdf_save::save_document(input).map_err(Into::into)
}

/// Saves `handle` to `path` (GTK4-style convenience — delegates to
/// [`save_to_bytes`], then writes the result).
#[uniffi::export]
pub fn save_to_path(
    handle: &DocumentHandle,
    path: String,
    intent: FfiSaveIntent,
) -> Result<(), FfiError> {
    let bytes = save_to_bytes(handle, intent)?;
    std::fs::write(Path::new(&path), bytes)?;
    Ok(())
}
