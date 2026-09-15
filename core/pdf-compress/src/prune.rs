//! The prune: what nothing points at, and what is stored twice.
//!
//! ## The rule this module exists to keep
//!
//! *Only bytes no reader can reach are removed, and only copies of bytes that
//! stay.*
//!
//! Two sweeps over the object graph, in this order, because the first feeds
//! the second:
//!
//! 1. **Merge duplicates.** Objects with identical content collapse onto one
//!    survivor, and every reference to the copies is pointed at it instead.
//!    This is where an assembled document pays back: `pdf_manip::graft_pages`
//!    brings a font program, an `/ExtGState` or an image along with every page
//!    it imports, so a document built from five copies of the same source
//!    carries five copies of each.
//! 2. **Sweep what is unreachable.** Every object the trailer cannot reach,
//!    directly or transitively, is deleted. The copies that step 1 just
//!    orphaned go out here too, which is why there is one counter rather than
//!    two.
//!
//! ## The dead cross-reference object, which is why this runs at all
//!
//! T-191 shipped a repack that was not a fixed point: a document that arrived
//! packed came back **bigger**, and grew again on every round trip. The cause
//! is one object, and it is `lopdf`'s asymmetry rather than ours.
//!
//! `Document::save_internal` — the classic writer — skips
//! `[ObjStm, XRef, Linearized]` when it serialises `objects`
//! (`lopdf-0.45/src/writer.rs:80`). `save_with_object_streams`, the writer
//! [`crate::session`] asks for, skips only `ObjStm`
//! (`lopdf-0.45/src/writer.rs:149`). A cross-reference stream is an
//! `Object::Stream`, so `ObjectStream::can_be_compressed` rejects it at its
//! first rule and it lands in `objects_to_write_directly` — written out whole,
//! beside the fresh cross-reference stream the writer appends at the end.
//! Nothing references it. Nothing ever will. One corpse per repack, for ever.
//!
//! Measured on `assets/sample/vitela-sample.pdf`: 2 060 bytes in, 1 774 after
//! one repack, **2 023 after two**. The never-grow guarantee caught the file
//! and handed the original back, so no user ever saw a bigger file — which is
//! exactly the problem, because it also meant nobody saw the bug. There is no
//! special case for `/Type /XRef` in this module: it is unreachable from the
//! trailer, so the sweep takes it for the same reason it takes anything else.
//!
//! ## What is never merged, and why
//!
//! Identical content is not the same thing as interchangeable identity. Four
//! kinds of object are excluded from step 1 even when two of them are byte
//! for byte the same:
//!
//! - **`/Type /Page` and `/Type /Pages`** — two identical pages merged into
//!   one leave `/Kids [5 0 R, 5 0 R]`. The page count survives, so neither
//!   the guarantee nor [`crate::session`]'s re-read would notice, and then an
//!   annotation the user drops on page two appears on page one as well.
//! - **`/Type /Annot`** — same hazard one level down, and the shells edit
//!   annotations by object identity.
//! - **`/Type /Catalog`** — there is one, and the trailer names it.
//! - **anything the trailer references directly** — `/Root` and `/Info` are
//!   the document's own handles.
//!
//! The rule behind the list: *an object the rest of the application edits by
//! identity is never merged.* Sharing a font program between two pages is
//! invisible; sharing a page is a bug that takes a year to find.
//!
//! ## Why the merge runs to a fixed point
//!
//! Collapsing duplicates changes the objects that referenced them, which can
//! make *those* identical in turn. The real shape is a chain: two identical
//! `FontFile2` streams merge, which makes two `/FontDescriptor` dictionaries
//! identical, which makes two `/Font` dictionaries identical. One round would
//! collect the font programs — the big bytes — and leave the two dictionary
//! layers above them. So the merge repeats until a round finds nothing,
//! capped at [`MAX_MERGE_ROUNDS`] so a pathological graph cannot spin.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};

use crate::report::Work;

