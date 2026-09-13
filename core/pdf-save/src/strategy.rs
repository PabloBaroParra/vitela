//! Save pipeline (T-032, T-033): the incremental-update writer (primary
//! path) and full rewrite (structural operations), selected by the top-level
//! [`save_document`] entry point.
//!
//! ## Writer selection
//!
//! [`save_document`] picks the writer automatically:
//! - Any structural page change ([`bridge::has_structural_page_changes`]),
//!   `SaveIntent::StripProtection`, or a freshly-created document with no
//!   `original_bytes` to append to → full rewrite.
//! - Otherwise (annotations and/or page rotation only, against a real
//!   previously-saved file) → incremental update.
//!
//! ## Two entry points, not one
//!
//! [`save_document`] writes the document. [`save_preview`] writes the same
//! document *minus its `annotations` set*, for a caller that rasterizes the
//! result and draws those itself — see its own doc, which also covers why
//! form fields are written by both. Both pick their writer by the rule above.

use std::sync::Arc;

use lopdf::{Dictionary, IncrementalDocument, Object, ObjectId};
use pdf_document::{Document, Page};
use pdf_manip::LopdfDocument;

use crate::annotations::{self, ObjectSink};
use crate::bridge::{self, ImportedSources};
use crate::clock::{Clock, IdGenerator, RandomIdGenerator, SystemClock};
use crate::content;
use crate::error::SaveError;
use crate::metadata;
use crate::security::{self, SaveIntent};

/// Injectable clock/id-generator hooks (T-036), shared by both writers.
/// Production code uses [`SaveOptions::default`]; CI/determinism tests
/// inject [`crate::clock::FixedClock`]/[`crate::clock::SequentialIdGenerator`].
pub struct SaveOptions {
    pub clock: Arc<dyn Clock>,
    pub id_generator: Arc<dyn IdGenerator>,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            id_generator: Arc::new(RandomIdGenerator::new()),
        }
    }
}

impl std::fmt::Debug for SaveOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveOptions").finish_non_exhaustive()
    }
}

/// Bundled input for a save — a **borrowing view** over state the caller
/// keeps.
///
/// Saving is not a one-shot operation: a shell opens a document once and then
/// saves the same `base`/`document` pair after every edit (and, once
/// annotations are wired up, after every *committed* annotation — see
/// `tests/perf_edit_reopen.rs`). Owning these fields would charge the caller a
/// full clone of the parsed document tree and the original byte buffer on
/// every save, which on a 52 MiB document measured 15.8 ms — 27% of the whole
/// edit round-trip. Borrowing moves that cost to the one place that genuinely
/// needs ownership: `lopdf::IncrementalDocument::create_from`.
///
/// `original_bytes` is `None` for a freshly created document
/// (`create_blank_document`, never yet saved) and `Some(..)` for a document
/// opened from an existing file — required by the incremental writer, unused
/// by the full-rewrite writer.
#[derive(Debug, Clone, Copy)]
pub struct SaveInput<'a> {
    pub document: &'a Document,
    pub base: &'a LopdfDocument,
    pub original_bytes: Option<&'a [u8]>,
    pub intent: SaveIntent,
    /// Whether the caller has already told the user that this save breaks a
    /// signature the file carries. See [`SignatureAcknowledgement`].
    pub signatures: SignatureAcknowledgement,
    /// The imported PDFs this save may materialize pages from — see
    /// [`ImportedSources`]. `ImportedSources::none()` for a document that
    /// never imported anything, which is every save that predates the import
    /// feature; a save that meets an imported page without its source is
    /// refused rather than guessed at.
    pub imported_sources: ImportedSources<'a, 'a>,
}

/// Whether the caller has dealt with the fact that a save will invalidate a
/// signature already in the file.
///
/// A full rewrite cannot preserve a signature — the bytes it signed are gone
/// — and page-content editing (Batch 21) makes a rewrite reachable from an
/// ordinary text change, so this stopped being a corner case.
///
/// The decision belongs to the user, not to this crate. What this crate can
/// do is make sure the decision is actually *made*: the default is
/// [`Unacknowledged`](Self::Unacknowledged), so a caller that never thought
/// about signatures gets [`SaveError::SignaturesWouldBeInvalidated`] instead
/// of a silently broken signature. Warning is one call
/// ([`will_invalidate_signatures`]) and proceeding is one field — what is no
/// longer possible is doing neither.
///
/// This is not a block. There is no way to *keep* the signature, so refusing
/// outright would just make signed documents uneditable; the acknowledgement
/// is what turns an invisible loss into a choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignatureAcknowledgement {
    /// Nobody has been told. A save that would break an existing signature
    /// is refused so the caller can ask first.
    #[default]
    Unacknowledged,
    /// The user was told and chose to save anyway. Nothing is checked.
    ProceedAndInvalidate,
}

/// Whether a save materializes the model's `Document::annotations` set into
/// the bytes it produces.
///
/// Every other layer — page structure, page content, form fields, metadata,
/// encryption — is written either way. This names the one layer a shell keeps
/// paintable by itself while pdfium would also draw it, and therefore the one
/// layer that can end up on screen twice; see [`save_preview`] for why form
/// fields are *not* in that category even though they are annotations too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnnotationLayer {
    /// Write it. The only correct answer for bytes that become the document.
    Materialize,
    /// Leave it alone. The base document's own annotations are carried
    /// through untouched — nothing is stripped — but nothing from the model's
    /// own set is appended to them.
    Preserve,
}

