//! When two objects are the same thing.
//!
//! ## The rule this module exists to keep
//!
//! *The hash decides what is worth comparing. Only the comparison decides
//! what is the same.*
//!
//! [`super::merge`] asks one question of every object pair it considers, and
//! a wrong answer either loses bytes (two identical objects called different)
//! or corrupts the document (two different objects called identical). So the
//! two halves are kept apart on purpose:
//!
//! - [`fingerprint`] is a cheap `u64` that buckets objects into shortlists.
//!   A collision costs one wasted comparison and nothing else, because it is
//!   never the decision.
//! - [`same_content`] is the decision, and it is exact.
//!
//! ## Why `same_content` is not `a == b`
//!
//! This is the trap, and it is not visible from the type. `lopdf::Stream`
//! derives `PartialEq` over **four** fields, and one of them is
//! `start_position` — the byte offset the stream was read from in the
//! original file. Two byte-identical streams parsed out of one document are
//! never at the same offset, so `==` reports every one of them as different
//! and a deduplication written with `==` finds **nothing at all**.
//!
//! What is compared here is what gets written out: the dictionary, the
//! content, and `allows_compression` — a font program that says it may not be
//! compressed is not the same object as one that says it may, however
//! identical their bytes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use lopdf::{Dictionary, Object, StringFormat};

/// A cheap shortlist key. See this module's header: never the decision.
pub(super) fn fingerprint(object: &Object) -> u64 {
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

/// Whether two objects hold the same thing. Not `a == b` — see this module's
/// header for why that would find nothing.
pub(super) fn same_content(a: &Object, b: &Object) -> bool {
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
    use lopdf::dictionary;

    use super::*;
    use crate::test_fixtures::stream_read_from;

    /// The trap, pinned. If this ever stops failing, `lopdf::Stream` stopped
    /// comparing `start_position` and [`same_content`] is worth revisiting.
    #[test]
    fn lopdfs_own_equality_would_have_found_no_duplicates() {
        let left = stream_read_from(b"identical", 100);
        let right = stream_read_from(b"identical", 900);

        assert_ne!(
            left, right,
            "if this passes, start_position stopped mattering"
        );
        assert!(same_content(&left, &right));
        assert_eq!(fingerprint(&left), fingerprint(&right));
    }

    #[test]
    fn streams_with_different_content_are_not_the_same() {
        let left = stream_read_from(b"one thing", 0);
        let right = stream_read_from(b"another thing", 0);

        assert!(!same_content(&left, &right));
    }

    /// `allows_compression` is part of what gets written, so it is part of
    /// the comparison: a font program that must stay raw is not the same
    /// object as one that may be flated.
    #[test]
    fn a_stream_that_refuses_compression_is_not_the_same_as_one_that_allows_it() {
        let left = stream_read_from(b"a font program", 0);
        let mut right = stream_read_from(b"a font program", 0);
        if let Object::Stream(stream) = &mut right {
            stream.allows_compression = false;
        }

        assert!(!same_content(&left, &right));
        assert_ne!(fingerprint(&left), fingerprint(&right));
    }

    /// The discriminant byte earning its keep: two objects that would hash
    /// the same from their payload alone must not share a bucket.
    #[test]
    fn a_name_and_a_string_of_the_same_bytes_do_not_share_a_bucket() {
        let name = Object::Name(b"7".to_vec());
        let string = Object::string_literal("7");

        assert!(!same_content(&name, &string));
        assert_ne!(fingerprint(&name), fingerprint(&string));
    }

    /// Dictionaries are compared by their entries, and a value one level down
    /// is enough to tell them apart.
    #[test]
    fn dictionaries_differing_one_level_down_are_not_the_same() {
        let left = Object::Dictionary(dictionary! { "Font" => dictionary! { "Size" => 12i64 } });
        let right = Object::Dictionary(dictionary! { "Font" => dictionary! { "Size" => 14i64 } });

        assert!(!same_content(&left, &right));
        assert_ne!(fingerprint(&left), fingerprint(&right));
    }

    /// References are compared by what they point at, which is what makes the
    /// chain in [`super::super::merge`] work: two dictionaries become
    /// identical only once the objects under them have.
    #[test]
    fn references_to_different_objects_are_not_the_same() {
        let left = Object::Reference((4, 0));
        let right = Object::Reference((9, 0));

        assert!(!same_content(&left, &right));
        assert_ne!(fingerprint(&left), fingerprint(&right));
        assert!(same_content(&left, &Object::Reference((4, 0))));
    }
}