/// How many times the merge may repeat before giving up on finding more.
///
/// A chain of duplicates needs one round per layer (see this module's
/// header); three is the deepest real case this repository produces
/// (`FontFile2` → `/FontDescriptor` → `/Font`). The cap is not a correctness
/// bound — stopping early only leaves bytes behind — it is a guard against a
/// graph that keeps finding one more merge for ever.
const MAX_MERGE_ROUNDS: usize = 8;

/// Merges duplicate objects and deletes everything unreachable.
///
/// Reports how many objects the document lost, counting a merged copy and an
/// orphan the same way: both are objects that were in the file and are not
/// any more.
pub(crate) fn pass(document: &mut Document) -> Work {
    // Without a `/Root` there is no graph to be reachable from, and sweeping
    // would delete the entire document. A file in that state is not this
    // module's to repair.
    let Some(roots) = trailer_references(document) else {
        return Work::default();
    };

    for _ in 0..MAX_MERGE_ROUNDS {
        if merge_duplicates(document, &roots) == 0 {
            break;
        }
    }

    let dropped = sweep_unreachable(document, &roots);
    reclaim_id_space(document);

    Work {
        objects_dropped: dropped,
        ..Work::default()
    }
}

/// Lowers `max_id` to the highest object that actually survived.
///
/// The second half of the same leak the sweep fixes, and it has to be here
/// because only the sweep knows what is left. `lopdf`'s writer mints its new
/// object stream and cross-reference stream at `max_id + 1`
/// (`lopdf-0.45/src/writer.rs`), and `max_id` only ever goes up: load a
/// packed document, delete those two objects as unreachable, and `max_id`
/// still claims they are the top of the file. The next write then mints two
/// ids *above* them and the cross-reference table has to carry two free
/// entries for ids nothing has ever used — two more on every round trip.
///
/// Measured: with the sweep alone, a repacked document still grew five bytes
/// each time it was repacked. That is the whole of what is left of the
/// non-idempotence, and this is the line that removes it.
///
/// What this deliberately does *not* do is renumber. Compacting holes in the
/// middle of the id space means rewriting every reference in the file, and it
/// buys one cross-reference entry per pruned object — a different feature,
/// with a much larger blast radius, not smuggled in here.
fn reclaim_id_space(document: &mut Document) {
    document.max_id = document
        .objects
        .keys()
        .map(|(id, _)| *id)
        .max()
        .unwrap_or(0);
}

/// Every object the trailer points at, or `None` when it does not name a
/// catalog.
fn trailer_references(document: &Document) -> Option<Vec<ObjectId>> {
    let trailer = Object::Dictionary(document.trailer.clone());
    let mut found = Vec::new();
    collect_references(&trailer, &mut found);

    document
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|root| root.as_reference().ok())?;

    Some(found)
}

/// Points every reference to a duplicate at the object that survives it, and
/// says how many objects were made redundant this round.
///
/// Nothing is deleted here. The copies simply stop being referenced, and
/// [`sweep_unreachable`] collects them — one deletion path, one count.
fn merge_duplicates(document: &mut Document, roots: &[ObjectId]) -> usize {
    let mergeable: HashSet<ObjectId> = document
        .objects
        .iter()
        .filter(|(id, object)| may_be_merged(**id, object, roots))
        .map(|(id, _)| *id)
        .collect();

    // Sorted so that the survivor of a group is always its lowest id: the
    // same document must prune to the same bytes on every run.
    let mut candidates: Vec<ObjectId> = mergeable.into_iter().collect();
    candidates.sort_unstable();

    let mut survivors: HashMap<u64, Vec<ObjectId>> = HashMap::new();
    let mut replacements: HashMap<ObjectId, ObjectId> = HashMap::new();

    for id in candidates {
        let Some(object) = document.objects.get(&id) else {
            continue;
        };
        let bucket = survivors.entry(fingerprint(object)).or_default();

        // A fingerprint collision is not a merge. The bucket is a shortlist;
        // `same_content` is the decision.
        let twin = bucket
            .iter()
            .find(|survivor| {
                document
                    .objects
                    .get(survivor)
                    .is_some_and(|kept| same_content(kept, object))
            })
            .copied();

        match twin {
            Some(survivor) => {
                replacements.insert(id, survivor);
            }
            None => bucket.push(id),
        }
    }

    if replacements.is_empty() {
        return 0;
    }

    for object in document.objects.values_mut() {
        redirect_references(object, &replacements);
    }
    let mut trailer = Object::Dictionary(std::mem::take(&mut document.trailer));
    redirect_references(&mut trailer, &replacements);
    if let Object::Dictionary(dict) = trailer {
        document.trailer = dict;
    }

    replacements.len()
}

