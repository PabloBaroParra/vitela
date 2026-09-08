//! What a destination means: turning a `/Dest` value into the page it names
//! (checklist "Estructuras de documento", `docs/batch-pdf-assembly.md`
//! section 4, items 4-6).
//!
//! Where those values *sit* — on a link annotation, on an outline entry — and
//! what an import does about each is [`crate::links`]'s question.
//!
//! ## Why an explicit destination is free and a named one is not
//!
//! A link reaches a page in one of two ways. An **explicit** destination
//! names the page object directly — `/Dest [12 0 R /XYZ null 700 null]` —
//! and survives a graft for nothing, because [`crate::graft_pages`] renumbers
//! the whole source once up front and then keeps every grafted page at its
//! renumbered id. The reference still lands on the same page dictionary,
//! wherever the destination document decides to put it in its page tree.
//!
//! A **named** destination names a string or a name instead — `/Dest
//! (chapter2)` — and the map from that key to a page lives in the source
//! *catalog*: `/Names /Dests` as a name tree, or the PDF 1.1 `/Dests`
//! dictionary (PDF 32000-1:2008 section 12.3.2.3). The catalog is precisely
//! what an import must not copy — it carries the source's page tree, its
//! `/Encrypt` policy and its metadata, none of which may govern the document
//! the user already has open. So the key would arrive pointing at nothing.
//!
//! The policy is therefore: **resolve the name against the source, while the
//! source is still at hand, and write the explicit destination it resolves to
//! into the imported link.** A name is not carried over as a name; it is
//! carried over as the page reference it meant.
//!
//! ## What cannot be rewritten
//!
//! Two cases have no correct rewrite, and both are reported rather than
//! silently left behind (see [`crate::GraftWarning`]):
//!
//! - the destination resolves to a page that was **not** selected. Following
//!   it would drag that page, and transitively most of the source, in behind
//!   one link. The reference is left pointing at an absent object, which
//!   section 7.3.10 defines as a reference to null — an inert link, not a
//!   corrupt one.
//! - the name resolves to nothing at all, because the source never defined
//!   it. It was already broken before the import and stays broken.
//!

use lopdf::{Dictionary, Document as LopdfRawDocument, Object, ObjectId};

/// Cap on nested name-tree nodes and `/Dest` indirections followed before the
/// structure is treated as malformed or cyclic. Same reasoning as the
/// inheritance cap in [`crate::page_graph`]: real files nest a handful of
/// levels, and the cap is what stops a cycle from hanging the import.
const MAX_TREE_DEPTH: usize = 32;

/// What a `/Dest` value — or a GoTo action's `/D` — turned out to name.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DestinationTarget {
    /// A page of this document, together with the explicit destination array
    /// that names it. `explicit` is what a named destination is rewritten to,
    /// so the view parameters the name carried (`/XYZ`, `/FitH`, …) are kept
    /// rather than replaced with a guess.
    Page { page: ObjectId, explicit: Object },
    /// A named destination neither the `/Names /Dests` name tree nor the
    /// legacy `/Dests` dictionary defines.
    UnresolvedName(String),
    /// Not a destination naming a page of this document: a `/GoToR` into
    /// another file, a page *number* instead of a reference, or a malformed
    /// value.
    Other,
}

/// Resolves `value` — an annotation's `/Dest`, an action's `/D`, or an
/// outline item's destination — to the page it names.
pub(crate) fn resolve_destination(doc: &LopdfRawDocument, value: &Object) -> DestinationTarget {
    resolve_at_depth(doc, value, 0)
}

