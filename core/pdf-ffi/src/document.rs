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

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use pdf_document::{Annotation, AnnotationId, AnnotationKind, Command, Document, PageId};
use pdf_manip::LopdfDocument;

use crate::error::FfiError;
use crate::selection::FfiPageCharacters;
use crate::types::{
    FfiAnnotation, FfiAnnotationKind, FfiDocumentInfo, FfiEditCommand, FfiOrientation,
    FfiPageContent, FfiPageDimensions, FfiPageSize, FfiPoint, FfiRect, FfiRenderOptions,
    FfiRenderTile, FfiSaveIntent, FfiSearchResult, FfiSignatureAcknowledgement, FfiTextRun,
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
    /// The password pdfium needs to reopen this document's bytes, kept only
    /// so [`refresh_preview`] can rebuild `render_doc` from an encrypted
    /// snapshot. `None` for an unencrypted or freshly created document.
    ///
    /// Held for the life of the handle because that is exactly as long as the
    /// render side may need to be rebuilt; it never leaves this crate, is
    /// never logged, and dies with the handle.
    render_password: Option<String>,
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
            FfiEditCommand::MoveAnnotation {
                annotation_id,
                dx,
                dy,
            } => self.replace_annotation(annotation_id, |annotation| {
                pdf_annotate::move_annotation(annotation, dx, dy)
            })?,
            FfiEditCommand::ResizeAnnotation {
                annotation_id,
                rect,
            } => self.replace_annotation(annotation_id, |annotation| {
                pdf_annotate::resize_annotation(annotation, rect.into())
            })?,
            FfiEditCommand::RestyleAnnotation {
                annotation_id,
                color,
            } => self.replace_annotation(annotation_id, |annotation| {
                pdf_annotate::restyle_annotation(annotation, color.into())
            })?,
            FfiEditCommand::ReplaceTextRunContent { item, after } => {
                Command::ReplaceTextRunContent {
                    item: item.into(),
                    after,
                }
            }
            FfiEditCommand::ReplaceTextRunWithInsertedFont { item, after } => {
                Command::ReplaceTextRunWithInsertedFont {
                    item: item.into(),
                    after,
                }
            }
            FfiEditCommand::InsertTextRun { item } => Command::InsertTextRun(item.into()),
            FfiEditCommand::RemoveTextRun { item } => Command::RemoveTextRun(item.into()),
            FfiEditCommand::MoveTextRun { item, to } => Command::MoveTextRun {
                item: item.into(),
                to: to.into(),
            },
            FfiEditCommand::InsertImage { item, source } => Command::InsertImage {
                item: item.into(),
                source,
            },
            FfiEditCommand::RemoveImage { item, source } => Command::RemoveImage {
                item: item.into(),
                source,
            },
            FfiEditCommand::MoveImage { item, to } => Command::MoveImage {
                item: item.into(),
                to: to.into(),
            },
            FfiEditCommand::ResizeImage { item, to } => Command::ResizeImage {
                item: item.into(),
                to: to.into(),
            },
            FfiEditCommand::ReplaceImageSource {
                item,
                before,
                after,
            } => Command::ReplaceImageSource {
                item: item.into(),
                before,
                after,
            },
            FfiEditCommand::SetDocumentInfo { after } => {
                let before = pdf_save::pending_document_info(&self.document)
                    .cloned()
                    .unwrap_or_else(|| self.base.document_info());
                Command::SetDocumentInfo {
                    before,
                    after: after.into(),
                }
            }
        })
    }

    fn replace_annotation(
        &self,
        annotation_id: u64,
        operation: impl FnOnce(&mut Annotation) -> Result<(), pdf_annotate::AnnotateError>,
    ) -> Result<Command, FfiError> {
        let before = self
            .document
            .annotations
            .get(AnnotationId(annotation_id))
            .cloned()
            .ok_or(FfiError::AnnotationNotFound { annotation_id })?;
        let mut after = before.clone();
        operation(&mut after).map_err(FfiError::from)?;
        Ok(Command::ReplaceAnnotation { before, after })
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

/// Applies `pdf-manip`'s permission rule to this handle's document.
///
/// The rule itself lives in `pdf_manip::text_extraction_is_allowed` — the
/// layer that owns `/P` bit semantics — so this boundary and the GTK4 shell,
/// which links the core crates directly instead of crossing UniFFI, refuse
/// the same documents.
fn text_extraction_is_allowed(document: &Document) -> bool {
    pdf_manip::text_extraction_is_allowed(document.security.as_ref())
}

fn annotation_editing_is_allowed(document: &Document) -> bool {
    pdf_manip::annotation_editing_is_allowed(document.security.as_ref())
}

/// The page-content twin of [`annotation_editing_is_allowed`], gated on the
/// modify-contents bit instead of the annotate one. A document may grant
/// either without the other, so neither may be inferred from the other.
///
/// Two questions, deliberately answered as one: whether the document permits
/// the change, and whether the result could ever be written. Every page-
/// content edit forces a full rewrite (Batch 21 decision 5), and an encrypted
/// document opened with only one of its two passwords cannot be re-encrypted
/// — so on such a document a content edit is not a change to be saved later,
/// it is one that can never be saved at all. A shell that offered it would
/// collect work it has no way to keep.
fn content_editing_is_allowed(document: &Document) -> bool {
    pdf_manip::content_editing_is_allowed(document.security.as_ref())
        && content_edit_could_be_saved(document)
}

/// Whether a full rewrite of `document` could be produced — see
/// `pdf_save::build_encryption_state`, which is the rule this mirrors.
fn content_edit_could_be_saved(document: &Document) -> bool {
    document
        .security
        .as_ref()
        .is_none_or(|security| security.credentials.complete().is_some())
}

/// Whether `command` rewrites a page's own content — text runs and images —
/// as opposed to an annotation drawn over it or the page structure around
/// it. These are the ten commands `pdf_save::content` replays into the
/// content stream at save time (Batch 21).
fn is_content_command(command: &FfiEditCommand) -> bool {
    matches!(
        command,
        FfiEditCommand::ReplaceTextRunContent { .. }
            | FfiEditCommand::ReplaceTextRunWithInsertedFont { .. }
            | FfiEditCommand::InsertTextRun { .. }
            | FfiEditCommand::RemoveTextRun { .. }
            | FfiEditCommand::MoveTextRun { .. }
            | FfiEditCommand::InsertImage { .. }
            | FfiEditCommand::RemoveImage { .. }
            | FfiEditCommand::MoveImage { .. }
            | FfiEditCommand::ResizeImage { .. }
            | FfiEditCommand::ReplaceImageSource { .. }
    )
}

/// Whether `command` edits an annotation rather than page structure — the
/// annotate permission bit (`/P` bit 6) has no say over `RotatePage`,
/// `InsertBlankPage`, `RemovePage`, or any page-content command (Batch 21):
/// editing a page's text/images is a content-modify operation, not an
/// annotation, the same distinction the PDF permission bits themselves draw.
fn is_annotation_command(command: &FfiEditCommand) -> bool {
    !matches!(
        command,
        FfiEditCommand::RotatePage { .. }
            | FfiEditCommand::InsertBlankPage { .. }
            | FfiEditCommand::RemovePage { .. }
            | FfiEditCommand::ReplaceTextRunContent { .. }
            | FfiEditCommand::ReplaceTextRunWithInsertedFont { .. }
            | FfiEditCommand::InsertTextRun { .. }
            | FfiEditCommand::RemoveTextRun { .. }
            | FfiEditCommand::MoveTextRun { .. }
            | FfiEditCommand::InsertImage { .. }
            | FfiEditCommand::RemoveImage { .. }
            | FfiEditCommand::MoveImage { .. }
            | FfiEditCommand::ResizeImage { .. }
            | FfiEditCommand::ReplaceImageSource { .. }
            | FfiEditCommand::SetDocumentInfo { .. }
    )
}

fn ffi_annotation(annotation: &Annotation) -> FfiAnnotation {
    let kind = match &annotation.kind {
        AnnotationKind::Highlight { rect, color } => FfiAnnotationKind::Highlight {
            rect: (*rect).into(),
            color: (*color).into(),
        },
        AnnotationKind::Underline { rect, color } => FfiAnnotationKind::Underline {
            rect: (*rect).into(),
            color: (*color).into(),
        },
        AnnotationKind::Strikeout { rect, color } => FfiAnnotationKind::Strikeout {
            rect: (*rect).into(),
            color: (*color).into(),
        },
        AnnotationKind::Ink { points, color } => FfiAnnotationKind::Ink {
            points: points.iter().map(|&(x, y)| FfiPoint { x, y }).collect(),
            color: (*color).into(),
        },
        AnnotationKind::Shape { rect, color } => FfiAnnotationKind::Shape {
            rect: (*rect).into(),
            color: (*color).into(),
        },
        AnnotationKind::TextNote { rect, contents, .. } => FfiAnnotationKind::TextNote {
            rect: (*rect).into(),
            contents: contents.clone(),
        },
        AnnotationKind::Stamp { rect, .. } => FfiAnnotationKind::Stamp {
            rect: (*rect).into(),
        },
        _ => FfiAnnotationKind::TextNote {
            rect: pdf_document::Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            }
            .into(),
            contents: String::new(),
        },
    };
    FfiAnnotation {
        id: annotation.id.0,
        page: annotation.page.0,
        kind,
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

    /// Per-page layout sizes in PDF points, in page order, read from the
    /// last-opened/saved bytes — the same source `render_page` draws from —
    /// so placeholder sizes always match the rendered output, even while
    /// unsaved edits are pending in the document model.
    pub fn page_dimensions(&self) -> Vec<FfiPageDimensions> {
        self.lock()
            .base
            .page_dimensions()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Extracts the text runs for one 0-indexed page. Each run has one
    /// PDF-space rectangle per Unicode scalar in its text.
    pub fn text_runs(&self, page_index: u32) -> Result<Vec<FfiTextRun>, FfiError> {
        let render_doc = {
            let state = self.lock();
            if !text_extraction_is_allowed(&state.document) {
                return Err(FfiError::UnsupportedOperation {
                    detail: "text extraction is not permitted".to_string(),
                });
            }
            state.render_doc.ok_or(FfiError::DocumentNotFound)?
        };
        pdf_render::PdfiumRenderer::new()
            .text_runs(render_doc, page_index, pdf_render::Priority::Visible)
            .wait()
            .map(|runs| runs.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    }

    /// Loads and flattens one page's characters for caret hit-testing and
    /// selection-rect queries (`FfiPageCharacters`) — the geometry a
    /// drag-select needs on every pointer-move. Same source and same
    /// permission gate as `text_runs`: this is text extraction with the
    /// paint deferred to the shell, not extraction with the pixels
    /// stripped out. Callers should hold the returned handle for the life
    /// of one drag rather than reloading it per pointer-move.
    pub fn page_characters(&self, page_index: u32) -> Result<Arc<FfiPageCharacters>, FfiError> {
        let render_doc = {
            let state = self.lock();
            if !text_extraction_is_allowed(&state.document) {
                return Err(FfiError::UnsupportedOperation {
                    detail: "text extraction is not permitted".to_string(),
                });
            }
            state.render_doc.ok_or(FfiError::DocumentNotFound)?
        };
        let runs = pdf_render::PdfiumRenderer::new()
            .text_runs(render_doc, page_index, pdf_render::Priority::Visible)
            .wait()
            .map_err(FfiError::from)?;
        Ok(Arc::new(FfiPageCharacters::new(
            pdf_render::PageCharacters::from_runs(&runs),
        )))
    }

    /// Finds exact, case-sensitive text matches in render-side page order.
    /// Search reflects the last-opened/saved bytes until pending edits are saved.
    ///
    /// The match algorithm itself lives in `pdf_render` (beside the text runs
    /// it reads) so this boundary and the GTK shell — which links the render
    /// core directly — share one implementation. The permission gate is shared
    /// the same way: the rule is `pdf_manip::text_extraction_is_allowed`,
    /// applied here to the lopdf security model the render core has no view
    /// of, and applied by the GTK shell to the context it reads with
    /// `pdf_manip::read_security_context`.
    pub fn search(&self, query: String) -> Result<Vec<FfiSearchResult>, FfiError> {
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let render_doc = {
            let state = self.lock();
            if !text_extraction_is_allowed(&state.document) {
                return Err(FfiError::UnsupportedOperation {
                    detail: "text extraction is not permitted".to_string(),
                });
            }
            state.render_doc.ok_or(FfiError::DocumentNotFound)?
        };
        pdf_render::PdfiumRenderer::new()
            .search(render_doc, query, pdf_render::Priority::Visible)
            .wait()
            .map(|found| found.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    }

    /// Annotation snapshots in paint order. Querying is read-only and remains
    /// available when editing permission is withheld.
    pub fn annotations(&self) -> Vec<FfiAnnotation> {
        self.lock()
            .document
            .annotations
            .iter()
            .map(ffi_annotation)
            .collect()
    }

    /// Reports whether a new annotation edit would be allowed by the PDF's
    /// security context.
    pub fn annotation_editing_allowed(&self) -> bool {
        annotation_editing_is_allowed(&self.lock().document)
    }

    /// Reports whether rewriting this page's own content — retyping a text
    /// run, moving an image — would be allowed by the PDF's security
    /// context. Separate from [`Self::annotation_editing_allowed`]: a
    /// document can grant either permission without the other, so a shell
    /// must ask this before offering content editing rather than reusing the
    /// annotation answer.
    pub fn content_editing_allowed(&self) -> bool {
        content_editing_is_allowed(&self.lock().document)
    }

    /// Parses `page`'s content stream on demand and returns its text runs
    /// and images (T-158, Batch 21 decision 2 — never cached on the
    /// document, unlike annotations). Reflects the last-opened/saved bytes;
    /// re-read after `save_to_bytes` before building a second content edit
    /// for the same page, since ids are only valid against the exact parse
    /// they came from (decision 6).
    pub fn read_page_content(&self, page: u32) -> Result<FfiPageContent, FfiError> {
        let state = self.lock();
        if !text_extraction_is_allowed(&state.document) {
            return Err(FfiError::UnsupportedOperation {
                detail: "text extraction is not permitted".to_string(),
            });
        }
        pdf_save::read_page_content(&state.base, PageId(page))
            .map(Into::into)
            .map_err(Into::into)
    }

    /// The `/BaseFont` name of each font `page` declares, keyed by the
    /// resource name its text runs report.
    ///
    /// For a shell drawing its own editing overlay: a run says which resource
    /// paints it, not what that resource is, and an overlay in a face the page
    /// does not use lands at the wrong width however carefully it is placed.
    /// The names come back raw — subset prefix and style suffix included — so
    /// each platform can decide which local font stands in for them, which is
    /// a question only it can answer.
    ///
    /// Behind the same permission as `read_page_content`: it describes the
    /// text on the page, and a document that withholds extraction withholds
    /// this too.
    pub fn page_font_families(&self, page: u32) -> Result<HashMap<String, String>, FfiError> {
        let state = self.lock();
        if !text_extraction_is_allowed(&state.document) {
            return Err(FfiError::UnsupportedOperation {
                detail: "text extraction is not permitted".to_string(),
            });
        }
        pdf_edit::page_font_families(state.base.as_lopdf(), PageId(page))
            .map(|families| families.into_iter().collect())
            .map_err(Into::into)
    }

    /// Current Document Info Dictionary snapshot (T-173, Batch 22): the last
    /// pending `SetDocumentInfo`'s value if one is queued (decision 5's
    /// "last one wins", same rule `pdf_save::pending_document_info` applies
    /// at save time), otherwise the value already in the file's bytes —
    /// `LopdfDocument::document_info`'s lazy read (T-169). Not cached, same
    /// criterion `read_page_content` uses: most sessions never open a
    /// metadata panel, so there is nothing to keep in sync.
    pub fn read_document_info(&self) -> FfiDocumentInfo {
        let state = self.lock();
        pdf_save::pending_document_info(&state.document)
            .cloned()
            .unwrap_or_else(|| state.base.document_info())
            .into()
    }

    pub fn can_undo(&self) -> bool {
        self.lock().document.pending_edits.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.lock().document.pending_edits.can_redo()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{
        Credential, EncryptionCredentials, Permissions, SecurityContext, SecurityHandler,
    };

    /// The rule's own cases live with the rule (`pdf_manip::security`); what
    /// this asserts is that the boundary is actually wired to it.
    #[test]
    fn text_extraction_respects_user_copy_permission_and_owner_bypass() {
        let mut document = Document::blank();
        document.security = Some(SecurityContext {
            handler: SecurityHandler::Rc4_128,
            credential: Credential::User,
            credentials: EncryptionCredentials::default(),
            permissions: Permissions(0),
        });

        assert!(!text_extraction_is_allowed(&document));

        document.security.as_mut().unwrap().credential = Credential::Owner;
        assert!(text_extraction_is_allowed(&document));
    }

    #[test]
    fn annotation_editing_respects_user_annotation_permission_and_owner_bypass() {
        let mut document = Document::blank();
        document.security = Some(SecurityContext {
            handler: SecurityHandler::Rc4_128,
            credential: Credential::User,
            credentials: EncryptionCredentials::default(),
            permissions: Permissions(0),
        });

        assert!(!annotation_editing_is_allowed(&document));

        document.security.as_mut().unwrap().credential = Credential::Owner;
        assert!(annotation_editing_is_allowed(&document));
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
) -> Result<pdf_render::DocumentHandle, FfiError> {
    pdf_render::PdfiumRenderer::new()
        .open_document_from_bytes(bytes, password)
        .map_err(Into::into)
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
    let render_doc = Some(open_render_doc_from_bytes(
        bytes.clone(),
        password.as_deref(),
    )?);

    Ok(DocumentHandle::new(DocumentState {
        document,
        base,
        original_bytes: Some(bytes),
        render_doc,
        render_password: password,
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
    let render_doc = Some(open_render_doc_from_bytes(
        bytes.clone(),
        Some(&owner_password),
    )?);

    Ok(DocumentHandle::new(DocumentState {
        document,
        base,
        original_bytes: Some(bytes),
        render_doc,
        render_password: Some(owner_password),
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
    // A zero-page document may not be renderable by pdfium; that is the one
    // intentional optional rendering case. Opened documents must always have
    // a render-side handle.
    let render_doc = match pdf_render::PdfiumRenderer::new().open_document_from_bytes(bytes, None) {
        Ok(handle) => Some(handle),
        Err(error @ pdf_render::RenderError::LibraryLoad { .. }) => return Err(error.into()),
        Err(_) => None,
    };

    Ok(DocumentHandle::new(DocumentState {
        document,
        base,
        original_bytes: None,
        render_doc,
        render_password: None,
        next_page_id: PageId(0),
        next_annotation_id: 0,
    }))
}

/// Creates a new document that already holds one blank page of the given
/// size/orientation — the "File > New" entrypoint a reader can actually
/// display.
///
/// [`create_blank_document`] deliberately returns *zero* pages: its size and
/// orientation are the defaults recorded on the page-tree root for pages
/// inserted later. That is the right base for an authoring pipeline, but a
/// shell that offers "new document" and no page-insertion UI hands its user a
/// dead end, so this wraps the two steps the GTK4 shell already performs by
/// hand (`create_blank_document` then `insert_blank_page`) and opens the
/// result through [`open_from_bytes`], the canonical entrypoint.
///
/// Going through the bytes lifecycle is what makes the difference, and it is
/// why the first page cannot simply be an `apply_edit(InsertBlankPage)` on a
/// zero-page handle: `apply_edit` mutates the `Document` model only, leaving
/// `render_doc` at the `None` a zero-page document starts with (see module
/// docs), so `page_count` would report 1 while `render_page` still failed.
/// The page has to exist in the bytes pdfium opens. It also leaves the
/// returned handle free of pending edits, so a shell's unsaved-work guard
/// does not fire on an untouched document.
#[uniffi::export]
pub fn create_document_with_blank_page(
    page_size: FfiPageSize,
    orientation: FfiOrientation,
) -> Result<Arc<DocumentHandle>, FfiError> {
    let size = page_size.into();
    let orientation = orientation.into();
    let base = pdf_manip::create_blank_document(size, orientation);
    let base = pdf_manip::insert_blank_page(&base, 0, size, orientation)?;

    let mut bytes = Vec::new();
    base.as_lopdf().clone().save_to(&mut bytes)?;
    open_from_bytes(bytes, None)
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

/// Renders every tile of one page in a single call, in the order given.
///
/// A viewer at deep zoom needs several tiles to cover one screen. Asking for
/// them one at a time reloads (and re-parses) the page per tile and pays a
/// full FFI round-trip between them; this loads the page once and returns the
/// whole set. The batch fails as a unit — a half-covered viewport is not a
/// useful result.
///
/// A single tile is this call with a one-element `tiles`; there is no separate
/// single-tile export.
#[uniffi::export]
pub fn render_page_tiles(
    handle: &DocumentHandle,
    page_index: u32,
    dpi: u32,
    tiles: Vec<FfiRenderTile>,
    options: FfiRenderOptions,
) -> Result<Vec<Arc<BitmapHandle>>, FfiError> {
    let render_doc = handle.lock().render_doc.ok_or(FfiError::DocumentNotFound)?;
    let bitmaps = pdf_render::PdfiumRenderer::new()
        .render_page_tiles(
            render_doc,
            page_index,
            dpi,
            tiles.into_iter().map(Into::into).collect(),
            options.into(),
            pdf_render::Priority::Visible,
        )
        .wait()?;
    Ok(bitmaps
        .into_iter()
        .map(|bitmap| Arc::new(BitmapHandle::new(bitmap)))
        .collect())
}

/// Applies a single undoable edit command to `handle`'s in-memory document
/// model (spec "Undo/Redo via EditLog"). Persisting the change to disk
/// requires a subsequent `save`/`save_to_bytes`/`save_to_path` call.
#[uniffi::export]
pub fn apply_edit(handle: &DocumentHandle, command: FfiEditCommand) -> Result<(), FfiError> {
    let mut state = handle.lock();
    if is_annotation_command(&command) && !annotation_editing_is_allowed(&state.document) {
        return Err(FfiError::UnsupportedOperation {
            detail: "annotation editing is not permitted".to_string(),
        });
    }
    let is_content = is_content_command(&command);
    if is_content && !pdf_manip::content_editing_is_allowed(state.document.security.as_ref()) {
        return Err(FfiError::UnsupportedOperation {
            detail: "content editing is not permitted".to_string(),
        });
    }
    if is_content && !content_edit_could_be_saved(&state.document) {
        // Refused here rather than at the save it would fail: an edit that can
        // never be written is not pending work, and letting it queue would
        // also break the preview refresh, which saves a snapshot the same way.
        return Err(FfiError::UnsupportedOperation {
            detail: "editing page content rewrites the whole file; reopen this encrypted                      document with both its user and owner passwords first"
                .to_string(),
        });
    }

    let core_command = state.build_core_command(command)?;
    if is_content {
        // Validate before recording, never only at save time — see
        // `pdf_save::validate_content_command` for why a content command
        // recorded unchecked can only fail later, and take every other
        // queued edit down with it.
        pdf_save::validate_content_command(state.base.as_lopdf(), &core_command)?;

        if let Some(index) = pending_replacement_index(&state.document, &core_command) {
            // Retyping the same run twice amends the queued command instead
            // of appending a second one. `EditLog::amend`'s own docs carry
            // the reasoning: a save replays content commands in order against
            // a document it mutates as it goes, so a second command still
            // describing the *pre-first-edit* run — all a caller can have,
            // since `read_page_content` reads the untouched base — would
            // resolve against nothing and take the whole save down.
            state.document.pending_edits.amend(index, core_command);
            return Ok(());
        }
    }
    apply_command(&mut state.document, core_command);
    Ok(())
}

/// The queued command `command` should replace rather than follow, if any.
///
/// Only text replacement folds today, because it is the only content edit a
/// caller can repeat against the same target without re-reading the page:
/// the run it names keeps its identity through the edit. The move/resize
/// variants describe geometry the caller cannot recompute against a pending
/// state, and are refused rather than folded by the shells that offer them
/// (see the GTK shell's `text_move_refusal`); they append here, as before.
fn pending_replacement_index(document: &Document, command: &Command) -> Option<usize> {
    let item = match command {
        Command::ReplaceTextRunContent { item, .. }
        | Command::ReplaceTextRunWithInsertedFont { item, .. } => item,
        _ => return None,
    };

    document.pending_edits.entries().iter().position(|queued| {
        matches!(
            queued,
            Command::ReplaceTextRunContent { item: queued_item, .. }
                | Command::ReplaceTextRunWithInsertedFont { item: queued_item, .. }
                if queued_item.id == item.id && queued_item.page == item.page
        )
    })
}

/// Re-derives the render-side document from the pending edits, so
/// `render_page` shows page-content changes that have not been saved to a
/// destination yet (Batch 21 decision 6).
///
/// Page content is not something a shell can draw over the bitmap the way an
/// annotation overlay can: retyped text *is* the page, and only pdfium can
/// show it. So this saves the current model to an in-memory snapshot and
/// reopens **that** as the render document, leaving `document`, `base` and
/// `original_bytes` — the edit log and everything a real save is computed
/// from — exactly as they were. Nothing touches disk, the handle keeps its
/// full undo history, and the next `save_to_bytes` is still computed from the
/// originally opened bytes rather than from this snapshot.
///
/// Signatures are acknowledged silently here for the same reason: this
/// produces pixels, not a file. Anything the reader could keep still goes
/// through `save_to_bytes`, which refuses an unacknowledged
/// signature-breaking save as before.
///
/// A shell calls this after every committed content edit, and after an
/// undo/redo that moved one. An annotation-only session never needs it.
///
/// The snapshot deliberately leaves this session's **annotations** out. Every
/// shell draws those as its own overlay over the bitmap — that is what makes
/// them selectable and draggable before a save — so baking them in here would
/// paint each of them twice, once by pdfium and once by the shell. Only the
/// session's annotations are dropped: annotations the file already carried
/// are never in this set (`pdf_save::document_from_lopdf` starts it empty)
/// and reach the snapshot through `base` like the rest of the page, so they
/// keep rendering exactly once.
#[uniffi::export]
pub fn refresh_preview(handle: &DocumentHandle) -> Result<(), FfiError> {
    let mut state = handle.lock();

    let mut preview = state.document.clone();
    preview.annotations = Default::default();

    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document: &preview,
        base: &state.base,
        original_bytes: state.original_bytes.as_deref(),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
    })?;

    let refreshed = open_render_doc_from_bytes(bytes, state.render_password.as_deref())?;
    if let Some(stale) = state.render_doc.replace(refreshed) {
        // Best-effort, exactly as in `Drop`: a preview that already renders
        // is not worth failing over a handle pdfium may have closed itself.
        let _ = pdf_render::PdfiumRenderer::new().close_document(stale);
    }
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
    if !annotation_editing_is_allowed(&state.document) {
        return Err(FfiError::UnsupportedOperation {
            detail: "annotation editing is not permitted".to_string(),
        });
    }
    let id = state.allocate_annotation_id();
    let annotation =
        pdf_annotate::stamp_from_image_bytes(id, PageId(page_index), &image_bytes, rect.into())?;
    apply_command(&mut state.document, Command::AddAnnotation(annotation));
    Ok(())
}

/// Rect for a stamp the app places at (`anchor_x`, `anchor_y`) — a drop or a
/// clipboard paste — rather than one the user traced.
///
/// The size is the core's policy, not the shell's: the image keeps its own
/// proportions instead of being squashed into a fixed box, and every shell
/// gets the same rect for the same bytes because none of them computes it.
/// Pair it with [`insert_image_stamp`], which takes the rect this returns.
///
/// Takes no document handle — it is pure geometry over the image bytes, and
/// is safe to call before deciding whether the insert can go ahead.
#[uniffi::export]
pub fn stamp_placement(
    image_bytes: Vec<u8>,
    anchor_x: f64,
    anchor_y: f64,
) -> Result<FfiRect, FfiError> {
    let rect = pdf_annotate::stamp_placement(
        &image_bytes,
        (anchor_x, anchor_y),
        pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT,
    )?;
    Ok(rect.into())
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
///
/// `signatures` is how a shell says it has told the user that this save
/// breaks a signature the file carries. Left at
/// [`FfiSignatureAcknowledgement::Unacknowledged`], such a save returns
/// [`FfiError::SignaturesWouldBeInvalidated`] rather than silently producing
/// a file whose signature no longer verifies. Ask
/// [`will_invalidate_signatures`] first if you want to warn before the user
/// commits to saving.
#[uniffi::export]
pub fn save_to_bytes(
    handle: &DocumentHandle,
    intent: FfiSaveIntent,
    signatures: FfiSignatureAcknowledgement,
) -> Result<Vec<u8>, FfiError> {
    let mut state = handle.lock();
    record_strip_consent_if_requested(&mut state.document, intent);

    let input = pdf_save::SaveInput {
        document: &state.document,
        base: &state.base,
        original_bytes: state.original_bytes.as_deref(),
        intent: intent.into(),
        signatures: signatures.into(),
    };
    pdf_save::save_document(input).map_err(Into::into)
}

/// Whether saving `handle` with `intent` would break a signature the file
/// already carries — the question a shell asks *before* saving, so it can
/// warn and let the user decide.
///
/// `true` here is exactly the condition under which [`save_to_bytes`] refuses
/// an unacknowledged save.
#[uniffi::export]
pub fn will_invalidate_signatures(
    handle: &DocumentHandle,
    intent: FfiSaveIntent,
) -> Result<bool, FfiError> {
    let state = handle.lock();

    pdf_save::will_invalidate_signatures(pdf_save::SaveInput {
        document: &state.document,
        base: &state.base,
        original_bytes: state.original_bytes.as_deref(),
        intent: intent.into(),
        // Irrelevant to the question: this reports what the file and the
        // edits imply, not what the caller has agreed to.
        signatures: pdf_save::SignatureAcknowledgement::Unacknowledged,
    })
    .map_err(Into::into)
}

/// Saves `handle` to `path` (GTK4-style convenience — delegates to
/// [`save_to_bytes`], then writes the result).
#[uniffi::export]
pub fn save_to_path(
    handle: &DocumentHandle,
    path: String,
    intent: FfiSaveIntent,
    signatures: FfiSignatureAcknowledgement,
) -> Result<(), FfiError> {
    let bytes = save_to_bytes(handle, intent, signatures)?;
    std::fs::write(Path::new(&path), bytes)?;
    Ok(())
}