/// Whether two objects with the same content may become one.
///
/// See this module's header for the list and the rule behind it.
fn may_be_merged(id: ObjectId, object: &Object, roots: &[ObjectId]) -> bool {
    if id.1 != 0 || roots.contains(&id) {
        return false;
    }

    !matches!(
        type_name(object),
        Some(b"Page") | Some(b"Pages") | Some(b"Catalog") | Some(b"Annot")
    )
}

/// The `/Type` of an object's dictionary, stream or not.
fn type_name(object: &Object) -> Option<&[u8]> {
    let dict = match object {
        Object::Dictionary(dict) => dict,
        Object::Stream(stream) => &stream.dict,
        _ => return None,
    };

    dict.get(b"Type").ok()?.as_name().ok()
}

/// Deletes every object the trailer cannot reach, and says how many.
fn sweep_unreachable(document: &mut Document, roots: &[ObjectId]) -> usize {
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

/// Rewrites every reference in `object` that `replacements` has an entry for.
fn redirect_references(object: &mut Object, replacements: &HashMap<ObjectId, ObjectId>) {
    match object {
        Object::Reference(id) => {
            if let Some(survivor) = replacements.get(id) {
                *id = *survivor;
            }
        }
        Object::Array(items) => {
            for item in items {
                redirect_references(item, replacements);
            }
        }
        Object::Dictionary(dict) => redirect_dictionary_references(dict, replacements),
        Object::Stream(stream) => redirect_dictionary_references(&mut stream.dict, replacements),
        _ => {}
    }
}

fn redirect_dictionary_references(
    dict: &mut Dictionary,
    replacements: &HashMap<ObjectId, ObjectId>,
) {
    for (_, value) in dict.iter_mut() {
        redirect_references(value, replacements);
    }
}

/// A cheap shortlist key. Equality is [`same_content`]'s job, never this
/// one's — a hash decides which objects are worth comparing, not which are
/// the same.
fn fingerprint(object: &Object) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_object(object, &mut hasher);
    hasher.finish()
}

fn hash_object(object: &Object, hasher: &mut DefaultHasher) {
    // The leading discriminant byte is what stops `Name(b"7")` and
    // `String(b"7", Literal)` from sharing a bucket for free.
    match object {
        Object::Null => 0u8.hash(hasher),
        Object::Boolean(value) => {
            1u8.hash(hasher);
            value.hash(hasher);
        }
        Object::Integer(value) => {
            2u8.hash(hasher);
            value.hash(hasher);
        }
        Object::Real(value) => {
            3u8.hash(hasher);
            // `f32` is not `Hash`; its bit pattern is, and two reals that are
            // byte-identical are what this is looking for anyway.
            value.to_bits().hash(hasher);
        }
        Object::Name(name) => {
            4u8.hash(hasher);
            name.hash(hasher);
        }
        Object::String(bytes, format) => {
            5u8.hash(hasher);
            bytes.hash(hasher);
            matches!(format, StringFormat::Hexadecimal).hash(hasher);
        }
        Object::Array(items) => {
            6u8.hash(hasher);
            items.len().hash(hasher);
            for item in items {
                hash_object(item, hasher);
            }
        }
        Object::Dictionary(dict) => {
            7u8.hash(hasher);
            hash_dictionary(dict, hasher);
        }
        Object::Stream(stream) => {
            8u8.hash(hasher);
            hash_dictionary(&stream.dict, hasher);
            stream.content.hash(hasher);
            stream.allows_compression.hash(hasher);
        }
        Object::Reference(id) => {
            9u8.hash(hasher);
            id.hash(hasher);
        }
    }
}