/// Auto-selects the incremental or full-rewrite path for `input` and produces
/// the saved bytes.
pub fn save_document(input: SaveInput<'_>) -> Result<Vec<u8>, SaveError> {
    save_document_with_report(input).map(|outcome| outcome.bytes)
}

#[derive(Debug, Clone)]
pub struct SaveOutcome {
    pub bytes: Vec<u8>,
    pub graft_warnings: Vec<pdf_manip::GraftWarning>,
}

pub fn save_document_with_report(input: SaveInput<'_>) -> Result<SaveOutcome, SaveError> {
    save_with_layer(input, SaveOptions::default(), AnnotationLayer::Materialize)
}

/// Produces bytes for an in-memory **preview** of `input` — a buffer the
/// caller reopens, rasterizes and throws away, not a document anyone keeps.
///
/// Identical to [`save_document`] except that the model's `annotations` set is
/// left out of the result. That omission is the entire point of the call.
///
/// A shell previews page operations, content edits and form-field edits this
/// way because all three change what pdfium itself draws, so nothing short of
/// a real reopen shows the actual result. Annotations are the exception: a
/// shell that paints them on an overlay paints them from the same model this
/// save reads, and pdfium rasterizes annotations whenever the file carries
/// them (`FPDF_ANNOT`). Materializing them here would put every one of them on
/// screen *twice* — a highlight over its own copy.
///
/// Nor is having the shell skip the overlay instead a workable alternative for
/// them: once an annotation is in the raster the overlay cannot take it away
/// again, so moving or undoing one after the preview was built would leave the
/// stale copy on screen until the next preview happened to run. Annotation
/// commands deliberately do not trigger a preview refresh — the overlay is
/// already showing the truth — so no later refresh is coming to clean it up.
///
/// **Form fields are written, even though a widget is an annotation.** The
/// difference is not the object, it is who is expected to draw it. A field's
/// appearance is pdfium's to render — the real `/AP`, the real `/DA` font, the
/// real comb and multiline layout are things an overlay can only approximate —
/// so a shell that wants those hands the whole field layer to pdfium and
/// refreshes on every form-field command
/// (`pdf_document::Command::is_form_field_edit`). Because every one of those
/// mutations reaches this function, a field in the raster is never stale for
/// longer than one refresh, which is exactly the property annotations lack.
/// Omitting them here would instead leave the canvas showing the field as the
/// file had it, no matter what the user did to it.
///
/// [`save_document`] stays the only correct call for a real save — bytes on
/// disk with no annotation layer would lose the user's work.
pub fn save_preview(input: SaveInput<'_>) -> Result<Vec<u8>, SaveError> {
    save_preview_with_report(input).map(|outcome| outcome.bytes)
}

/// [`save_preview`] with the graft's report alongside the bytes, the way
/// [`save_document_with_report`] pairs with [`save_document`].
///
/// A preview refresh is the *first* thing that runs after an import, so it is
/// where a shell learns what the graft had to leave behind — dropping the
/// report here would mean the warnings only surfaced on the eventual disk
/// save, long after the user stopped thinking about the import.
pub fn save_preview_with_report(input: SaveInput<'_>) -> Result<SaveOutcome, SaveError> {
    save_with_layer(input, SaveOptions::default(), AnnotationLayer::Preserve)
}

/// Same as [`save_document`], with explicit clock/id-generator hooks — used
/// by CI's determinism check (T-038).
pub fn save_document_with_options(
    input: SaveInput<'_>,
    options: SaveOptions,
) -> Result<Vec<u8>, SaveError> {
    save_with_layer(input, options, AnnotationLayer::Materialize).map(|outcome| outcome.bytes)
}

/// The one save. `layer` says whether the model's annotation layer is written
/// (see [`AnnotationLayer`]); the returned [`SaveOutcome`] carries whatever
/// the graft had to report. The two are independent — a preview can be
/// refused by the same graft that would refuse a real save — so they are a
/// parameter and a return value on the same function rather than two
/// functions that would each have to grow the other's half.
fn save_with_layer(
    input: SaveInput<'_>,
    options: SaveOptions,
    layer: AnnotationLayer,
) -> Result<SaveOutcome, SaveError> {
    // Populated once and threaded into whichever writer runs. Both the writer
    // choice and the writer itself need the base document's *original* page
    // list, and `populate_document` walks every page dictionary to build it —
    // so computing it per-consumer meant a full page walk twice on every
    // save, on both paths.
    let original_pages = bridge::populate_document(input.base)?;

    if !requires_full_rewrite(input, &original_pages) {
        return save_incremental(input, &original_pages, layer).map(|bytes| SaveOutcome {
            bytes,
            graft_warnings: Vec::new(),
        });
    }

    // Only a rewrite can break a signature, and scanning every object for one
    // is not free — so this asks in the one branch where the answer matters,
    // and only when the caller has not already settled it.
    if input.signatures == SignatureAcknowledgement::Unacknowledged
        && pdf_manip::document_has_signatures(input.base)
    {
        return Err(SaveError::SignaturesWouldBeInvalidated);
    }

    save_full_rewrite(input, &options, &original_pages, layer)
}

