//! Detaching a page from the tree it came from, and walking everything it
//! reaches.
//!
//! Split out of [`crate::graft`] so that both halves of the import can share
//! it: the copy itself needs the page's object graph to know what to bring
//! along, and [`crate::report`] needs the same graph to answer "what would
//! this import cost?" *before* a single object is copied. Two different
//! traversals of the same page would eventually disagree; one cannot.

use std::collections::{BTreeSet, HashSet};

use lopdf::{Dictionary, Document as LopdfRawDocument, Object, ObjectId};

use crate::error::ManipError;

/// How far up a `/Parent` chain an inherited attribute is looked for before
/// the page tree is treated as malformed. Mirrors `pdf-edit`'s own cap on the
/// same walk: real files nest a handful of levels, and a cap is what keeps a
/// cyclic `/Parent` from hanging the import.
const MAX_INHERITANCE_DEPTH: usize = 32;

/// Page attributes a page may inherit from an ancestor `/Pages` node
/// (PDF 32000-1:2008 section 7.7.3.4).
const INHERITABLE_ATTRIBUTES: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// The page's own dictionary with every inherited attribute written onto it
/// and `/Parent` removed — a page that no longer needs the tree it came from.
///
/// `/Parent` is dropped rather than rewritten here so the traversal that
/// follows cannot walk back up into the source's page tree; the caller sets
/// the destination's root once the copy is done.
pub(crate) fn flattened_page(
    donor: &LopdfRawDocument,
    page_id: ObjectId,
) -> Result<Dictionary, ManipError> {
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
/// `selected` (see [`crate::graft`]'s docs on why the link is left inert
/// instead) and the source's own page-tree nodes and catalog, which have no
/// business governing the destination.
pub(crate) fn collect_reachable(
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
