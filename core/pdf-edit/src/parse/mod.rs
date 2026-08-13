//! Content-stream parsing (T-152) and the lazy read API (T-149).
//!
//! [`read_page_content`] is the entry point a shell calls when the user
//! opens content-edit mode on a page — **not** at document open. Batch
//! decision 2: interpreting every page's content stream up front is wasted
//! work for the majority of sessions that never edit page content, so
//! `Document` never carries a `page_content` field and nothing here is
//! cached.

mod filter;
pub(crate) use filter::encode_flate;
pub mod interpreter;
pub mod lexer;
pub mod matrix;

pub use interpreter::{LocatedContent, LocatedImage, LocatedTextRun, PageStream};
pub use lexer::{tokenize, Operand, SpannedOperation};
pub use matrix::Matrix;

use crate::error::EditError;
use lopdf::{Dictionary, Document, Object, ObjectId};
use pdf_document::{PageContent, PageId};

/// Reads the text runs and images painted by `page`, on demand.
///
/// The returned ids are assigned in stream order and are **only meaningful
/// against the document revision they came from**: they are positions in a
/// parse, not stored identities, because page content is not addressable in
/// the file. Re-read after a save before acting on ids again.
///
/// `page` is resolved **positionally**, which is the identity
/// `pdf-save`'s bridge hands out for an unmodified document. Once pages have
/// been deleted, reordered or inserted, position and `PageId` part ways —
/// see [`page_object_id`].
pub fn read_page_content(document: &Document, page: PageId) -> Result<PageContent, EditError> {
    let page_object = page_object_id(document, page)?;
    Ok(read_located_content(document, page_object)?.page_content(page))
}

/// The same read, keeping the byte locations [`crate::edit`] needs.
///
/// Takes the page's **object id**, not its `PageId`: every editing entry
/// point in this crate does, because a `PageId` is a position in the page
/// tree and a save that reorders or deletes pages moves it out from under
/// the commands recorded against it. The caller that knows the mapping —
/// `pdf-save`, which owns the page replay — resolves it.
pub(crate) fn read_located_content(
    document: &Document,
    page_object: ObjectId,
) -> Result<LocatedContent, EditError> {
    let page_dict = document.get_dictionary(page_object)?;
    let resources = page_resources(document, page_dict);
    let streams = page_streams(document, page_dict)?;

    interpreter::interpret(document, &resources, &streams)
}

/// Maps a `PageId` — a zero-based position, the same identity
/// `pdf-save`'s bridge assigns when it populates a `Document` — to the
/// object it names.
///
/// **Only valid while the document's page order still matches the one the
/// `PageId`s were assigned from.** A document that has had pages deleted,
/// reordered or inserted needs the mapping its page replay produced
/// (`pdf_save::bridge::page_object_ids`), which is why the editing API takes
/// object ids rather than calling this itself.
pub fn page_object_id(document: &Document, page: PageId) -> Result<ObjectId, EditError> {
    document
        .get_pages()
        .values()
        .nth(page.0 as usize)
        .copied()
        .ok_or(EditError::PageNotFound(page))
}

/// The page's resource dictionary, inherited from an ancestor `/Pages` node
/// when the page itself does not carry one — inheritance is normal in files
/// produced by tools that share resources across pages.
fn page_resources(document: &Document, page_dict: &Dictionary) -> Dictionary {
    let mut current = page_dict.clone();

    for _ in 0..MAX_INHERITANCE_DEPTH {
        if let Ok(resources) = current.get(b"Resources") {
            if let Some(Object::Dictionary(dict)) = dereference(document, resources) {
                return dict.clone();
            }
        }
        let Ok(Object::Reference(parent_id)) = current.get(b"Parent") else {
            break;
        };
        let Ok(parent) = document.get_dictionary(*parent_id) else {
            break;
        };
        current = parent.clone();
    }

    Dictionary::new()
}

/// A `/Parent` chain in a valid file is a handful of levels deep; this cap
/// exists so a file with a cyclic one cannot hang the reader.
const MAX_INHERITANCE_DEPTH: usize = 32;