/// Appends one incremental revision to `original_bytes` using the same
/// `lopdf::IncrementalDocument` writer as the ordinary incremental save path.
///
/// `base` must be the document loaded from `original_bytes`: `lopdf` computes
/// the new revision's `/Prev` offset and object numbering from it, so a
/// mismatched pair produces a structurally broken file. A base that was never
/// parsed from bytes at all (built in memory) is rejected up front; a base
/// parsed from *different* bytes cannot be detected cheaply and remains the
/// caller's contract.
///
/// The callback is responsible for adding or changing only objects in the new
/// revision. The original bytes remain unchanged, which makes this the narrow
/// extension point for operations such as the pdf-sign signature-field append
/// (T-076).
///
/// # Errors
///
/// Returns a [`SaveError`] if `base` was not parsed from a serialized
/// document, if `lopdf` cannot create or serialize the incremental revision,
/// or if `update` returns one.
pub fn append_incremental_update(
    original_bytes: Vec<u8>,
    base: LopdfDocument,
    update: impl FnOnce(&mut IncrementalDocument) -> Result<(), SaveError>,
) -> Result<Vec<u8>, SaveError> {
    let base = base.into_lopdf();
    if base.xref_start == 0 {
        return Err(SaveError::InvalidSaveRequest(
            "append_incremental_update requires a base parsed from original_bytes — a document \
             built in memory has no existing cross-reference table to append to",
        ));
    }

    let mut incremental = IncrementalDocument::create_from(original_bytes, base);
    update(&mut incremental)?;

    let mut bytes = Vec::new();
    incremental.save_to(&mut bytes)?;
    Ok(bytes)
}

fn requires_full_rewrite(input: SaveInput<'_>, original_pages: &[Page]) -> bool {
    input.intent == SaveIntent::StripProtection
        || input.original_bytes.is_none()
        // Batch 21 decision 5: editing a page's content rewrites its stream
        // object, which an incremental append cannot express as a narrow
        // addition. Content edits are structural, always.
        || content::has_content_edits(input.document)
        || bridge::has_structural_page_changes(original_pages, &input.document.pages)
}

/// Whether saving `input` will break a signature the file already carries.
///
/// A full rewrite invalidates existing signatures — that has always been true
/// of structural edits, and page-content editing (Batch 21) makes it reachable
/// from an ordinary text change, which is why it is worth asking about
/// explicitly.
///
/// This is the *query*: it answers the question without attempting a save, so
/// a shell can put the warning in front of the user at the moment they press
/// save rather than after a failed attempt. Answering `true` here is exactly
/// the condition under which [`save_document`] returns
/// [`SaveError::SignaturesWouldBeInvalidated`] for an
/// [`SignatureAcknowledgement::Unacknowledged`] input — the shell warns, the
/// user decides, and the save is re-submitted acknowledged.
///
/// The `signatures` field of `input` is ignored here: this reports what the
/// file and the edits imply, not what the caller has agreed to.
pub fn will_invalidate_signatures(input: SaveInput<'_>) -> Result<bool, SaveError> {
    if !pdf_manip::document_has_signatures(input.base) {
        return Ok(false);
    }

    let original_pages = bridge::populate_document(input.base)?;
    Ok(requires_full_rewrite(input, &original_pages))
}

fn save_full_rewrite(
    input: SaveInput<'_>,
    options: &SaveOptions,
    original_pages: &[Page],
    layer: AnnotationLayer,
) -> Result<SaveOutcome, SaveError> {
    // Resolved once, by the replay itself and against the *replayed*
    // document: page ops have already moved pages around, so this map is the
    // only thing that still connects a model `PageId` to the object it names.
    let bridge::ReplayOutcome {
        document: mut working,
        page_objects: page_ids,
        graft_warnings,
    } = bridge::replay_page_ops(
        input.base,
        original_pages,
        &input.document.pages,
        input.imported_sources,
    )?;

    // Before annotations: content edits are located by re-parsing the page's
    // streams, so they must run while `working` still matches the parse the
    // commands were recorded against.
    content::replay_content_edits(working.as_lopdf_mut(), input.document, &page_ids)?;

    // Skipped for a preview, down to the `/Annots` walk that feeds it — see
    // [`save_preview`] for why a preview must not bake this one layer.
    if layer == AnnotationLayer::Materialize {
        // Read from the materialized document, not from `base`: an imported
        // page is not in `base` at all, and its `/Annots` arrived with the
        // graft.
        let existing_annotations = bridge::page_annotation_objects(&working, &page_ids)?;
        annotations::attach_annotations(
            working.as_lopdf_mut(),
            &page_ids,
            &existing_annotations,
            &input.document.annotations,
        )?;
    }

    // Written either way: a preview exists to show what pdfium will draw, and
    // a form field is one of the things pdfium draws.
    let catalog_id = catalog_object_id(working.as_lopdf())?;
    crate::forms::write_form_fields(
        working.as_lopdf_mut(),
        catalog_id,
        &page_ids,
        &input.document.form_fields,
    )?;

    // Decision 6: an explicit `/ModDate` from `SetDocumentInfo` must win this
    // save over `set_mod_date`'s auto-stamp. Applying the pending
    // `DocumentInfo` first and then telling `set_mod_date` whether an
    // explicit value just landed keeps that a one-way override — a save with
    // no pending metadata edit (`pending_info` is `None`) takes the exact
    // path it always has.
    let pending_info = metadata::pending_document_info(input.document);
    if let Some(info) = pending_info {
        metadata::apply_document_info(working.as_lopdf_mut(), info)?;
    }
    let explicit_mod_date = pending_info.is_some_and(|info| info.mod_date.is_some());
    set_mod_date(
        working.as_lopdf_mut(),
        options.clock.as_ref(),
        explicit_mod_date,
    );
    ensure_trailer_id(working.as_lopdf_mut(), options.id_generator.as_ref());

    security::apply_encryption_for_full_rewrite(
        working.as_lopdf_mut(),
        input.document.security.as_ref(),
        input.intent,
    )?;

    let mut bytes = Vec::new();
    working.as_lopdf_mut().save_to(&mut bytes)?;
    Ok(SaveOutcome {
        bytes,
        graft_warnings,
    })
}

