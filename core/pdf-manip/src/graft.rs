//! Grafting: copying real pages out of one document and into another
//! (checklist "Importación PDF").
//!
//! ## Why this is not `merge`
//!
//! [`crate::merge`] builds a *new* document out of several sources: it takes
//! whichever catalog and page-tree root it happens to see first and rebuilds
//! the tree around them. That is right for "combine these files into a new
//! one" and wrong for importing into a document the user already has open —
//! the destination's catalog, its `/Encrypt` policy, its `/Info`, its object
//! ids and its existing pages all have to survive untouched, because an
//! `EditLog` full of commands is still keyed to them.
//!
//! So this walks the other way round: the destination is cloned as-is, and
//! only the selected pages plus the object graph they reach are lifted out of
//! the source and dropped into it.
//!
//! ## What comes along, and what does not
//!
//! Everything the page dictionary can reach comes along — content streams,
//! fonts, images and other XObjects, colour spaces, patterns, shadings,
//! `/ExtGState`, `/Group`, and the page's own `/Annots` with their appearance
//! streams. Attributes the page only *inherited* (`/Resources`, `/MediaBox`,
//! `/CropBox`, `/Rotate` — PDF 32000-1:2008 section 7.7.3.4) are materialized
//! onto the page first, because the `/Pages` node they were inherited from is
//! deliberately left behind.
//!
//! What does not come along: the source's catalog, its page-tree nodes, its
//! trailer, its `/Encrypt` dictionary and its document-level metadata. None
//! of those may govern the destination.
//!
//! ## References into pages that were not selected
//!
//! A link annotation on an imported page can name a destination on a page
//! that was *not* imported. Following it would drag that page — and
//! transitively most of the source — in behind it, which is not what the user
//! asked for. The traversal therefore stops at any page object outside the
//! selection, and the reference is left pointing at an object that is not
//! there. PDF 32000-1:2008 section 7.3.10 defines exactly that case: a
//! reference to an object that does not exist is a reference to null, so the
//! link is inert rather than corrupt.
//!
//! Turning that inert link into something better — remapping destinations
//! that *were* imported, and telling the user about the ones that were not —
//! is the bookmarks-and-destinations policy work, not this operation's job.

use std::collections::{BTreeSet, HashSet};

use lopdf::{Dictionary, Document as LopdfRawDocument, Object, ObjectId};

use crate::create_blank::root_pages_id;
use crate::document::LopdfDocument;
use crate::error::ManipError;

/// How far up a `/Parent` chain an inherited attribute is looked for before
/// the page tree is treated as malformed. Mirrors `pdf-edit`'s own cap on the
/// same walk: real files nest a handful of levels, and a cap is what keeps a
/// cyclic `/Parent` from hanging the import.
const MAX_INHERITANCE_DEPTH: usize = 32;

/// Page attributes a page may inherit from an ancestor `/Pages` node
/// (PDF 32000-1:2008 section 7.7.3.4).
const INHERITABLE_ATTRIBUTES: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Copies the pages of `source` named by the 0-based indices in `pages` into
/// `document`, at 0-based position `index`, preserving their real PDF content.
///
/// The grafted pages land contiguously, in `pages` order, so they occupy
/// `index..index + pages.len()` in the returned document; every page the
/// destination already had keeps its content, its object id and its relative
/// order. `index` is clamped to the destination's page count, so appending is
/// any `index` at or past the end — the same convention
/// [`crate::insert_blank_page`] uses.
///
/// Fails without touching anything when the selection is empty, names a page
/// the source does not have, or names one page twice; `document` is borrowed,
/// so a failed graft cannot leave a half-imported document behind.
pub fn graft_pages(
    document: &LopdfDocument,
    index: usize,
    source: &LopdfDocument,
    pages: &[usize],
) -> Result<LopdfDocument, ManipError> {
    if pages.is_empty() {
        return Err(ManipError::EmptyPageSelection);
    }
    let mut seen = HashSet::with_capacity(pages.len());
    if let Some(&duplicate) = pages.iter().find(|page| !seen.insert(**page)) {
        return Err(ManipError::DuplicatePageSelection(duplicate));
    }

    let mut doc = document.0.clone();
    let destination_root = root_pages_id(&doc)?;

    // Renumbered into a range that starts above every id the destination
    // uses, so nothing copied below can land on an object already there.
    // This is the whole answer to "avoid id collisions between documents":
    // do it once, up front, rather than remapping reference by reference.
    let mut donor = source.0.clone();
    donor.renumber_objects_with(doc.max_id + 1);

    let donor_pages: Vec<ObjectId> = donor.get_pages().into_values().collect();
    // Resolved before a single object is copied, so an out-of-range index is
    // refused rather than discovered halfway through.
    let selected: Vec<ObjectId> = pages
        .iter()
        .map(|&page| {
            donor_pages
                .get(page)
                .copied()
                .ok_or(ManipError::InvalidPageIndex(page))
        })
        .collect::<Result<_, _>>()?;
    let selected_set: HashSet<ObjectId> = selected.iter().copied().collect();

    // Each page is flattened first and walked afterwards: materializing an
    // inherited `/Resources` pulls a reference into the page dictionary that
    // the traversal then has to follow, and doing it the other way round
    // would leave the resources behind.
    let mut grafted: Vec<(ObjectId, Dictionary)> = Vec::with_capacity(selected.len());
    let mut reachable: BTreeSet<ObjectId> = BTreeSet::new();
    for &page_id in &selected {
        let dict = flattened_page(&donor, page_id)?;
        collect_reachable(&donor, &dict, &selected_set, &mut reachable);
        grafted.push((page_id, dict));
    }

    for object_id in reachable {
        if let Ok(object) = donor.get_object(object_id) {
            doc.objects.insert(object_id, object.clone());
        }
    }
    for (page_id, mut dict) in grafted {
        // Set last: the traversal above must not follow it, and by now every
        // object the page reaches is already in the destination.
        dict.set("Parent", destination_root);
        doc.objects.insert(page_id, Object::Dictionary(dict));
    }
    doc.max_id = doc.max_id.max(donor.max_id);

    let pages_dict = doc.get_dictionary_mut(destination_root)?;
    let mut kids = pages_dict
        .get(b"Kids")
        .and_then(|kids| kids.as_array())
        .cloned()
        .unwrap_or_default();
    let insert_at = index.min(kids.len());
    for (offset, &page_id) in selected.iter().enumerate() {
        kids.insert(insert_at + offset, Object::Reference(page_id));
    }
    let count = kids.len() as i64;
    pages_dict.set("Kids", kids);
    pages_dict.set("Count", count);

    Ok(LopdfDocument(doc))
}

