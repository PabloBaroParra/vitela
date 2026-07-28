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

use std::sync::Arc;

use lopdf::{Dictionary, IncrementalDocument, Object};
use pdf_document::{Document, Page};
use pdf_manip::LopdfDocument;

use crate::annotations::{self, ObjectSink};
use crate::bridge;
use crate::clock::{Clock, IdGenerator, RandomIdGenerator, SystemClock};
use crate::error::SaveError;
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
}

/// Auto-selects the incremental or full-rewrite path for `input` and produces
/// the saved bytes.
pub fn save_document(input: SaveInput<'_>) -> Result<Vec<u8>, SaveError> {
    save_document_with_options(input, SaveOptions::default())
}

/// Same as [`save_document`], with explicit clock/id-generator hooks — used
/// by CI's determinism check (T-038).
pub fn save_document_with_options(
    input: SaveInput<'_>,
    options: SaveOptions,
) -> Result<Vec<u8>, SaveError> {
    // Populated once and threaded into whichever writer runs. Both the writer
    // choice and the writer itself need the base document's *original* page
    // list, and `populate_document` walks every page dictionary to build it —
    // so computing it per-consumer meant a full page walk twice on every
    // save, on both paths.
    let original_pages = bridge::populate_document(input.base)?;

    if requires_full_rewrite(input, &original_pages) {
        save_full_rewrite(input, &options, &original_pages)
    } else {
        save_incremental(input, &original_pages)
    }
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
        || bridge::has_structural_page_changes(original_pages, &input.document.pages)
}

fn save_full_rewrite(
    input: SaveInput<'_>,
    options: &SaveOptions,
    original_pages: &[Page],
) -> Result<Vec<u8>, SaveError> {
    let mut working = bridge::replay_page_ops(input.base, original_pages, &input.document.pages)?;

    let page_ids = bridge::page_object_ids(&working, &input.document.pages)?;
    let existing_annotations = bridge::page_annotation_objects(input.base)?;
    annotations::attach_annotations(
        working.as_lopdf_mut(),
        &page_ids,
        &existing_annotations,
        &input.document.annotations,
    )?;

    set_mod_date(working.as_lopdf_mut(), options.clock.as_ref());
    ensure_trailer_id(working.as_lopdf_mut(), options.id_generator.as_ref());

    security::apply_encryption_for_full_rewrite(
        working.as_lopdf_mut(),
        input.document.security.as_ref(),
        input.intent,
    )?;

    let mut bytes = Vec::new();
    working.as_lopdf_mut().save_to(&mut bytes)?;
    Ok(bytes)
}

/// Deliberately leaves `/Info`/`/ModDate` and the trailer `/ID` untouched: an
/// incremental update's `new_document.trailer` starts as a clone of the
/// previous revision's trailer (see `lopdf::Document::new_from_prev`), so
/// `/Info`/`/ID` already carry over pointing at objects that still live in
/// the previous, unmodified revision. Updating them correctly would require
/// cloning the `/Info` dict into `new_document` first (mirroring
/// `page_dict_mut`'s clone-before-mutate pattern) — deferred as a follow-up;
/// the saved output remains a fully valid PDF either way (an unmodified
/// `/ModDate` on an incremental update is not a spec violation).
fn save_incremental(input: SaveInput<'_>, original_pages: &[Page]) -> Result<Vec<u8>, SaveError> {
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
    let existing_annotations = bridge::page_annotation_objects(input.base)?;
    // The one clone the borrowing API cannot remove: lopdf's
    // `IncrementalDocument::create_from` takes both by value.
    append_incremental_update(original_bytes.to_vec(), input.base.clone(), |incremental| {
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

        annotations::attach_annotations(
            incremental,
            &page_ids,
            &existing_annotations,
            &input.document.annotations,
        )
    })
}

fn set_mod_date(doc: &mut lopdf::Document, clock: &dyn Clock) {
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
            }
        }

        fn input(&self) -> SaveInput<'_> {
            SaveInput {
                document: &self.document,
                base: &self.base,
                original_bytes: self.original_bytes.as_deref(),
                intent: self.intent,
            }
        }

        fn original_pages(&self) -> Vec<Page> {
            bridge::populate_document(&self.base).unwrap()
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
        apply_command(
            &mut fixture.document,
            Command::InsertPage { index: 0, page },
        );
        apply_command(
            &mut fixture.document,
            Command::AddAnnotation(pdf_document::Annotation {
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
            }),
        );

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
    fn full_rewrite_with_fixed_options_is_byte_identical_across_runs() {
        let build_bytes = || {
            let mut fixture = Fixture::blank();
            let page = pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
            apply_command(
                &mut fixture.document,
                Command::InsertPage { index: 0, page },
            );
            let original_pages = fixture.original_pages();
            save_full_rewrite(fixture.input(), &fixed_options(), &original_pages)
                .expect("save should succeed")
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
        apply_command(
            &mut fixture.document,
            Command::InsertPage { index: 0, page },
        );

        let original_pages = fixture.original_pages();
        let result = save_incremental(fixture.input(), &original_pages);
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    #[test]
    fn incremental_save_rejects_strip_protection_intent() {
        let mut fixture = Fixture::blank();
        fixture.original_bytes = Some(vec![]);
        fixture.intent = SaveIntent::StripProtection;

        let original_pages = fixture.original_pages();
        let result = save_incremental(fixture.input(), &original_pages);
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    #[test]
    fn incremental_save_rejects_missing_original_bytes() {
        let fixture = Fixture::blank(); // original_bytes: None
        let original_pages = fixture.original_pages();
        let result = save_incremental(fixture.input(), &original_pages);
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
}