/// `/Info` gains a write path here only when a `SetDocumentInfo` is pending
/// (decision 8, T-171) — `metadata::apply_document_info` uses
/// `ObjectSink::page_dict_mut`, so the incremental writer clones the `/Info`
/// dict into `new_document` before mutating it, the same clone-before-mutate
/// every other trailer-reachable dict this writer touches already gets.
/// Without a pending command, `/Info` is untouched, exactly as before this
/// batch: `new_document.trailer` starts as a full clone of the previous
/// revision's trailer (`Document::new_from_prev`), so an unmodified `/Info`
/// reference simply keeps pointing at the object the previous revision left
/// behind. `/ModDate` is never auto-stamped on this path (unrelated to
/// metadata editing, unchanged by this batch) and the trailer `/ID` is left
/// untouched too — an unmodified `/ModDate`/`/ID` on an incremental update is
/// not a spec violation.
fn save_incremental(
    input: SaveInput<'_>,
    original_pages: &[Page],
    layer: AnnotationLayer,
) -> Result<Vec<u8>, SaveError> {
    let original_bytes = input.original_bytes.ok_or(SaveError::InvalidSaveRequest(
        "incremental save requires original_bytes (a freshly created document has nothing to \
          append to — use save_document instead)",
    ))?;

    if input.intent == SaveIntent::StripProtection {
        return Err(SaveError::InvalidSaveRequest(
            "explicit strip-protection cannot be expressed as an incremental update — an append \
             cannot retroactively decrypt bytes already written in a prior encrypted revision; \
              use save_document instead",
        ));
    }

    // Kept as a precondition check even though `save_document_with_options`
    // only routes non-structural saves here: this is the writer that would
    // silently produce a corrupt append if the contract were ever broken by a
    // future caller.
    if bridge::has_structural_page_changes(original_pages, &input.document.pages) {
        return Err(SaveError::InvalidSaveRequest(
            "structural page changes present — use save_document instead",
        ));
    }

    let page_ids = bridge::page_object_ids(input.base, &input.document.pages)?;
    // `None` for a preview: the annotation set is not written at all, so the
    // `/Annots` walk that feeds it is not paid for either. See
    // [`save_preview`].
    let existing_annotations = match layer {
        AnnotationLayer::Materialize => {
            Some(bridge::page_annotation_objects(input.base, &page_ids)?)
        }
        AnnotationLayer::Preserve => None,
    };
    let catalog_id = catalog_object_id(input.base.as_lopdf())?;
    let pending_info = metadata::pending_document_info(input.document);
    // The one clone the borrowing API cannot remove: lopdf's
    // `IncrementalDocument::create_from` takes both by value.
    append_incremental_update(original_bytes.to_vec(), input.base.clone(), |incremental| {
        if let Some(info) = pending_info {
            metadata::apply_document_info(incremental, info)?;
        }

        for (page_id, rotation) in bridge::rotation_changes(original_pages, &input.document.pages) {
            let page_object_id = *page_ids.get(&page_id).ok_or(SaveError::InvalidSaveRequest(
                "rotation change references a page id absent from the base document",
            ))?;
            let dict: &mut Dictionary = incremental.page_dict_mut(page_object_id)?;
            dict.set(
                "Rotate",
                Object::Integer(i64::from(bridge::rotation_degrees(rotation))),
            );
        }

        if let Some(existing_annotations) = existing_annotations {
            annotations::attach_annotations(
                incremental,
                &page_ids,
                &existing_annotations,
                &input.document.annotations,
            )?;
        }

        crate::forms::write_form_fields(
            incremental,
            catalog_id,
            &page_ids,
            &input.document.form_fields,
        )
    })
}

/// Resolves the catalog's own object id from `/Root` — needed to reach
/// `/AcroForm` (via [`crate::forms::ensure_acroform`]) since neither writer
/// otherwise tracks it once the page/annotation pipeline is done with it.
fn catalog_object_id(doc: &lopdf::Document) -> Result<ObjectId, SaveError> {
    doc.trailer
        .get(b"Root")
        .and_then(|object| object.as_reference())
        .map_err(|_| SaveError::InvalidSaveRequest("document trailer has no valid /Root reference"))
}

/// `explicit_mod_date`: whether this save's pending `SetDocumentInfo` (if
/// any) carried a `Some` `mod_date` — [`crate::metadata::apply_document_info`]
/// already wrote it to `/ModDate` before this runs, and decision 6 says that
/// explicit value wins over the auto-stamp for this save. `true` here is the
/// only thing that changes this function's behavior; everything else is the
/// unmodified auto-stamp this crate has always done.
fn set_mod_date(doc: &mut lopdf::Document, clock: &dyn Clock, explicit_mod_date: bool) {
    if explicit_mod_date {
        return;
    }

    let date = Object::string_literal(clock.pdf_date_string());
    let info_ref = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| o.as_reference().ok());

    match info_ref {
        Some(info_id) => {
            if let Ok(dict) = doc.get_dictionary_mut(info_id) {
                dict.set("ModDate", date);
            }
        }
        None => {
            let mut dict = Dictionary::new();
            dict.set("ModDate", date.clone());
            dict.set("CreationDate", date);
            let info_id = doc.add_object(Object::Dictionary(dict));
            doc.trailer.set("Info", info_id);
        }
    }
}