/// The page's own dictionary with every inherited attribute written onto it
/// and `/Parent` removed — a page that no longer needs the tree it came from.
///
/// `/Parent` is dropped rather than rewritten here so the traversal that
/// follows cannot walk back up into the source's page tree; the caller sets
/// the destination's root once the copy is done.
fn flattened_page(donor: &LopdfRawDocument, page_id: ObjectId) -> Result<Dictionary, ManipError> {
    let mut dict = donor.get_dictionary(page_id)?.clone();
    for attribute in INHERITABLE_ATTRIBUTES {
        if dict.get(attribute).is_ok() {
            continue;
        }
        if let Some(value) = inherited_attribute(donor, page_id, attribute) {
            dict.set(attribute.to_vec(), value);
        }
    }
    dict.remove(b"Parent");
    Ok(dict)
}

/// Walks the page's `/Parent` chain for the nearest ancestor that sets
/// `attribute`, or `None` when nothing in the chain does.
fn inherited_attribute(
    donor: &LopdfRawDocument,
    page_id: ObjectId,
    attribute: &[u8],
) -> Option<Object> {
    let mut current = donor.get_dictionary(page_id).ok()?.clone();
    for _ in 0..MAX_INHERITANCE_DEPTH {
        let parent = current
            .get(b"Parent")
            .and_then(|value| value.as_reference())
            .ok()?;
        let dict = donor.get_dictionary(parent).ok()?;
        if let Ok(value) = dict.get(attribute) {
            return Some(value.clone());
        }
        current = dict.clone();
    }
    None
}

/// Adds every object `dict` can reach to `reachable`, following indirect
/// references transitively.
///
/// Stops at the two kinds of object that must not be copied: a page outside
/// `selected` (see this module's docs on why the link is left inert instead)
/// and the source's own page-tree nodes and catalog, which have no business
/// governing the destination.
fn collect_reachable(
    donor: &LopdfRawDocument,
    dict: &Dictionary,
    selected: &HashSet<ObjectId>,
    reachable: &mut BTreeSet<ObjectId>,
) {
    for (_, value) in dict.iter() {
        collect_from_object(donor, value, selected, reachable);
    }
}

fn collect_from_object(
    donor: &LopdfRawDocument,
    value: &Object,
    selected: &HashSet<ObjectId>,
    reachable: &mut BTreeSet<ObjectId>,
) {
    match value {
        Object::Reference(id) => {
            if reachable.contains(id) {
                return;
            }
            let Ok(object) = donor.get_object(*id) else {
                return;
            };
            match object.type_name().unwrap_or_default() {
                // A selected page is written by the caller in its flattened
                // form, so it is neither copied nor descended into here — and
                // an unselected one is exactly what must not be followed.
                b"Page" | b"Pages" | b"Catalog" => return,
                _ => {}
            }
            if selected.contains(id) {
                return;
            }
            reachable.insert(*id);
            collect_from_object(donor, object, selected, reachable);
        }
        Object::Array(items) => {
            for item in items {
                collect_from_object(donor, item, selected, reachable);
            }
        }
        Object::Dictionary(dict) => collect_reachable(donor, dict, selected, reachable),
        // A stream's dictionary carries references of its own — `/Length` as
        // an indirect object, a soft-mask image, a form XObject's own
        // `/Resources`.
        Object::Stream(stream) => collect_reachable(donor, &stream.dict, selected, reachable),
        _ => {}
    }
}
