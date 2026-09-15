//! Objects stored twice, made one.
//!
//! ## The rule this module exists to keep
//!
//! *Two objects become one only when nothing can tell them apart, and only
//! when nothing else in the application needs to tell them apart.*
//!
//! *Identical* is [`super::identity`]'s word, not this module's — and it is
//! not `==`, for a reason worth reading before touching either.
//!
//! Nothing is deleted here. References to a duplicate are pointed at the
//! object that survives it, and the copies are left orphaned for
//! [`super::sweep`] to collect — one deletion path, one count.
//!
//! This is where an assembled document pays back: `pdf_manip::graft_pages`
//! brings a font program, an `/ExtGState` or an image along with every page it
//! imports, so a document built from five copies of one source carries five
//! copies of each.
//!
//! ## What is never merged, and why
//!
//! Identical content is not the same thing as interchangeable identity. Four
//! kinds of object are excluded even when two of them are byte for byte the
//! same:
//!
//! - **`/Type /Page` and `/Type /Pages`** — two identical pages merged into
//!   one leave `/Kids [5 0 R, 5 0 R]`. The page count survives, so neither
//!   the never-grow guarantee nor [`crate::session`]'s re-read would notice,
//!   and then an annotation the user drops on page two appears on page one as
//!   well.
//! - **`/Type /Annot`** — the same hazard one level down, and the shells edit
//!   annotations by object identity.
//! - **`/Type /Catalog`** — there is one, and the trailer names it.
//! - **anything the trailer references directly** — `/Root` and `/Info` are
//!   the document's own handles.
//!
//! The rule behind the list: *an object the rest of the application edits by
//! identity is never merged.* Sharing a font program between two pages is
//! invisible; sharing a page is a bug that takes a year to find.
//!
//! ## Why it runs to a fixed point
//!
//! Collapsing duplicates changes the objects that referenced them, which can
//! make *those* identical in turn. The real shape is a chain: two identical
//! `FontFile2` streams merge, which makes two `/FontDescriptor` dictionaries
//! identical, which makes two `/Font` dictionaries identical. One round would
//! collect the font programs — the big bytes — and leave the two dictionary
//! layers above them. So it repeats until a round finds nothing, capped at
//! [`MAX_ROUNDS`] so a pathological graph cannot spin.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::identity::{fingerprint, same_content};

/// How many times the merge may repeat before giving up on finding more.
///
/// A chain of duplicates needs one round per layer (see this module's
/// header); three is the deepest real case this repository produces
/// (`FontFile2` → `/FontDescriptor` → `/Font`). The cap is not a correctness
/// bound — stopping early only leaves bytes behind — it is a guard against a
/// graph that keeps finding one more merge for ever.
const MAX_ROUNDS: usize = 8;

/// Collapses duplicates until there are none left to find, and says how many
/// objects were made redundant.
///
/// Those objects are still in the document when this returns; they are simply
/// no longer referenced. Which is exactly why the rounds need `retired`: a
/// copy that has been collapsed is still sitting in `objects`, still
/// byte-identical to its survivor, and a round that did not know better would
/// find the same pair again every time — never converging, and running all
/// [`MAX_ROUNDS`] redirections over the whole graph on every document.
pub(super) fn pass(document: &mut Document, roots: &[ObjectId]) -> usize {
    let mut retired: HashSet<ObjectId> = HashSet::new();

    for _ in 0..MAX_ROUNDS {
        if one_round(document, roots, &mut retired) == 0 {
            break;
        }
    }

    retired.len()
}

/// Points every reference to a duplicate at the object that survives it, and
/// says how many objects this round made redundant.
///
/// `retired` carries what earlier rounds already collapsed, both as the
/// exclusion that lets the loop terminate and as the running total.
fn one_round(
    document: &mut Document,
    roots: &[ObjectId],
    retired: &mut HashSet<ObjectId>,
) -> usize {
    let mergeable: HashSet<ObjectId> = document
        .objects
        .iter()
        .filter(|(id, object)| !retired.contains(id) && may_be_merged(**id, object, roots))
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
    retired.extend(replacements.keys().copied());

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
        super::type_name(object),
        Some(b"Page") | Some(b"Pages") | Some(b"Catalog") | Some(b"Annot")
    )
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

#[cfg(test)]
mod tests {
    use lopdf::dictionary;

    use super::super::sweep::trailer_references;
    use super::*;
    use crate::test_fixtures::{
        add_kid, attach_to_catalog, loaded_document as loaded, stream_read_from,
    };

    /// Runs the merge the way [`super::super::pass`] does, against the roots
    /// the document actually names.
    fn merge(document: &mut Document) -> usize {
        let roots = trailer_references(document).expect("the fixture names a catalog");
        pass(document, &roots)
    }

    #[test]
    fn two_byte_identical_streams_become_one() {
        let mut document = loaded(2);
        let first = document.add_object(stream_read_from(b"shared font program bytes", 100));
        let second = document.add_object(stream_read_from(b"shared font program bytes", 900));
        let holder = document.add_object(dictionary! { "A" => first, "B" => second });
        attach_to_catalog(&mut document, holder);

        assert_eq!(merge(&mut document), 1);

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

    /// Streams that really are different stay different.
    #[test]
    fn streams_with_different_content_are_not_merged() {
        let mut document = loaded(2);
        let first = document.add_object(stream_read_from(b"one thing", 100));
        let second = document.add_object(stream_read_from(b"another thing", 100));
        let holder = document.add_object(dictionary! { "A" => first, "B" => second });
        attach_to_catalog(&mut document, holder);

        assert_eq!(merge(&mut document), 0);
    }

    /// The chain from this module's header: a duplicate two layers up only
    /// becomes visible once the layer below it has collapsed, which is what
    /// the rounds are for.
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

        assert_eq!(
            merge(&mut document),
            3,
            "the program, the descriptor and the font should all collapse"
        );
    }

    /// One round is not enough, which is the whole reason [`MAX_ROUNDS`]
    /// exists. Without this the test above could pass against a single pass
    /// that happened to visit the layers in a lucky order.
    #[test]
    fn one_round_only_reaches_the_bottom_layer() {
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
        let roots = trailer_references(&document).expect("the fixture names a catalog");
        let mut retired = HashSet::new();

        assert_eq!(
            one_round(&mut document, &roots, &mut retired),
            1,
            "only the font programs are identical before anything has collapsed"
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

        assert_eq!(merge(&mut document), 0);
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

        assert_eq!(merge(&mut document), 0);
        assert!(
            document.get_object(twin).is_ok(),
            "a second catalog is reachable, kept, and not merged away"
        );
    }
}