fn resolve_at_depth(doc: &LopdfRawDocument, value: &Object, depth: usize) -> DestinationTarget {
    if depth >= MAX_TREE_DEPTH {
        return DestinationTarget::Other;
    }
    match value {
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(object) => resolve_at_depth(doc, object, depth + 1),
            Err(_) => DestinationTarget::Other,
        },
        Object::Array(items) => match items.first().map(Object::as_reference) {
            Some(Ok(page)) => DestinationTarget::Page {
                page,
                explicit: Object::Array(items.clone()),
            },
            _ => DestinationTarget::Other,
        },
        // A destination may be wrapped as `<< /D [...] >>` — the shape every
        // entry in a `/Names /Dests` name tree is allowed to take.
        Object::Dictionary(dict) => match dict.get(b"D") {
            Ok(inner) => resolve_at_depth(doc, inner, depth + 1),
            Err(_) => DestinationTarget::Other,
        },
        Object::Name(key) | Object::String(key, _) => match lookup_named(doc, key) {
            Some(found) => resolve_at_depth(doc, &found, depth + 1),
            None => DestinationTarget::UnresolvedName(String::from_utf8_lossy(key).into_owned()),
        },
        _ => DestinationTarget::Other,
    }
}

/// True when `value` names a destination rather than giving one directly —
/// the only case a graft has to rewrite, because the map it depends on is
/// left behind with the catalog.
pub(crate) fn is_named_destination(value: &Object) -> bool {
    matches!(value, Object::Name(_) | Object::String(_, _))
}

/// Looks a destination name up in the source catalog: the `/Names /Dests`
/// name tree first, then the PDF 1.1 `/Dests` dictionary a producer may have
/// written instead.
fn lookup_named(doc: &LopdfRawDocument, key: &[u8]) -> Option<Object> {
    let catalog = doc.catalog().ok()?;
    let from_name_tree = catalog
        .get(b"Names")
        .ok()
        .and_then(|names| dictionary_at(doc, names))
        .and_then(|names| names.get(b"Dests").ok().cloned())
        .and_then(|root| name_tree_lookup(doc, &root, key, 0));
    if from_name_tree.is_some() {
        return from_name_tree;
    }
    catalog
        .get(b"Dests")
        .ok()
        .and_then(|dests| dictionary_at(doc, dests))
        .and_then(|dests| dests.get(key).ok().cloned())
}

/// Searches a name tree (PDF 32000-1:2008 section 7.9.6) for `key`.
///
/// `/Limits` is deliberately ignored and every branch is searched: it is an
/// ordering hint a producer is free to get wrong, and a lookup that trusted a
/// wrong hint would silently report a destination as unresolved — exactly the
/// quiet data loss this section forbids.
fn name_tree_lookup(
    doc: &LopdfRawDocument,
    node: &Object,
    key: &[u8],
    depth: usize,
) -> Option<Object> {
    if depth >= MAX_TREE_DEPTH {
        return None;
    }
    let dict = dictionary_at(doc, node)?;
    if let Some(names) = dict
        .get(b"Names")
        .ok()
        .and_then(|value| array_at(doc, value))
    {
        // A leaf's `/Names` is a flat `[ (key) dest (key) dest … ]` array.
        for pair in names.chunks(2) {
            let [name, destination] = pair else { continue };
            let matches = match name {
                Object::Name(bytes) | Object::String(bytes, _) => bytes.as_slice() == key,
                _ => false,
            };
            if matches {
                return Some(destination.clone());
            }
        }
    }
    let kids = dict
        .get(b"Kids")
        .ok()
        .and_then(|value| array_at(doc, value))?;
    kids.iter()
        .find_map(|kid| name_tree_lookup(doc, kid, key, depth + 1))
}

/// Resolves a value that should be a dictionary, following one indirect
/// reference.
pub(crate) fn dictionary_at(doc: &LopdfRawDocument, value: &Object) -> Option<Dictionary> {
    match value {
        Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
        Object::Dictionary(dict) => Some(dict.clone()),
        _ => None,
    }
}

/// Resolves a value that should be an array, following one indirect
/// reference.
pub(crate) fn array_at(doc: &LopdfRawDocument, value: &Object) -> Option<Vec<Object>> {
    match value {
        Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok().cloned(),
        Object::Array(items) => Some(items.clone()),
        _ => None,
    }
}
