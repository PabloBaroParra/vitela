//! Page-level operations over `LopdfDocument`: rotate/reorder/extract/delete
//! (T-023). All page numbers taken/returned here are **1-indexed**, matching
//! lopdf's own `Document::get_pages()`/`delete_pages()` convention (spec.md
//! scenarios also speak in 1-indexed terms, e.g. "page 3 deleted").
//!
//! Every function here is a pure transform: it clones the input document and
//! returns a new one, never mutating the caller's handle in place. This
//! matches `pdf_document::EditLog`'s command/inverse model (Batch 2), where
//! the caller retains the pre-op document as the "before" state.

use std::collections::{BTreeMap, HashSet};

use lopdf::{Object, ObjectId};

use crate::document::LopdfDocument;
use crate::error::ManipError;

/// Rotates page `page_number` (1-indexed) clockwise by `delta_degrees`
/// (accumulates with, and is normalized modulo 360 against, any existing
/// `/Rotate` value — mirrors `pdf_document::Rotation::rotated_by`).
pub fn rotate_page(
    document: &LopdfDocument,
    page_number: u32,
    delta_degrees: i32,
) -> Result<LopdfDocument, ManipError> {
    let mut doc = document.0.clone();
    let pages = doc.get_pages();
    let page_id = *pages
        .get(&page_number)
        .ok_or(ManipError::InvalidPageNumber(page_number))?;

    let dict = doc.get_dictionary_mut(page_id)?;
    let current = dict.get(b"Rotate").and_then(|o| o.as_i64()).unwrap_or(0);
    let updated = (current + i64::from(delta_degrees)).rem_euclid(360);
    dict.set("Rotate", updated);

    Ok(LopdfDocument(doc))
}

/// Deletes the given 1-indexed page numbers, preserving all other pages
/// intact (spec "Rotate, Reorder, Extract, Delete Pages").
pub fn delete_pages(
    document: &LopdfDocument,
    page_numbers: &[u32],
) -> Result<LopdfDocument, ManipError> {
    let mut doc = document.0.clone();
    let total = doc.get_pages().len() as u32;

    for &page_number in page_numbers {
        if page_number == 0 || page_number > total {
            return Err(ManipError::InvalidPageNumber(page_number));
        }
    }

    doc.delete_pages(page_numbers);
    doc.prune_objects();
    doc.renumber_objects();

    Ok(LopdfDocument(doc))
}

/// Reorders every page of `document` to the sequence given by `new_order` (a
/// permutation of `1..=page_count`).
pub fn reorder_pages(
    document: &LopdfDocument,
    new_order: &[u32],
) -> Result<LopdfDocument, ManipError> {
    let mut doc = document.0.clone();
    let pages = doc.get_pages();
    let total = pages.len() as u32;

    validate_permutation(new_order, total)?;

    let desired_ids: Vec<ObjectId> = new_order.iter().map(|n| pages[n]).collect();
    reorder_kids_by_object_id(&mut doc, &desired_ids)?;

    Ok(LopdfDocument(doc))
}

/// Produces a new document containing exactly the pages in `page_numbers`
/// (1-indexed, into the *original* document), in the given order — used
/// directly by callers wanting an "extract" op, and as the shared
/// implementation behind [`crate::split`].
pub fn extract_pages(
    document: &LopdfDocument,
    page_numbers: &[u32],
) -> Result<LopdfDocument, ManipError> {
    if page_numbers.is_empty() {
        return Err(ManipError::EmptyPageSelection);
    }

    let mut doc = document.0.clone();
    let old_pages = doc.get_pages();
    let total = old_pages.len() as u32;

    for &page_number in page_numbers {
        if !old_pages.contains_key(&page_number) {
            return Err(ManipError::InvalidPageNumber(page_number));
        }
    }

    let selected_ids: Vec<ObjectId> = page_numbers.iter().map(|n| old_pages[n]).collect();
    let to_delete: Vec<u32> = (1..=total).filter(|n| !page_numbers.contains(n)).collect();

    doc.delete_pages(&to_delete);
    reorder_kids_by_object_id(&mut doc, &selected_ids)?;
    doc.prune_objects();
    doc.renumber_objects();

    Ok(LopdfDocument(doc))
}

/// Validates that `order` is a permutation of `1..=total` (used by
/// `reorder_pages`).
fn validate_permutation(order: &[u32], total: u32) -> Result<(), ManipError> {
    if order.len() as u32 != total {
        return Err(ManipError::InvalidPageOrder);
    }

    let mut seen = HashSet::with_capacity(order.len());
    for &n in order {
        if n == 0 || n > total || !seen.insert(n) {
            return Err(ManipError::InvalidPageOrder);
        }
    }

    Ok(())
}

/// Resequences each page's parent `/Kids` array so its children matching
/// `desired_order` appear in that relative order.
///
/// Limitation (documented, not enforced): assumes a flat page tree where
/// every page in `desired_order` shares a `/Parent` whose `/Kids` contains
/// only page objects also present in `desired_order` (true for `pdf-manip`'s
/// own `create_blank_document`/`insert_blank_page` output and every fixture
/// used by Batch 4's tests). A deeply nested page tree with mixed
/// page/sub-tree kids is out of scope for Batch 4.
fn reorder_kids_by_object_id(
    doc: &mut lopdf::Document,
    desired_order: &[ObjectId],
) -> Result<(), ManipError> {
    let mut parent_children: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();

    for &page_id in desired_order {
        let parent = doc
            .get_dictionary(page_id)?
            .get(b"Parent")
            .and_then(|o| o.as_reference())
            .map_err(|_| ManipError::MalformedPageTree)?;
        parent_children.entry(parent).or_default().push(page_id);
    }

    for (parent_id, children) in parent_children {
        let parent_dict = doc.get_dictionary_mut(parent_id)?;
        parent_dict.set(
            "Kids",
            children
                .into_iter()
                .map(Object::Reference)
                .collect::<Vec<_>>(),
        );
    }

    Ok(())
}
