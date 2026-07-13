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
use pdf_document::Document;
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

/// Bundled input for a save.
///
/// `original_bytes` is `None` for a freshly created document
/// (`create_blank_document`, never yet saved) and `Some(..)` for a document
/// opened from an existing file — required by the incremental writer, unused
/// by the full-rewrite writer.
pub struct SaveInput {
    pub document: Document,
    pub base: LopdfDocument,
    pub original_bytes: Option<Vec<u8>>,
    pub intent: SaveIntent,
}

/// Auto-selects the incremental or full-rewrite path for `input` and produces
/// the saved bytes.
pub fn save_document(input: SaveInput) -> Result<Vec<u8>, SaveError> {
    save_document_with_options(input, SaveOptions::default())
}

/// Same as [`save_document`], with explicit clock/id-generator hooks — used
/// by CI's determinism check (T-038).
pub fn save_document_with_options(
    input: SaveInput,
    options: SaveOptions,
) -> Result<Vec<u8>, SaveError> {
    if requires_full_rewrite(&input)? {
        save_full_rewrite(&input, &options)
    } else {
        save_incremental(&input)
    }
}

fn requires_full_rewrite(input: &SaveInput) -> Result<bool, SaveError> {
    if input.intent == SaveIntent::StripProtection || input.original_bytes.is_none() {
        return Ok(true);
    }
    let original_pages = bridge::populate_document(&input.base)?;
    Ok(bridge::has_structural_page_changes(
        &original_pages,
        &input.document.pages,
    ))
}

fn save_full_rewrite(input: &SaveInput, options: &SaveOptions) -> Result<Vec<u8>, SaveError> {
    let original_pages = bridge::populate_document(&input.base)?;
    let mut working = bridge::replay_page_ops(&input.base, &original_pages, &input.document.pages)?;

    let page_ids = bridge::page_object_ids(&working, &input.document.pages)?;
    let existing_annotations = bridge::page_annotation_objects(&input.base)?;
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
fn save_incremental(input: &SaveInput) -> Result<Vec<u8>, SaveError> {
    let original_bytes = input
        .original_bytes
        .as_ref()
        .ok_or(SaveError::InvalidSaveRequest(
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

    let original_pages = bridge::populate_document(&input.base)?;
    if bridge::has_structural_page_changes(&original_pages, &input.document.pages) {
        return Err(SaveError::InvalidSaveRequest(
            "structural page changes present — use save_document instead",
        ));
    }

    let mut incremental =
        IncrementalDocument::create_from(original_bytes.clone(), input.base.as_lopdf().clone());

    let page_ids = bridge::page_object_ids(&input.base, &input.document.pages)?;
    let existing_annotations = bridge::page_annotation_objects(&input.base)?;

    for (page_id, rotation) in bridge::rotation_changes(&original_pages, &input.document.pages) {
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
        &mut incremental,
        &page_ids,
        &existing_annotations,
        &input.document.annotations,
    )?;

    let mut bytes = Vec::new();
    incremental.save_to(&mut bytes)?;
    Ok(bytes)
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

    fn blank_input() -> SaveInput {
        let base = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
        let document = bridge::document_from_lopdf(&base, None).unwrap();
        SaveInput {
            document,
            base,
            original_bytes: None,
            intent: SaveIntent::Default,
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
        let input = blank_input();
        assert!(requires_full_rewrite(&input).unwrap());
    }

    #[test]
    fn save_document_on_freshly_created_blank_doc_produces_a_reloadable_pdf() {
        let input = blank_input();
        let bytes = save_document(input).expect("save should succeed");
        let reloaded = lopdf::Document::load_mem(&bytes).expect("output must reload");
        assert_eq!(reloaded.get_pages().len(), 0);
    }

    #[test]
    fn save_document_writes_an_inserted_page_and_annotation() {
        let mut input = blank_input();
        let page = pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
        apply_command(&mut input.document, Command::InsertPage { index: 0, page });
        apply_command(
            &mut input.document,
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

        let bytes = save_document(input).expect("save should succeed");
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
            let mut input = blank_input();
            let page = pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
            apply_command(&mut input.document, Command::InsertPage { index: 0, page });
            save_full_rewrite(&input, &fixed_options()).expect("save should succeed")
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
        let mut input = blank_input();
        input.original_bytes = Some(vec![]);
        let page = pdf_document::Page::blank(PageId(0), PageSize::A4, Orientation::Portrait);
        apply_command(&mut input.document, Command::InsertPage { index: 0, page });

        let result = save_incremental(&input);
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    #[test]
    fn incremental_save_rejects_strip_protection_intent() {
        let mut input = blank_input();
        input.original_bytes = Some(vec![]);
        input.intent = SaveIntent::StripProtection;

        let result = save_incremental(&input);
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    #[test]
    fn incremental_save_rejects_missing_original_bytes() {
        let input = blank_input(); // original_bytes: None
        let result = save_incremental(&input);
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }
}
