//! The bytes a boundary reopens in pdfium to show a session's unsaved edits
//! — the pdfium-free half of `refresh_preview`.
//!
//! This used to live only in `pdf-ffi`'s `refresh_preview`; `pdf-wasm` needs
//! the same snapshot to keep its thumbnails and pages current after a move,
//! a removal or an import. Reopening the bytes stays with each boundary,
//! because that is where its pdfium lives.
//!
//! ## Not [`crate::save_preview`]
//!
//! Both leave the session's annotations out, for the same reason: every
//! shell draws those on its own overlay. They do it differently, and the
//! outputs are not interchangeable, so this keeps what `pdf-ffi` has always
//! done rather than switching to `save_preview`:
//!
//! - this empties the model's annotation set on a clone and runs an ordinary
//!   [`crate::save_document`], so the writer still walks every page's
//!   `/Annots` and rewrites it with nothing added;
//! - `save_preview` skips that walk and leaves `/Annots` untouched.
//!
//! The two can differ in the bytes they write (an incremental update that
//! re-emits a page's `/Annots` or not) and in what they refuse (a malformed
//! `/Annots` the walk meets). Switching would be a behaviour change for every
//! FFI shell, which this crate's callers have not asked for.

use pdf_document::Document;
use pdf_manip::LopdfDocument;

use crate::bridge::ImportedSources;
use crate::error::SaveError;
use crate::security::SaveIntent;
use crate::strategy::{save_document, SaveInput, SignatureAcknowledgement};

/// The session as the next save would write it, minus the session's own
/// annotations, as bytes to reopen and rasterize.
///
/// `document` is cloned, not touched: its edit log and annotations — the
/// work a real save is computed from — stay exactly as they were, and the
/// next real save is still computed from `base` and `original_bytes`.
///
/// Only the session's annotations are dropped. Annotations the file already
/// carried are never in that set (`document_from_lopdf` starts it empty) and
/// reach the snapshot through `base` like the rest of the page, so they keep
/// rendering exactly once.
///
/// A signature the file carries is acknowledged silently: this produces
/// pixels, not a file. Anything a reader could keep still goes through
/// [`save_document`], which refuses an unacknowledged signature-breaking
/// save.
pub fn session_preview(
    document: &Document,
    base: &LopdfDocument,
    original_bytes: Option<&[u8]>,
    imported_sources: ImportedSources<'_, '_>,
) -> Result<Vec<u8>, SaveError> {
    let mut preview = document.clone();
    preview.annotations = Default::default();
    save_document(SaveInput {
        document: &preview,
        base,
        original_bytes,
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::ProceedAndInvalidate,
        imported_sources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::document_from_lopdf;
    use lopdf::Object;
    use pdf_document::{
        Annotation, AnnotationId, AnnotationKind, Color, Command, Orientation, PageId, PageSize,
        Rect,
    };

    fn apply(document: &mut Document, command: Command) {
        let mut log = std::mem::take(&mut document.pending_edits);
        assert!(log.apply(document, command), "applies");
        document.pending_edits = log;
    }

    fn one_page() -> LopdfDocument {
        pdf_manip::insert_blank_page(
            &pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait),
            0,
            PageSize::A4,
            Orientation::Portrait,
        )
        .expect("one blank page")
    }

    /// A one-page file as opened from its bytes — the shape the
    /// incremental writer needs to append to.
    fn opened_one_page() -> (LopdfDocument, Vec<u8>) {
        let mut bytes = Vec::new();
        one_page()
            .as_lopdf()
            .clone()
            .save_to(&mut bytes)
            .expect("serializes");
        let base = LopdfDocument::from_lopdf(lopdf::Document::load_mem(&bytes).expect("parses"));
        (base, bytes)
    }

    fn highlight() -> Command {
        Command::AddAnnotation(Annotation {
            id: AnnotationId(1),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: Rect {
                    x: 10.0,
                    y: 10.0,
                    width: 50.0,
                    height: 20.0,
                },
                color: Color { r: 255, g: 0, b: 0 },
            },
        })
    }

    fn has_highlight(bytes: &[u8]) -> bool {
        lopdf::Document::load_mem(bytes)
            .expect("parses")
            .objects
            .values()
            .any(|object| {
                object.as_dict().ok().is_some_and(|dict| {
                    dict.get(b"Subtype")
                        .ok()
                        .and_then(|subtype| subtype.as_name().ok())
                        == Some(b"Highlight".as_slice())
                })
            })
    }

    fn page_count(bytes: &[u8]) -> usize {
        lopdf::Document::load_mem(bytes)
            .expect("parses")
            .get_pages()
            .len()
    }

    #[test]
    fn the_session_annotations_stay_out_of_the_preview_and_in_the_session() {
        let (base, original) = opened_one_page();
        let mut document = document_from_lopdf(&base, None).expect("model");
        apply(&mut document, highlight());

        let preview = session_preview(&document, &base, Some(&original), ImportedSources::none())
            .expect("previews");

        assert!(!has_highlight(&preview));
        assert_eq!(document.annotations.iter().count(), 1, "session untouched");
        // The premise: a real save of the same session does write it.
        let saved = save_document(SaveInput {
            document: &document,
            base: &base,
            original_bytes: Some(&original),
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
            imported_sources: ImportedSources::none(),
        })
        .expect("saves");
        assert!(has_highlight(&saved));
    }

    #[test]
    fn the_preview_shows_the_pending_page_structure() {
        let (base, original) = opened_one_page();
        let mut document = document_from_lopdf(&base, None).expect("model");
        let insert =
            Command::insert_blank_page(&mut document, 1, PageSize::A4, Orientation::Portrait)
                .expect("ids to spare");
        apply(&mut document, insert);

        let preview = session_preview(&document, &base, Some(&original), ImportedSources::none())
            .expect("previews");

        assert_eq!(page_count(&preview), 2);
    }

    /// A signed file previews without asking: the bytes become pixels, and
    /// the real save still refuses until the reader is told.
    #[test]
    fn a_signed_file_previews_without_an_acknowledgement() {
        let mut base = one_page();
        let raw = base.as_lopdf_mut();
        let form = raw.add_object(lopdf::dictionary! { "SigFlags" => 3 });
        let root = raw
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .expect("root");
        raw.get_dictionary_mut(root)
            .expect("catalog")
            .set("AcroForm", Object::Reference(form));
        assert!(pdf_manip::document_has_signatures(&base));
        let document = document_from_lopdf(&base, None).expect("model");

        // No original bytes: the full-rewrite writer, where the signature
        // question is asked.
        assert!(session_preview(&document, &base, None, ImportedSources::none()).is_ok());
        let refused = save_document(SaveInput {
            document: &document,
            base: &base,
            original_bytes: None,
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
            imported_sources: ImportedSources::none(),
        });
        assert!(matches!(
            refused,
            Err(SaveError::SignaturesWouldBeInvalidated)
        ));
    }
}