fn hash_dictionary(dict: &Dictionary, hasher: &mut DefaultHasher) {
    dict.len().hash(hasher);
    for (key, value) in dict.iter() {
        key.hash(hasher);
        hash_object(value, hasher);
    }
}

/// Whether two objects hold the same thing.
///
/// Not `a == b`, and the difference is the whole reason this function exists:
/// `lopdf::Stream` derives `PartialEq` over **four** fields, and one of them
/// is `start_position` — the byte offset the stream was read from in the
/// original file. Two byte-identical streams parsed out of one document are
/// never at the same offset, so `==` reports every one of them as different
/// and the merge would find nothing at all. What is compared here is what
/// gets written out: the dictionary, the content, and whether the stream may
/// be compressed (a font program that says it may not is not the same object
/// as one that says it may, however identical their bytes).
fn same_content(a: &Object, b: &Object) -> bool {
    match (a, b) {
        (Object::Stream(left), Object::Stream(right)) => {
            left.dict == right.dict
                && left.content == right.content
                && left.allows_compression == right.allows_compression
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use lopdf::{dictionary, Stream};

    use super::*;
    use crate::test_fixtures::loaded_document as loaded;

    fn object_count(document: &Document) -> usize {
        document.objects.len()
    }

    /// A stream carrying `content`, with a `start_position` as if it had been
    /// read from `offset` — the field that makes `Stream`'s derived
    /// `PartialEq` useless here.
    fn stream_read_from(content: &[u8], offset: usize) -> Object {
        let mut stream = Stream::new(
            dictionary! { "Length" => content.len() as i64 },
            content.to_vec(),
        );
        stream.start_position = Some(offset);
        Object::Stream(stream)
    }

    #[test]
    fn an_object_nothing_points_at_is_dropped() {
        let mut document = loaded(3);
        let before = object_count(&document);
        document.add_object(dictionary! { "Orphan" => true });

        let work = pass(&mut document);

        assert_eq!(work.objects_dropped, 1);
        assert_eq!(object_count(&document), before);
    }

    #[test]
    fn a_document_with_nothing_to_prune_loses_nothing() {
        let mut document = loaded(3);
        let before = object_count(&document);

        let work = pass(&mut document);

        assert_eq!(work.objects_dropped, 0);
        assert_eq!(object_count(&document), before);
    }

    #[test]
    fn every_page_survives_the_prune() {
        let mut document = loaded(6);
        document.add_object(dictionary! { "Orphan" => true });

        pass(&mut document);

        assert_eq!(document.get_pages().len(), 6);
    }

    /// The object this module was written for. After one repack the reloaded
    /// document holds the previous generation's cross-reference stream as a
    /// live entry that nothing references — see this module's header.
    #[test]
    fn the_previous_generations_cross_reference_stream_is_swept() {
        let mut document = loaded(3);
        document.add_object(Stream::new(
            dictionary! { "Type" => "XRef", "Size" => 12i64 },
            vec![0; 64],
        ));

        let work = pass(&mut document);

        assert_eq!(work.objects_dropped, 1);
        assert!(
            !document
                .objects
                .values()
                .any(|object| type_name(object) == Some(b"XRef")),
            "a cross-reference object nothing points at must not survive"
        );
    }

    /// The other half of the leak: the sweep gives the top of the id space
    /// back, so the next write does not have to describe two ids that no
    /// longer exist.
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

        pass(&mut document);

        assert_eq!(document.max_id, highest);
    }

    /// And it does not give back space that is still in use.
    #[test]
    fn a_document_with_nothing_to_sweep_keeps_its_ceiling() {
        let mut document = loaded(3);
        let ceiling = document.max_id;

        pass(&mut document);

        assert_eq!(document.max_id, ceiling);
    }

    #[test]
    fn two_byte_identical_streams_become_one() {
        let mut document = loaded(2);
        let first = document.add_object(stream_read_from(b"shared font program bytes", 100));
        let second = document.add_object(stream_read_from(b"shared font program bytes", 900));
        let holder = document.add_object(dictionary! { "A" => first, "B" => second });
        attach_to_catalog(&mut document, holder);

        let work = pass(&mut document);

        assert_eq!(work.objects_dropped, 1);
        let holder = document
            .get_object(holder)
            .and_then(|object| object.as_dict())
            .expect("the holder survives");
        assert_eq!(
            holder.get(b"A").ok().and_then(|a| a.as_reference().ok()),
            holder.get(b"B").ok().and_then(|b| b.as_reference().ok()),
            "both references must point at the one survivor"
        );
    }

    /// The case that actually pays, taken from
    /// `tests/fixtures/large/edit_reopen_10pg.pdf`: ten pages, each drawing
    /// one image, each with its own thirty-byte content stream reading
    /// `q 612 0 0 792 0 0 cm /Im0 Do Q`. All ten streams are byte-identical —
    /// nine of them collapse — and the pages still draw different images,
    /// because `/Im0` is resolved through each page's own `/Resources` and
    /// those are not identical. Name indirection is what makes sharing a
    /// content stream safe, and it is why the resource dictionaries must
    /// *not* merge along with it.
    #[test]
    fn pages_that_draw_through_the_same_operators_share_one_content_stream() {
        // Two real pages already in the tree, so the only thing this test
        // adds is the pair below — a zero-page fixture would leave its own
        // font and resources orphaned and the count would be about those.
        let mut document = loaded(2);
        let draw = b"q 612 0 0 792 0 0 cm /Im0 Do Q".to_vec();

        let mut page_with = |image: &[u8]| {
            let xobject = document.add_object(stream_read_from(image, 0));
            let resources =
                document.add_object(dictionary! { "XObject" => dictionary! { "Im0" => xobject } });
            let content = document.add_object(stream_read_from(&draw, image.len()));
            document.add_object(dictionary! {
                "Type" => "Page",
                "Contents" => content,
                "Resources" => resources,
            })
        };
        let left = page_with(b"the first image");
        let right = page_with(b"a different image");
        let holder = document.add_object(dictionary! { "L" => left, "R" => right });
        attach_to_catalog(&mut document, holder);

        let work = pass(&mut document);

        assert_eq!(
            work.objects_dropped, 1,
            "the shared content stream should collapse, and nothing else"
        );
        let resources_of = |page| {
            document
                .get_object(page)
                .and_then(|object| object.as_dict())
                .and_then(|dict| dict.get(b"Resources"))
                .and_then(|value| value.as_reference())
                .expect("each page keeps a resources reference")
        };
        assert_ne!(
            resources_of(left),
            resources_of(right),
            "the pages draw different images; their resources must stay apart"
        );
    }

    /// The gotcha named in [`same_content`]'s header, pinned on its own: the
    /// two streams above differ in `start_position` and nothing else, so
    /// `lopdf`'s derived equality calls them different objects.
    #[test]
    fn lopdfs_own_equality_would_have_found_no_duplicates() {
        let left = stream_read_from(b"identical", 100);
        let right = stream_read_from(b"identical", 900);

        assert_ne!(
            left, right,
            "if this passes, start_position stopped mattering"
        );
        assert!(same_content(&left, &right));
    }

    /// Streams that really are different stay different.
    #[test]
    fn streams_with_different_content_are_not_merged() {
        let mut document = loaded(2);
        let first = document.add_object(stream_read_from(b"one thing", 100));
        let second = document.add_object(stream_read_from(b"another thing", 100));
        let holder = document.add_object(dictionary! { "A" => first, "B" => second });
        attach_to_catalog(&mut document, holder);

        let work = pass(&mut document);

        assert_eq!(work.objects_dropped, 0);
    }

    /// The chain from this module's header: a duplicate two layers down only
    /// becomes visible once the layer below it has collapsed.
    #[test]
    fn a_chain_of_duplicates_collapses_all_the_way_up() {
        let mut document = loaded(2);

        let mut layered = |offset: usize| {
            let program = document.add_object(stream_read_from(b"a font program", offset));
            let descriptor = document
                .add_object(dictionary! { "Type" => "FontDescriptor", "FontFile2" => program });
            document.add_object(dictionary! { "Type" => "Font", "FontDescriptor" => descriptor })
        };
        let left = layered(100);
        let right = layered(900);
        let holder = document.add_object(dictionary! { "L" => left, "R" => right });
        attach_to_catalog(&mut document, holder);

        let work = pass(&mut document);

        assert_eq!(
            work.objects_dropped, 3,
            "the program, the descriptor and the font should all collapse"
        );
    }

    /// The exclusion that matters most. Two identical pages sharing one
    /// object would keep the page count — so neither the guarantee nor the
    /// session's re-read would catch it — and then an annotation on one would
    /// appear on both.
    #[test]
    fn two_identical_pages_are_never_merged() {
        let mut document = loaded(1);
        let page_id = *document
            .get_pages()
            .values()
            .next()
            .expect("the fixture has a page");
        let page = document.get_object(page_id).expect("it is there").clone();
        let twin = document.add_object(page);
        add_kid(&mut document, twin);
        let before = object_count(&document);

        pass(&mut document);

        assert_eq!(object_count(&document), before);
        assert_eq!(document.get_pages().len(), 2);
    }

    /// The catalog is named by the trailer, so the roots exclusion already
    /// covers it; this pins that a second catalog is not merged into it
    /// either.
    #[test]
    fn a_second_catalog_is_not_merged_into_the_first() {
        let mut document = loaded(2);
        let catalog_id = document
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .expect("the fixture names a catalog");
        let catalog = document
            .get_object(catalog_id)
            .expect("it is there")
            .clone();
        let twin = document.add_object(catalog);
        let holder = document.add_object(dictionary! { "Extra" => twin });
        attach_to_catalog(&mut document, holder);

        pass(&mut document);

        assert!(
            document.get_object(twin).is_ok(),
            "a second catalog is reachable, kept, and not merged away"
        );
    }

    /// Without a `/Root` there is nothing to be reachable from, and a sweep
    /// would delete the document rather than prune it.
    #[test]
    fn a_document_with_no_catalog_is_left_alone() {
        let mut document = loaded(3);
        let before = object_count(&document);
        document.trailer.remove(b"Root");

        let work = pass(&mut document);

        assert_eq!(work.objects_dropped, 0);
        assert_eq!(object_count(&document), before);
    }

    /// Whatever the trailer names stays, even when it looks redundant —
    /// `/Info` is the case that arrives in real files.
    #[test]
    fn what_the_trailer_points_at_is_never_swept() {
        let mut document = loaded(2);
        let info =
            document.add_object(dictionary! { "Producer" => Object::string_literal("vitela") });
        document.trailer.set("Info", info);

        pass(&mut document);

        assert!(document.get_object(info).is_ok());
    }

    /// Hangs `id` off the catalog so the sweep can reach it.
    fn attach_to_catalog(document: &mut Document, id: ObjectId) {
        let catalog_id = document
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .expect("the fixture names a catalog");
        document
            .get_object_mut(catalog_id)
            .and_then(|object| object.as_dict_mut())
            .expect("the catalog is a dictionary")
            .set("VitelaTestHolder", id);
    }

    /// Adds `id` to the page tree's `/Kids`, so a cloned page is a real
    /// second page rather than an orphan.
    fn add_kid(document: &mut Document, id: ObjectId) {
        let pages_id = document
            .catalog()
            .ok()
            .and_then(|catalog| catalog.get(b"Pages").ok())
            .and_then(|pages| pages.as_reference().ok())
            .expect("the fixture has a page tree");
        let tree = document
            .get_object_mut(pages_id)
            .and_then(|object| object.as_dict_mut())
            .expect("the page tree is a dictionary");
        if let Ok(Object::Array(kids)) = tree.get_mut(b"Kids") {
            kids.push(Object::Reference(id));
        }
        tree.set("Count", 2i64);
    }
}
