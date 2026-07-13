//! Merge and split operations over `LopdfDocument`s (T-022), plus the
//!
//! `merge`'s algorithm follows lopdf's own documented merge recipe
//! (`examples/merge.rs` in the lopdf source): renumber each source
//! document's objects into a disjoint id range (via
//! `Document::renumber_objects_with`, which also reorders a document's own
//! pages to ascend in traversal order first), collect pages/catalog/pages-
//! root separately from other objects, then rebuild a single pages tree and
//! catalog. Bookmark/outline merging (present in the lopdf example) is out
//! of scope — no spec requirement covers a merged table of contents.

use std::collections::BTreeMap;

use crate::document::LopdfDocument;
use crate::error::ManipError;
use crate::page_ops::extract_pages;
use lopdf::{Document as LopdfRawDocument, Object, ObjectId};

/// Merges `documents` in order, preserving each document's own page order
/// and the given slice order across documents (spec "Merge Documents").
pub fn merge(documents: &[LopdfDocument]) -> Result<LopdfDocument, ManipError> {
    if documents.is_empty() {
        return Err(ManipError::EmptyMerge);
    }

    let mut max_id = 1u32;
    let mut merged_pages: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut merged_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();

    for source in documents {
        let mut doc = source.0.clone();
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        for (_, page_object_id) in doc.get_pages() {
            if let Ok(object) = doc.get_object(page_object_id) {
                merged_pages.insert(page_object_id, object.clone());
            }
        }

        merged_objects.extend(doc.objects);
    }

    let mut merged = LopdfRawDocument::with_version("1.5");
    let mut catalog: Option<(ObjectId, lopdf::Dictionary)> = None;
    let mut pages_root: Option<(ObjectId, lopdf::Dictionary)> = None;

    for (object_id, object) in merged_objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                if catalog.is_none() {
                    catalog = object.as_dict().ok().map(|dict| (object_id, dict.clone()));
                }
            }
            b"Pages" => {
                if pages_root.is_none() {
                    pages_root = object.as_dict().ok().map(|dict| (object_id, dict.clone()));
                }
            }
            b"Page" => {} // re-parented to the merged pages root below
            _ => {
                merged.objects.insert(object_id, object);
            }
        }
    }

    let (pages_id, mut pages_dict) = pages_root.ok_or(ManipError::MalformedPageTree)?;
    let (catalog_id, mut catalog_dict) = catalog.ok_or(ManipError::MalformedPageTree)?;

    for (page_id, page_object) in &merged_pages {
        if let Ok(dict) = page_object.as_dict() {
            let mut dict = dict.clone();
            dict.set("Parent", pages_id);
            merged.objects.insert(*page_id, Object::Dictionary(dict));
        }
    }

    pages_dict.set("Count", merged_pages.len() as i64);
    pages_dict.set(
        "Kids",
        merged_pages
            .keys()
            .map(|&id| Object::Reference(id))
            .collect::<Vec<_>>(),
    );
    merged
        .objects
        .insert(pages_id, Object::Dictionary(pages_dict));

    catalog_dict.set("Pages", pages_id);
    merged
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dict));

    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.len() as u32;
    merged.renumber_objects();

    Ok(LopdfDocument(merged))
}

/// Splits `document` after page `after_page` (1-indexed) into two documents:
/// pages `1..=after_page` and pages `(after_page + 1)..=total` (spec "Split
/// Document"). Both halves must end up non-empty.
pub fn split(
    document: &LopdfDocument,
    after_page: u32,
) -> Result<(LopdfDocument, LopdfDocument), ManipError> {
    let total = document.0.get_pages().len() as u32;
    if after_page == 0 || after_page >= total {
        return Err(ManipError::InvalidPageRange {
            after_page,
            total_pages: total,
        });
    }

    let left_pages: Vec<u32> = (1..=after_page).collect();
    let right_pages: Vec<u32> = (after_page + 1..=total).collect();

    let left = extract_pages(document, &left_pages)?;
    let right = extract_pages(document, &right_pages)?;

    Ok((left, right))
}
