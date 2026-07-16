//! Document creation and blank-page insert/remove (T-024): builds a valid,
//! empty PDF skeleton and lets callers add/remove blank pages. See
//! spec.md "Create Blank Document" and design.md's "Document creation (new)".
//!
//! Indexing convention: `insert_blank_page`/`remove_page` take a
//! **0-indexed** `usize` position, matching `pdf_document::EditLog`'s
//! `Command::InsertPage { index }` / `RemovePage { index }` (Vec-based,
//! Batch 2) — deliberately different from this crate's sibling `page_ops`
//! module (1-indexed page numbers, matching lopdf's own convention), since
//! these two functions exist specifically to mirror the EditLog command
//! shape for the "create blank document" authoring flow.

use lopdf::content::Content;
use lopdf::{dictionary, Document as LopdfRawDocument, Object, Stream};
use pdf_document::{Orientation, PageSize};

use crate::document::{oriented_dimensions, LopdfDocument};
use crate::error::ManipError;

/// Creates a new, valid, empty PDF (zero pages) with the given default page
/// size/orientation recorded on the page-tree root (inherited by pages that
/// don't set their own `/MediaBox`) — spec "Create Blank Document".
pub fn create_blank_document(size: PageSize, orientation: Orientation) -> LopdfDocument {
    let (width, height) = oriented_dimensions(size, orientation);

    let mut doc = LopdfRawDocument::with_version("1.5");
    let pages_id = doc.new_object_id();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<Object>::new(),
            "Count" => 0,
            "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
        }),
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    LopdfDocument(doc)
}

/// Inserts a new blank page at 0-indexed position `index` (clamped to the
/// current page count if out of range, so appending is `index =
/// page_count()`).
pub fn insert_blank_page(
    document: &LopdfDocument,
    index: usize,
    size: PageSize,
    orientation: Orientation,
) -> Result<LopdfDocument, ManipError> {
    let mut doc = document.0.clone();
    let (width, height) = oriented_dimensions(size, orientation);
    let pages_id = root_pages_id(&doc)?;

    let resources_id = doc.add_object(dictionary! {});
    let content = Content { operations: vec![] };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Resources" => resources_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
    });

    let pages_dict = doc.get_dictionary_mut(pages_id)?;
    let mut kids = pages_dict
        .get(b"Kids")
        .and_then(|o| o.as_array())
        .cloned()
        .unwrap_or_default();
    let insert_at = index.min(kids.len());
    kids.insert(insert_at, Object::Reference(page_id));
    let count = kids.len() as i64;
    pages_dict.set("Kids", kids);
    pages_dict.set("Count", count);

    Ok(LopdfDocument(doc))
}

/// Removes the page at 0-indexed position `index`.
pub fn remove_page(document: &LopdfDocument, index: usize) -> Result<LopdfDocument, ManipError> {
    let mut doc = document.0.clone();
    let total = doc.get_pages().len();
    if index >= total {
        return Err(ManipError::InvalidPageIndex(index));
    }

    doc.delete_pages(&[(index + 1) as u32]);
    doc.prune_objects();
    doc.renumber_objects();

    Ok(LopdfDocument(doc))
}

/// Resolves the document's page-tree root via `/Root/Pages` in the catalog.
pub(crate) fn root_pages_id(doc: &lopdf::Document) -> Result<lopdf::ObjectId, ManipError> {
    doc.catalog()?
        .get(b"Pages")
        .and_then(|o| o.as_reference())
        .map_err(|_| ManipError::MalformedPageTree)
}
