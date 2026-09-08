//! Who carries a destination, and what an import does with it (checklist
//! "Estructuras de documento", `docs/batch-pdf-assembly.md` section 4,
//! items 4-6).
//!
//! Two kinds of object point at a page: a **link annotation** on a page, and
//! an **outline entry** (a bookmark) in the source catalog. They get opposite
//! treatment, and for the same reason.
//!
//! A link annotation travels with the page it sits on, so its destination can
//! be fixed up: [`destination_slots`] finds every one an imported page
//! carries and [`set_destination_value`] writes the corrected value back.
//! What that value *should* be is [`crate::destinations`]'s question.
//!
//! An outline entry does not travel at all — the tree hangs off the source
//! catalog and is ordered against the source's own page order, so importing
//! it would be inventing a structure nobody asked for. It cannot be fixed;
//! it can only be counted, which is what [`outline_entries_into`] does, and
//! counting only the entries that pointed *into* the selection is what keeps
//! a source with a big unrelated table of contents from crying wolf.
//!
//! One annotation shape cannot be fixed either: one written *inline* in a
//! page's `/Annots` array rather than as an indirect object has no object of
//! its own to write back to. [`inline_named_destinations`] reads those for
//! reporting, so a named destination on one is reported as dropped instead of
//! being quietly carried over dead.

use std::collections::HashSet;

use lopdf::{Dictionary, Document as LopdfRawDocument, Object, ObjectId};

use crate::destinations::{array_at, dictionary_at, resolve_destination, DestinationTarget};

/// Cap on outline levels walked before the tree is treated as malformed.
/// Same reasoning as [`crate::destinations`]'s own cap: real files nest a
/// handful of levels, and the cap is what stops a cycle from hanging the
/// import.
const MAX_OUTLINE_DEPTH: usize = 32;

/// Where a destination sits on a page, so it can be read now and written back
/// after the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DestinationSlot {
    /// `/Dest` on the annotation object itself.
    Annotation(ObjectId),
    /// `/D` on a GoTo action written inline in the annotation's `/A`.
    InlineAction(ObjectId),
    /// `/D` on a GoTo action that is its own indirect object.
    Action(ObjectId),
}

/// Every rewritable destination `page` carries, in `/Annots` order.
///
/// An annotation written inline in `/Annots` is skipped here — it has no
/// object id to write back to. [`inline_named_destinations`] reports those
/// instead, so they are never silently dropped.
pub(crate) fn destination_slots(doc: &LopdfRawDocument, page: &Dictionary) -> Vec<DestinationSlot> {
    let mut slots = Vec::new();
    for annot in page_annotations(doc, page) {
        let Ok(annot_id) = annot.as_reference() else {
            continue;
        };
        let Ok(dict) = doc.get_dictionary(annot_id) else {
            continue;
        };
        if dict.get(b"Dest").is_ok() {
            slots.push(DestinationSlot::Annotation(annot_id));
        }
        match dict.get(b"A") {
            Ok(Object::Reference(action_id)) => {
                if is_goto_action(doc.get_dictionary(*action_id).ok()) {
                    slots.push(DestinationSlot::Action(*action_id));
                }
            }
            Ok(Object::Dictionary(action)) if is_goto_action(Some(action)) => {
                slots.push(DestinationSlot::InlineAction(annot_id));
            }
            _ => {}
        }
    }
    slots
}

/// The names carried by destinations on annotations written inline in
/// `/Annots`, which cannot be rewritten and so do not survive the import.
pub(crate) fn inline_named_destinations(doc: &LopdfRawDocument, page: &Dictionary) -> Vec<String> {
    let mut names = Vec::new();
    for annot in page_annotations(doc, page) {
        let Object::Dictionary(dict) = annot else {
            continue;
        };
        let candidate = dict.get(b"Dest").ok().or_else(|| {
            dict.get(b"A")
                .ok()
                .and_then(|action| action.as_dict().ok())
                .filter(|action| is_goto_action(Some(action)))
                .and_then(|action| action.get(b"D").ok())
        });
        if let Some(Object::Name(key) | Object::String(key, _)) = candidate {
            names.push(String::from_utf8_lossy(key).into_owned());
        }
    }
    names
}

/// The current value at `slot`, or `None` when the object it names is gone.
pub(crate) fn destination_value(doc: &LopdfRawDocument, slot: DestinationSlot) -> Option<Object> {
    match slot {
        DestinationSlot::Annotation(id) => doc.get_dictionary(id).ok()?.get(b"Dest").ok(),
        DestinationSlot::Action(id) => doc.get_dictionary(id).ok()?.get(b"D").ok(),
        DestinationSlot::InlineAction(annot_id) => doc
            .get_dictionary(annot_id)
            .ok()?
            .get(b"A")
            .ok()?
            .as_dict()
            .ok()?
            .get(b"D")
            .ok(),
    }
    .cloned()
}

