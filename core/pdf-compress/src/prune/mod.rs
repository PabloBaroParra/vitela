//! The prune: what nothing points at, and what is stored twice.
//!
//! ## The rule this module exists to keep
//!
//! *Only bytes no reader can reach are removed, and only copies of bytes that
//! stay.*
//!
//! Two passes over the object graph, in this order, because the first feeds
//! the second:
//!
//! 1. [`merge`] — objects with identical content collapse onto one survivor,
//!    and every reference to the copies is pointed at it instead. Nothing is
//!    deleted there; the copies simply stop being referenced. What counts as
//!    identical is [`identity`]'s answer, and it is deliberately not `==`.
//! 2. [`sweep`] — every object the trailer cannot reach, directly or
//!    transitively, is deleted. The copies the merge just orphaned go out
//!    here too, which is why there is one counter rather than two: one
//!    deletion path, one number.
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
//! special case for `/Type /XRef` anywhere here: it is unreachable from the
//! trailer, so [`sweep`] takes it for the same reason it takes anything else.
//!
//! The other half of that leak is the id space, and it belongs to [`sweep`]
//! too — see `sweep::reclaim_id_space`.

use lopdf::{Document, Object};

use crate::report::Work;

mod identity;
mod merge;
mod sweep;

/// Merges duplicate objects and deletes everything unreachable.
///
/// Reports how many objects the document lost, counting a merged copy and an
/// orphan the same way: both are objects that were in the file and are not
/// any more.
pub(crate) fn pass(document: &mut Document) -> Work {
    // Without a `/Root` there is no graph to be reachable from, and sweeping
    // would delete the entire document rather than prune it. A file in that
    // state is not this module's to repair.
    let Some(roots) = sweep::trailer_references(document) else {
        return Work::default();
    };

    merge::pass(document, &roots);

    Work {
        objects_dropped: sweep::pass(document, &roots),
        ..Work::default()
    }
}

/// The `/Type` of an object's dictionary, stream or not.
///
/// Shared because both halves ask it: [`merge`] to decide what may never
/// become one object, and this module's tests to name what the sweep took.
fn type_name(object: &Object) -> Option<&[u8]> {
    let dict = match object {
        Object::Dictionary(dict) => dict,
        Object::Stream(stream) => &stream.dict,
        _ => return None,
    };

    dict.get(b"Type").ok()?.as_name().ok()
}

#[cfg(test)]
mod tests {
    use lopdf::{dictionary, Stream};

    use super::*;
    use crate::test_fixtures::{attach_to_catalog, loaded_document as loaded, stream_read_from};

    fn object_count(document: &Document) -> usize {
        document.objects.len()
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

    /// Both halves in one document, and the case that actually pays, taken
    /// from `tests/fixtures/large/edit_reopen_10pg.pdf`: ten pages, each
    /// drawing one image, each with its own thirty-byte content stream
    /// reading `q 612 0 0 792 0 0 cm /Im0 Do Q`. All ten streams are
    /// byte-identical — nine collapse — and the pages still draw different
    /// images, because `/Im0` is resolved through each page's own
    /// `/Resources` and those are not identical. Name indirection is what
    /// makes sharing a content stream safe, and it is why the resource
    /// dictionaries must *not* merge along with it.
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
}