fn ensure_trailer_id(doc: &mut lopdf::Document, id_generator: &dyn IdGenerator) {
    let existing = doc
        .trailer
        .get(b"ID")
        .ok()
        .and_then(|o| o.as_array().ok())
        .cloned();
    match existing {
        Some(array) if array.len() == 2 => {
            let mut updated = array;
            updated[1] = Object::string_literal(id_generator.next_id());
            doc.trailer.set("ID", updated);
        }
        _ => {
            let id_obj = Object::string_literal(id_generator.next_id());
            doc.trailer.set("ID", vec![id_obj.clone(), id_obj]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{FixedClock, SequentialIdGenerator};
    use pdf_document::{
        AnnotationId, AnnotationKind, Color, Command, Orientation, PageId, PageSize, Rect,
    };

    /// `EditLog::apply` needs `&mut EditLog` and `&mut Document` at once,
    /// which can't both be reached as `document.pending_edits.apply(&mut
    /// document, ..)` (double mutable borrow of the same struct) — the same
    /// take/apply/restore dance `audit_log.rs`'s own tests use.
    fn apply_command(document: &mut Document, command: Command) {
        let mut log = std::mem::take(&mut document.pending_edits);
        log.apply(document, command);
        document.pending_edits = log;
    }

    /// Owns what a shell owns. `SaveInput` only borrows, so the state it
    /// points at has to outlive the save — exactly the shape a real caller
    /// has (`pdf_ffi`'s `DocumentState`, a GTK4 `DocumentSession`).
    struct Fixture {
        document: Document,
        base: LopdfDocument,
        original_bytes: Option<Vec<u8>>,
        intent: SaveIntent,
        signatures: SignatureAcknowledgement,
    }

    impl Fixture {
        fn blank() -> Self {
            let base = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
            let document = bridge::document_from_lopdf(&base, None).unwrap();
            Fixture {
                document,
                base,
                original_bytes: None,
                intent: SaveIntent::Default,
                signatures: SignatureAcknowledgement::Unacknowledged,
            }
        }

        fn input(&self) -> SaveInput<'_> {
            SaveInput {
                document: &self.document,
                base: &self.base,
                original_bytes: self.original_bytes.as_deref(),
                intent: self.intent,
                signatures: self.signatures,
                imported_sources: ImportedSources::none(),
            }
        }

        fn original_pages(&self) -> Vec<Page> {
            bridge::populate_document(&self.base).unwrap()
        }
    }

    /// The one annotation both the save and the preview cases add, so the
    /// only difference between them stays the entry point being tested.
    fn a_highlight() -> pdf_document::Annotation {
        pdf_document::Annotation {
            id: AnnotationId(1),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                color: Color { r: 1, g: 2, b: 3 },
            },
        }
    }

    /// Its form-field twin — `FieldOrigin::New`, the only origin a user can
    /// place and therefore the only one an overlay owns outright.
    fn a_text_field() -> pdf_document::FormField {
        pdf_document::FormField {
            id: pdf_document::FormFieldId(1),
            page: PageId(0),
            name: "Name".to_string(),
            rect: Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 20.0,
            },
            style: pdf_document::TextStyle {
                font: pdf_document::FontFamily::Helvetica,
                size_pt: 12.0,
                color: Color { r: 0, g: 0, b: 0 },
            },
            value: pdf_document::FieldValue::Text("Ada".to_string()),
            kind: pdf_document::FormFieldKind::Text {
                multiline: false,
                max_len: None,
            },
            origin: pdf_document::FieldOrigin::New,
        }
    }

    fn fixed_options() -> SaveOptions {
        SaveOptions {
            clock: Arc::new(FixedClock::new(1_000_000)),
            id_generator: Arc::new(SequentialIdGenerator::new(1)),
        }
    }

    #[test]
    fn blank_document_forces_full_rewrite_with_no_original_bytes() {
        let fixture = Fixture::blank();
        assert!(requires_full_rewrite(
            fixture.input(),
            &fixture.original_pages()
        ));
    }

    #[test]
    fn save_document_rejects_an_imported_page_that_reuses_a_base_page_id() {
        let base = base_with_populated_info();
        let mut document = bridge::document_from_lopdf(&base, None).unwrap();
        document.pages[0] = pdf_document::Page::imported(
            PageId(0),
            pdf_document::ImportedDocumentId(4),
            0,
            PageSize::Letter,
            Orientation::Portrait,
            pdf_document::Rotation::None,
        );
        let fixture = Fixture {
            document,
            base,
            original_bytes: Some(Vec::new()),
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
        };

        let error = save_document(fixture.input()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid save request: imported page names a source this save was not given"
        );
    }

    #[test]
    fn save_document_on_freshly_created_blank_doc_produces_a_reloadable_pdf() {
        let fixture = Fixture::blank();
        let bytes = save_document(fixture.input()).expect("save should succeed");
        let reloaded = lopdf::Document::load_mem(&bytes).expect("output must reload");
        assert_eq!(reloaded.get_pages().len(), 0);
    }

    #[test]
    fn save_document_writes_an_inserted_page_and_annotation() {
        let mut fixture = Fixture::blank();
        let page = pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
        apply_command(&mut fixture.document, Command::insert_page(0, page));
        apply_command(&mut fixture.document, Command::AddAnnotation(a_highlight()));

        let bytes = save_document(fixture.input()).expect("save should succeed");
        let reloaded = lopdf::Document::load_mem(&bytes).expect("output must reload");
        assert_eq!(reloaded.get_pages().len(), 1);

        let page_id = *reloaded.get_pages().get(&1).unwrap();
        let annots = reloaded
            .get_dictionary(page_id)
            .unwrap()
            .get(b"Annots")
            .and_then(|o| o.as_array())
            .unwrap();
        assert_eq!(annots.len(), 1);
    }

    #[test]
    fn save_document_writes_a_new_form_field_that_reads_back() {
        let mut fixture = Fixture::blank();
        let page = pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
        apply_command(&mut fixture.document, Command::insert_page(0, page));
        apply_command(&mut fixture.document, Command::AddFormField(a_text_field()));

        let bytes = save_document(fixture.input()).expect("save should succeed");
        let reloaded = lopdf::Document::load_mem(&bytes).expect("output must reload");

        let fields = pdf_form::read_form_fields(&reloaded);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "Name");
        assert_eq!(
            fields[0].value,
            pdf_document::FieldValue::Text("Ada".to_string())
        );
    }

    // --- Preview saves (the annotation layer a shell paints itself) -------

    /// A one-page base whose page already carries an `/Annots` entry of its
    /// own — an annotation some other PDF editor left in the file, which the
    /// model never sees (`document_from_lopdf` starts with an empty
    /// annotation set). Distinguishes "the preview skipped the model's layer"
    /// from "the preview stripped the page's annotations".
    fn base_with_an_existing_annotation() -> LopdfDocument {
        use lopdf::dictionary;

        let mut doc = lopdf::Document::with_version("1.5");
        let annot_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Square",
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Annots" => vec![Object::Reference(annot_id)],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        LopdfDocument::from_lopdf(doc)
    }

    /// `original_bytes: None` is what forces the full-rewrite writer without
    /// adding a structural change, so the base's single page passes straight
    /// through `replay_page_ops`.
    fn fixture_over(base: LopdfDocument) -> Fixture {
        let document = bridge::document_from_lopdf(&base, None).unwrap();
        Fixture {
            document,
            base,
            original_bytes: None,
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
        }
    }

    fn annots_on_first_page(bytes: &[u8]) -> Vec<Object> {
        let reloaded = lopdf::Document::load_mem(bytes).expect("output must reload");
        let page_id = *reloaded.get_pages().get(&1).expect("saved page 1");
        reloaded
            .get_dictionary(page_id)
            .unwrap()
            .get(b"Annots")
            .map(|object| object.as_array().expect("/Annots must be an array").clone())
            .unwrap_or_default()
    }

    /// The bug this exists to prevent: the shell's overlay paints every model
    /// annotation itself, so a preview that also wrote them made pdfium
    /// rasterize a second copy underneath — a Highlight came out darker than
    /// it should be.
    #[test]
    fn save_preview_leaves_the_model_annotation_layer_out() {
        let mut fixture = fixture_over(base_with_an_existing_annotation());
        apply_command(&mut fixture.document, Command::AddAnnotation(a_highlight()));

        let preview = save_preview(fixture.input()).expect("preview should succeed");
        let saved = save_document(fixture.input()).expect("save should succeed");

        assert_eq!(
            annots_on_first_page(&preview).len(),
            1,
            "a preview must add nothing to the page's own /Annots"
        );
        assert_eq!(
            annots_on_first_page(&saved).len(),
            2,
            "a real save must still write the model annotation"
        );
    }

    /// A widget is an annotation, but it is not in the layer this call
    /// omits. A field's appearance belongs to pdfium — the real `/AP`, the
    /// real `/DA` font — so a preview has to carry it, and a shell that wants
    /// that rendering refreshes on every form-field command
    /// (`Command::is_form_field_edit`) rather than drawing the field itself.
    #[test]
    fn save_preview_writes_a_new_form_field_because_pdfium_draws_it() {
        let mut fixture = fixture_over(base_with_an_existing_annotation());
        apply_command(&mut fixture.document, Command::AddFormField(a_text_field()));

        let preview = save_preview(fixture.input()).expect("preview should succeed");
        let reloaded = lopdf::Document::load_mem(&preview).expect("output must reload");

        let fields = pdf_form::read_form_fields(&reloaded);
        assert_eq!(fields.len(), 1, "the preview must carry the placed field");
        assert_eq!(
            fields[0].value,
            pdf_document::FieldValue::Text("Ada".to_string()),
            "and its value, which is the thing pdfium will rasterize"
        );
    }

    /// The distinction those two turn on, in one place: same preview, one
    /// layer omitted and the other written.
    #[test]
    fn a_preview_omits_annotations_and_keeps_form_fields() {
        let mut fixture = fixture_over(base_with_an_existing_annotation());
        apply_command(&mut fixture.document, Command::AddAnnotation(a_highlight()));
        apply_command(&mut fixture.document, Command::AddFormField(a_text_field()));

        let preview = save_preview(fixture.input()).expect("preview should succeed");
        let reloaded = lopdf::Document::load_mem(&preview).expect("output must reload");

        assert_eq!(
            pdf_form::read_form_fields(&reloaded).len(),
            1,
            "the field is pdfium's to draw"
        );
        assert_eq!(
            annots_on_first_page(&preview).len(),
            2,
            "the page's own annotation plus the field's widget, and not the model highlight"
        );
    }

    /// `Preserve`, not "strip": the annotations the *file* carries are not
    /// the model's layer, the overlay never draws them, and pdfium is the
    /// only thing that can show them at all.
    #[test]
    fn save_preview_keeps_the_base_documents_own_annotations() {
        let fixture = fixture_over(base_with_an_existing_annotation());

        let preview = save_preview(fixture.input()).expect("preview should succeed");

        assert_eq!(
            annots_on_first_page(&preview).len(),
            1,
            "the page's pre-existing /Annots entry must survive a preview"
        );
    }

    /// Page structure is the other thing a preview exists to show, so the
    /// layer choice must not touch it.
    #[test]
    fn save_preview_still_materializes_a_page_operation() {
        let mut fixture = fixture_over(base_with_an_existing_annotation());
        let page = pdf_document::Page::blank(PageId(1), PageSize::A4, Orientation::Portrait);
        apply_command(&mut fixture.document, Command::insert_page(1, page));

        let preview = save_preview(fixture.input()).expect("preview should succeed");
        let reloaded = lopdf::Document::load_mem(&preview).expect("output must reload");

        assert_eq!(
            reloaded.get_pages().len(),
            2,
            "a preview must still write the page op it was asked to show"
        );
    }

    #[test]
    fn full_rewrite_with_fixed_options_is_byte_identical_across_runs() {
        let build_bytes = || {
            let mut fixture = Fixture::blank();
            let page = pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
            apply_command(&mut fixture.document, Command::insert_page(0, page));
            let original_pages = fixture.original_pages();
            save_full_rewrite(
                fixture.input(),
                &fixed_options(),
                &original_pages,
                AnnotationLayer::Materialize,
            )
            .expect("save should succeed")
            .bytes
        };

        let first = build_bytes();
        let second = build_bytes();
        assert_eq!(
            first, second,
            "fixed clock+id-generator must yield byte-identical output"
        );
    }

    #[test]
    fn incremental_save_rejects_structural_changes() {
        let mut fixture = Fixture::blank();
        fixture.original_bytes = Some(vec![]);
        let page = pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
        apply_command(&mut fixture.document, Command::insert_page(0, page));

        let original_pages = fixture.original_pages();
        let result = save_incremental(
            fixture.input(),
            &original_pages,
            AnnotationLayer::Materialize,
        );
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    #[test]
    fn incremental_save_rejects_strip_protection_intent() {
        let mut fixture = Fixture::blank();
        fixture.original_bytes = Some(vec![]);
        fixture.intent = SaveIntent::StripProtection;

        let original_pages = fixture.original_pages();
        let result = save_incremental(
            fixture.input(),
            &original_pages,
            AnnotationLayer::Materialize,
        );
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    #[test]
    fn incremental_save_rejects_missing_original_bytes() {
        let fixture = Fixture::blank(); // original_bytes: None
        let original_pages = fixture.original_pages();
        let result = save_incremental(
            fixture.input(),
            &original_pages,
            AnnotationLayer::Materialize,
        );
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    /// A shell holds `base` and `document` for the whole editing session and
    /// saves repeatedly against them, so a `SaveInput` that *owns* its fields
    /// forces the caller to clone both on every single save — measured at 27%
    /// of the edit round-trip cost on a 52 MiB document (see
    /// `tests/perf_edit_reopen.rs`). This test pins the borrowing contract:
    /// it only compiles if saving leaves the caller's state usable.
    #[test]
    fn saving_borrows_its_input_so_a_shell_can_save_repeatedly() {
        let fixture = Fixture::blank();

        let first = save_document(fixture.input()).expect("first save");
        let second = save_document(fixture.input()).expect("second save");

        let first = lopdf::Document::load_mem(&first).expect("first output must reload");
        let second = lopdf::Document::load_mem(&second).expect("second output must reload");
        assert_eq!(
            first.get_pages().len(),
            second.get_pages().len(),
            "repeated saves of an unchanged input must describe the same document"
        );
    }

    #[test]
    fn append_incremental_update_rejects_a_base_never_parsed_from_bytes() {
        let base = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);

        let result = append_incremental_update(Vec::new(), base, |_| Ok(()));

        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    // --- SetDocumentInfo at save time (B22, T-170) --------------------------

    fn sample_document_info() -> pdf_document::DocumentInfo {
        pdf_document::DocumentInfo {
            title: Some("Contrato".to_string()),
            author: Some("Ada".to_string()),
            ..pdf_document::DocumentInfo::default()
        }
    }

    fn reloaded_info_dict(bytes: &[u8]) -> lopdf::Dictionary {
        let reloaded = lopdf::Document::load_mem(bytes).expect("output must reload");
        let info_id = reloaded
            .trailer
            .get(b"Info")
            .expect("saved document must have an /Info entry")
            .as_reference()
            .expect("/Info must be an indirect reference");
        reloaded.get_dictionary(info_id).unwrap().clone()
    }

    #[test]
    fn a_pending_set_document_info_reaches_the_saved_info_dict() {
        let mut fixture = Fixture::blank();
        apply_command(
            &mut fixture.document,
            Command::SetDocumentInfo {
                before: pdf_document::DocumentInfo::default(),
                after: sample_document_info(),
            },
        );

        let bytes = save_document(fixture.input()).expect("save should succeed");
        let dict = reloaded_info_dict(&bytes);

        assert_eq!(dict.get(b"Title").unwrap().as_str().unwrap(), b"Contrato");
        assert_eq!(dict.get(b"Author").unwrap().as_str().unwrap(), b"Ada");
    }

    /// Decision 6: an explicit `mod_date` from `SetDocumentInfo` wins this
    /// save over `set_mod_date`'s auto-stamp — the fixed clock's timestamp
    /// must not appear.
    #[test]
    fn an_explicit_mod_date_wins_over_the_auto_stamp() {
        let mut fixture = Fixture::blank();
        let explicit = pdf_document::PdfDate::parse("D:20200101000000Z").unwrap();
        apply_command(
            &mut fixture.document,
            Command::SetDocumentInfo {
                before: pdf_document::DocumentInfo::default(),
                after: pdf_document::DocumentInfo {
                    mod_date: Some(explicit),
                    ..pdf_document::DocumentInfo::default()
                },
            },
        );

        let original_pages = fixture.original_pages();
        let bytes = save_full_rewrite(
            fixture.input(),
            &fixed_options(),
            &original_pages,
            AnnotationLayer::Materialize,
        )
        .expect("save should succeed")
        .bytes;
        let dict = reloaded_info_dict(&bytes);

        assert_eq!(
            dict.get(b"ModDate").unwrap().as_str().unwrap(),
            b"D:20200101000000Z",
            "the explicit mod_date must survive, not the fixed clock's stamp"
        );
    }

    /// The common case decision 6 says must not change: no pending
    /// `SetDocumentInfo` at all still auto-stamps `/ModDate` exactly as
    /// before this batch.
    #[test]
    fn without_a_pending_set_document_info_mod_date_is_still_auto_stamped() {
        let fixture = Fixture::blank();

        let original_pages = fixture.original_pages();
        let bytes = save_full_rewrite(
            fixture.input(),
            &fixed_options(),
            &original_pages,
            AnnotationLayer::Materialize,
        )
        .expect("save should succeed")
        .bytes;
        let dict = reloaded_info_dict(&bytes);

        assert!(
            dict.has(b"ModDate"),
            "set_mod_date's unconditional auto-stamp must still run"
        );
    }

    /// T-172 regression: a base document that already carries a fully
    /// populated `/Info` dict, saved through a full rewrite (forced here by
    /// a structural page insert) with no `SetDocumentInfo` pending at all —
    /// `apply_document_info` must never run, so every pre-existing field
    /// must survive byte-for-byte and no field beyond `/ModDate` (the
    /// pre-existing, unrelated auto-stamp) may appear.
    fn base_with_populated_info() -> LopdfDocument {
        use lopdf::{content::Content, content::Operation, dictionary, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let content = Content {
            operations: vec![Operation::new("Tj", vec![Object::string_literal("hola")])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        });
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
            dict.set("Parent", pages_id);
        }
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let mut info = Dictionary::new();
        info.set("Title", Object::string_literal("Original Title"));
        info.set("Author", Object::string_literal("Original Author"));
        info.set("Subject", Object::string_literal("Original Subject"));
        info.set("Keywords", Object::string_literal("orig, keywords"));
        info.set("Creator", Object::string_literal("Original Creator"));
        info.set("Producer", Object::string_literal("Original Producer"));
        info.set("CreationDate", Object::string_literal("D:20200101000000Z"));
        let info_id = doc.add_object(Object::Dictionary(info));
        doc.trailer.set("Info", info_id);

        LopdfDocument::from_lopdf(doc)
    }

    #[test]
    fn full_rewrite_without_pending_document_info_leaves_existing_info_byte_for_byte() {
        let base = base_with_populated_info();
        let document = bridge::document_from_lopdf(&base, None).unwrap();
        let mut fixture = Fixture {
            document,
            base,
            original_bytes: None,
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
        };
        let page = pdf_document::Page::blank(PageId(1), PageSize::A4, Orientation::Portrait);
        apply_command(&mut fixture.document, Command::insert_page(1, page));

        let original_pages = fixture.original_pages();
        let bytes = save_full_rewrite(
            fixture.input(),
            &fixed_options(),
            &original_pages,
            AnnotationLayer::Materialize,
        )
        .expect("save should succeed")
        .bytes;
        let dict = reloaded_info_dict(&bytes);

        assert_eq!(
            dict.get(b"Title").unwrap().as_str().unwrap(),
            b"Original Title"
        );
        assert_eq!(
            dict.get(b"Author").unwrap().as_str().unwrap(),
            b"Original Author"
        );
        assert_eq!(
            dict.get(b"Subject").unwrap().as_str().unwrap(),
            b"Original Subject"
        );
        assert_eq!(
            dict.get(b"Keywords").unwrap().as_str().unwrap(),
            b"orig, keywords"
        );
        assert_eq!(
            dict.get(b"Creator").unwrap().as_str().unwrap(),
            b"Original Creator"
        );
        assert_eq!(
            dict.get(b"Producer").unwrap().as_str().unwrap(),
            b"Original Producer"
        );
        assert_eq!(
            dict.get(b"CreationDate").unwrap().as_str().unwrap(),
            b"D:20200101000000Z"
        );

        let mut keys: Vec<Vec<u8>> = dict.iter().map(|(k, _)| k.clone()).collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                b"Author".to_vec(),
                b"CreationDate".to_vec(),
                b"Creator".to_vec(),
                b"Keywords".to_vec(),
                b"ModDate".to_vec(),
                b"Producer".to_vec(),
                b"Subject".to_vec(),
                b"Title".to_vec(),
            ],
            "no field beyond the pre-existing ModDate auto-stamp may be written on its own"
        );
    }
}