/// Writes `value` at `slot`. Silently does nothing when the object is not
/// there: the copy decides what exists, and a slot the copy did not bring
/// along has nothing to fix.
pub(crate) fn set_destination_value(
    doc: &mut LopdfRawDocument,
    slot: DestinationSlot,
    value: Object,
) {
    match slot {
        DestinationSlot::Annotation(id) => {
            if let Ok(dict) = doc.get_dictionary_mut(id) {
                dict.set("Dest", value);
            }
        }
        DestinationSlot::Action(id) => {
            if let Ok(dict) = doc.get_dictionary_mut(id) {
                dict.set("D", value);
            }
        }
        DestinationSlot::InlineAction(annot_id) => {
            if let Ok(annot) = doc.get_dictionary_mut(annot_id) {
                if let Ok(action) = annot.get_mut(b"A").and_then(Object::as_dict_mut) {
                    action.set("D", value);
                }
            }
        }
    }
}

/// How many of the source's outline entries (bookmarks) point at a page in
/// `selected`.
///
/// The source's outline tree is never imported: it hangs off the source
/// catalog, it is ordered against the source's own page order, and merging it
/// into the destination's outline would be inventing a structure the user did
/// not ask for. Counting the entries that pointed *into* the selection is
/// what turns that policy from a silent omission into a reported one — and
/// counting only those is what keeps a source with a large unrelated table of
/// contents from crying wolf.
pub(crate) fn outline_entries_into(doc: &LopdfRawDocument, selected: &HashSet<ObjectId>) -> usize {
    let Some(root) = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"Outlines").ok())
        .and_then(|outlines| dictionary_at(doc, outlines))
    else {
        return 0;
    };
    let mut visited = HashSet::new();
    let mut count = 0;
    count_outline_level(
        doc,
        root.get(b"First").ok().cloned(),
        selected,
        &mut visited,
        &mut count,
        0,
    );
    count
}

fn count_outline_level(
    doc: &LopdfRawDocument,
    first: Option<Object>,
    selected: &HashSet<ObjectId>,
    visited: &mut HashSet<ObjectId>,
    count: &mut usize,
    depth: usize,
) {
    if depth >= MAX_OUTLINE_DEPTH {
        return;
    }
    let mut next = first;
    while let Some(value) = next {
        // A sibling chain is followed by object id so a `/Next` that loops
        // back on itself terminates instead of spinning.
        let Ok(item_id) = value.as_reference() else {
            return;
        };
        if !visited.insert(item_id) {
            return;
        }
        let Ok(item) = doc.get_dictionary(item_id) else {
            return;
        };
        if let Some(destination) = outline_destination(doc, item) {
            if let DestinationTarget::Page { page, .. } = resolve_destination(doc, &destination) {
                if selected.contains(&page) {
                    *count += 1;
                }
            }
        }
        count_outline_level(
            doc,
            item.get(b"First").ok().cloned(),
            selected,
            visited,
            count,
            depth + 1,
        );
        next = item.get(b"Next").ok().cloned();
    }
}

/// An outline item's destination, whether written as `/Dest` or as a GoTo
/// action under `/A`.
fn outline_destination(doc: &LopdfRawDocument, item: &Dictionary) -> Option<Object> {
    if let Ok(dest) = item.get(b"Dest") {
        return Some(dest.clone());
    }
    let action = item
        .get(b"A")
        .ok()
        .and_then(|value| dictionary_at(doc, value))?;
    if !is_goto_action(Some(&action)) {
        return None;
    }
    action.get(b"D").ok().cloned()
}

/// A page's `/Annots` entries, resolved through an indirect `/Annots` array.
/// Empty when the page has none or the array cannot be read.
fn page_annotations(doc: &LopdfRawDocument, page: &Dictionary) -> Vec<Object> {
    page.get(b"Annots")
        .ok()
        .and_then(|value| array_at(doc, value))
        .unwrap_or_default()
}

/// True for a `/S /GoTo` action — the only action kind whose destination
/// names a page of *this* document. `/GoToR` and `/GoToE` point into another
/// file and are left exactly as they are.
fn is_goto_action(action: Option<&Dictionary>) -> bool {
    action.and_then(|dict| dict.get(b"S").and_then(|value| value.as_name()).ok())
        == Some(b"GoTo".as_slice())
}
