//! The structural pass: the same document, packed the way PDF 1.5 allows.
//!
//! ## The rule this module exists to keep
//!
//! *Not one byte a reader can see changes. Only how those bytes are stored.*
//!
//! Three levers, all of them already in `lopdf` (`docs/batch-compress.md`
//! fact 3), none of them touching a pixel or a glyph:
//!
//! - **object streams** — the dictionaries that describe pages, fonts and
//!   resources stop being individually serialised ASCII and get packed into a
//!   flate-compressed container.
//! - **a cross-reference stream** — the offset table stops being a
//!   twenty-bytes-per-object ASCII listing and becomes a compressed binary
//!   one.
//! - **flate over streams that arrived unfiltered** — a content stream
//!   written in the clear is re-encoded. A stream that already carries a
//!   `/Filter` is never touched, because re-filtering compressed bytes grows
//!   them.
//!
//! The first two are the *write format*, not a transformation: they are
//! [`crate::session`]'s `repacked()`, asked for once when the document is
//! serialised. What is left here is the third — the only one of the three
//! that changes the object graph, and therefore the only one that is a stage.
//!
//! This is what [`CompressPreset::Lossless`](crate::CompressPreset::Lossless)
//! is, together with [`crate::prune`], and what the other two presets do
//! before they touch an image.

use lopdf::{Document, Object};

use crate::report::Work;

/// Flate-encodes every stream that arrived without a `/Filter`.
///
/// `lopdf::Document::compress` does this walk already, but it returns `()`,
/// so a caller cannot tell one stream from four hundred and the report would
/// have to guess. The walk is three lines; the count is the reason it is
/// here.
///
/// Two guards, both `lopdf`'s and both kept:
///
/// - `allows_compression` is how a stream that must stay raw (a font
///   program's, classically) says so.
/// - `Stream::compress` only swaps its content in if the flated bytes are
///   actually shorter, so an incompressible stream is left alone rather than
///   re-encoded into something larger. That is the never-grow rule at a third
///   scale, and it is already written down inside `lopdf`.
pub(crate) fn pass(document: &mut Document) -> Work {
    let mut recompressed = 0;

    for object in document.objects.values_mut() {
        let Object::Stream(stream) = object else {
            continue;
        };
        if !stream.allows_compression || stream.dict.get(b"Filter").is_ok() {
            continue;
        }

        // Errors are per-stream and not fatal: a stream that will not encode
        // is a stream that stays as it was, exactly as `Document::compress`
        // treats it.
        let _ = stream.compress();

        if stream.dict.get(b"Filter").is_ok() {
            recompressed += 1;
        }
    }

    Work {
        streams_recompressed: recompressed,
        ..Work::default()
    }
}

#[cfg(test)]
mod tests {
    use lopdf::{dictionary, Document, Stream};

    use super::*;
    use crate::test_fixtures::loaded_document;

    #[test]
    fn a_stream_that_arrived_unfiltered_leaves_flated_and_counted() {
        let mut document = loaded_document(3);

        assert_eq!(
            pass(&mut document).streams_recompressed,
            3,
            "three unfiltered content streams arrived; three should be flated"
        );
    }

    #[test]
    fn a_stream_that_already_carried_a_filter_is_left_alone() {
        let mut document = loaded_document(3);
        assert_eq!(
            pass(&mut document).streams_recompressed,
            3,
            "the first pass flates what arrived unfiltered"
        );

        assert_eq!(
            pass(&mut document).streams_recompressed,
            0,
            "a second pass must not re-filter already-compressed bytes"
        );
    }

    /// `lopdf`'s own never-grow rule, borrowed rather than re-implemented: a
    /// stream too short for flate to win keeps its raw bytes and is not
    /// counted as work.
    #[test]
    fn an_incompressible_stream_is_not_counted_as_recompressed() {
        let mut document = Document::with_version("1.5");
        document.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));

        assert_eq!(pass(&mut document).streams_recompressed, 0);
    }

    /// A stream that says it may not be compressed is not compressed, however
    /// unfiltered it arrived.
    #[test]
    fn a_stream_that_refuses_compression_is_left_raw() {
        let mut document = Document::with_version("1.5");
        let mut stream = Stream::new(dictionary! {}, b"a font program, repeated. ".repeat(40));
        stream.allows_compression = false;
        let id = document.add_object(stream);

        assert_eq!(pass(&mut document).streams_recompressed, 0);
        assert!(document
            .get_object(id)
            .and_then(|object| object.as_stream())
            .expect("still a stream")
            .dict
            .get(b"Filter")
            .is_err());
    }
}
