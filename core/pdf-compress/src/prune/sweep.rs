//! What the trailer cannot reach.
//!
//! ## The rule this module exists to keep
//!
//! *Reachability from the trailer is the only definition of "in use", and the
//! only licence to delete.*
//!
//! Mark from every reference the trailer holds, follow them transitively,
//! delete what was never marked. No object type is special-cased — not the
//! dead cross-reference stream that started T-192, not an orphaned object
//! stream, not a `/Linearized` dictionary left over from a producer that
//! linearised. They all go for the same reason: nothing points at them.
//!
//! ## The second half of the leak: the id space
//!
//! Deleting the objects is not enough on its own, and this is the part that
//! is easy to miss. `lopdf`'s writer mints its new object stream and
//! cross-reference stream at `max_id + 1`, and `max_id` only ever goes *up*.
//! Load a packed document, delete those two objects as unreachable, and
//! `max_id` still claims they are the top of the file — so the next write
//! mints two ids *above* them, and the cross-reference table has to carry two
//! free entries for ids nothing has ever used. Two more on every round trip.
//!
//! Measured: with the deletion alone, a repacked document still grew five
//! bytes each time it was repacked. [`reclaim_id_space`] is what removes the
//! last of the non-idempotence.

use lopdf::{Dictionary, Document, Object, ObjectId};

use std::collections::HashSet;

/// Deletes every object the trailer cannot reach, gives back the top of the
/// id space, and says how many objects went.
pub(super) fn pass(document: &mut Document, roots: &[ObjectId]) -> usize {
    let dropped = delete_unreachable(document, roots);
    reclaim_id_space(document);
    dropped
}

/// Every object the trailer points at, or `None` when it does not name a
/// catalog.
///
/// `None` is not an error, it is a refusal to guess: without a `/Root` there
/// is nothing to be reachable *from*, and a sweep would delete the whole
/// document. The caller leaves such a file alone.
pub(super) fn trailer_references(document: &Document) -> Option<Vec<ObjectId>> {
    document
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|root| root.as_reference().ok())?;

    let mut found = Vec::new();
    collect_dictionary_references(&document.trailer, &mut found);
    Some(found)
}

fn delete_unreachable(document: &mut Document, roots: &[ObjectId]) -> usize {
    let mut reachable: HashSet<ObjectId> = HashSet::new();
    let mut pending: Vec<ObjectId> = roots.to_vec();

    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let Some(object) = document.objects.get(&id) else {
            continue;
        };

        let mut found = Vec::new();
        collect_references(object, &mut found);
        pending.extend(found);
    }

    let before = document.objects.len();
    document.objects.retain(|id, _| reachable.contains(id));
    before - document.objects.len()
}

/// Lowers `max_id` to the highest object that actually survived.
///
/// See this module's header for why this is not cosmetic. It is deliberately
/// *not* a renumbering: compacting holes in the middle of the id space means
/// rewriting every reference in the file, and it buys one cross-reference
/// entry per pruned object — a different feature, with a much larger blast
/// radius, not smuggled in here.
fn reclaim_id_space(document: &mut Document) {
    document.max_id = document
        .objects
        .keys()
        .map(|(id, _)| *id)
        .max()
        .unwrap_or(0);
}

/// Appends every reference reachable *within* `object` — without following
/// them — to `found`.
fn collect_references(object: &Object, found: &mut Vec<ObjectId>) {
    match object {
        Object::Reference(id) => found.push(*id),
        Object::Array(items) => {
            for item in items {
                collect_references(item, found);
            }
        }
        Object::Dictionary(dict) => collect_dictionary_references(dict, found),
        Object::Stream(stream) => collect_dictionary_references(&stream.dict, found),
        _ => {}
    }
}

fn collect_dictionary_references(dict: &Dictionary, found: &mut Vec<ObjectId>) {
    for (_, value) in dict.iter() {
        collect_references(value, found);
    }
}

#[cfg(test)]
mod tests {
    use lopdf::dictionary;

    use super::*;
    use crate::test_fixtures::loaded_document as loaded;

    fn sweep(document: &mut Document) -> usize {
        let roots = trailer_references(document).expect("the fixture names a catalog");
        pass(document, &roots)
    }

    #[test]
    fn an_object_nothing_points_at_is_deleted() {
        let mut document = loaded(3);
        let orphan = document.add_object(dictionary! { "Orphan" => true });

        assert_eq!(sweep(&mut document), 1);
        assert!(document.get_object(orphan).is_err());
    }

    /// Reachability is transitive, and the thing most easily got wrong: an
    /// object referenced only from a *stream's* dictionary is still in use.
    #[test]
    fn an_object_referenced_only_from_a_stream_dictionary_survives() {
        let mut document = loaded(2);
        let deep = document.add_object(dictionary! { "Deep" => true });
        let mut carrier = lopdf::Stream::new(dictionary! { "Deep" => deep }, b"x".to_vec());
        carrier.allows_compression = false;
        let carrier = document.add_object(carrier);
        crate::test_fixtures::attach_to_catalog(&mut document, carrier);

        assert_eq!(sweep(&mut document), 0);
        assert!(document.get_object(deep).is_ok());
    }

    /// Whatever the trailer names stays, even when it looks redundant —
    /// `/Info` is the case that arrives in real files.
    #[test]
    fn what_the_trailer_points_at_is_never_deleted() {
        let mut document = loaded(2);
        let info =
            document.add_object(dictionary! { "Producer" => Object::string_literal("vitela") });
        document.trailer.set("Info", info);

        sweep(&mut document);

        assert!(document.get_object(info).is_ok());
    }

    /// The other half of the leak: the sweep gives the top of the id space
    /// back, so the next write does not have to describe ids that no longer
    /// exist.
    #[test]
    fn sweeping_the_top_of_the_id_space_gives_it_back() {
        let mut document = loaded(3);
        let highest = document
            .objects
            .keys()
            .map(|(id, _)| *id)
            .max()
            .expect("the fixture has objects");
        document.add_object(dictionary! { "Orphan" => true });
        assert!(document.max_id > highest, "the orphan raised the ceiling");

        sweep(&mut document);

        assert_eq!(document.max_id, highest);
    }

    /// And it does not give back space that is still in use.
    #[test]
    fn a_document_with_nothing_to_sweep_keeps_its_ceiling() {
        let mut document = loaded(3);
        let ceiling = document.max_id;

        sweep(&mut document);

        assert_eq!(document.max_id, ceiling);
    }

    #[test]
    fn a_document_with_no_catalog_has_no_roots_to_sweep_from() {
        let mut document = loaded(3);
        document.trailer.remove(b"Root");

        assert!(trailer_references(&document).is_none());
    }
}