/// The page's content streams, decoded, in the order they concatenate.
pub(crate) fn page_streams(
    document: &Document,
    page_dict: &Dictionary,
) -> Result<Vec<PageStream>, EditError> {
    let Ok(contents) = page_dict.get(b"Contents") else {
        return Ok(Vec::new());
    };

    let object_ids: Vec<ObjectId> = match contents {
        Object::Reference(id) => vec![*id],
        Object::Array(items) => items
            .iter()
            .filter_map(|item| match item {
                Object::Reference(id) => Some(*id),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };

    object_ids
        .into_iter()
        .map(|object_id| {
            let stream = document.get_object(object_id)?.as_stream()?;
            let decoded = filter::decode(document, stream, object_id)?;
            Ok(PageStream {
                object_id,
                bytes: decoded.bytes,
                filtered: decoded.filtered,
            })
        })
        .collect()
}

fn dereference<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        direct => Some(direct),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use pdf_document::ContentItemId;

    #[test]
    fn reading_a_page_that_does_not_exist_says_so() {
        let (document, _) = fixture::document_with_content(b"", Dictionary::new());

        assert_eq!(
            read_page_content(&document, PageId(7)),
            Err(EditError::PageNotFound(PageId(7)))
        );
    }

    #[test]
    fn a_page_with_no_contents_reads_as_empty() {
        let (mut document, page_id) =
            fixture::document_with_content(b"", fixture::helvetica_resources());
        document
            .get_dictionary_mut(page_id)
            .expect("page dictionary")
            .remove(b"Contents");

        let content = read_page_content(&document, PageId(0)).expect("readable page");

        assert!(content.text_runs.is_empty() && content.images.is_empty());
    }

    /// A stream that declares `/FlateDecode` over bytes that are not deflate
    /// is a damaged file. Reading it as if the raw bytes were operators
    /// yields a page that looks empty — and the danger is what happens next:
    /// an edit would write those still-encoded bytes back as the page's
    /// plain content and re-compress them, turning a damaged page into an
    /// unrecoverable one while the save reports success.
    #[test]
    fn a_stream_claiming_a_filter_it_does_not_decode_with_is_an_error() {
        let (mut document, page_object) = fixture::document_with_content(
            b"BT /F1 12 Tf (a) Tj ET",
            fixture::helvetica_resources(),
        );
        let contents_id = match document
            .get_dictionary(page_object)
            .expect("page dictionary")
            .get(b"Contents")
        {
            Ok(Object::Reference(id)) => *id,
            _ => panic!("the fixture uses a single content stream"),
        };
        let stream = document
            .get_object_mut(contents_id)
            .expect("content stream")
            .as_stream_mut()
            .expect("content stream");
        stream.dict.set("Filter", "FlateDecode");

        let error = read_page_content(&document, PageId(0)).expect_err("the bytes do not decode");

        assert_eq!(
            error,
            EditError::UndecodableContentStream {
                object_id: contents_id
            }
        );
    }

    /// The case the check must not break: a stream with no filter at all.
    /// `decompressed_content` errors on those too, and treating that as a
    /// damaged file would make every plain page unreadable.
    #[test]
    fn a_stream_with_no_filter_reads_its_bytes_as_the_content() {
        let (document, _) = fixture::document_with_content(
            b"BT /F1 12 Tf (a) Tj ET",
            fixture::helvetica_resources(),
        );

        assert_eq!(
            read_page_content(&document, PageId(0))
                .expect("readable page")
                .text_runs
                .len(),
            1
        );
    }

    #[test]
    fn read_page_content_returns_the_pure_model() {
        let (document, _) = fixture::document_with_content(
            b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET",
            fixture::helvetica_resources(),
        );

        let content = read_page_content(&document, PageId(0)).expect("readable page");

        assert_eq!(
            content.text_run(ContentItemId(0)).expect("one run").text,
            "Hello"
        );
    }

    /// Resources live on an ancestor `/Pages` node often enough that a
    /// reader which only looks at the page itself finds no fonts and
    /// silently reports a page with no text.
    #[test]
    fn resources_inherited_from_the_pages_node_are_found() {
        let (mut document, page_id) = fixture::document_with_content(
            b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET",
            fixture::helvetica_resources(),
        );
        let page_dict = document
            .get_dictionary_mut(page_id)
            .expect("page dictionary");
        let resources = page_dict.get(b"Resources").expect("resources").clone();
        page_dict.remove(b"Resources");
        let parent_id = match document
            .get_dictionary(page_id)
            .expect("page dictionary")
            .get(b"Parent")
        {
            Ok(Object::Reference(id)) => *id,
            _ => panic!("the fixture page has a parent"),
        };
        document
            .get_dictionary_mut(parent_id)
            .expect("pages node")
            .set("Resources", resources);

        let content = read_page_content(&document, PageId(0)).expect("readable page");

        assert_eq!(content.text_runs.len(), 1);
    }
}
