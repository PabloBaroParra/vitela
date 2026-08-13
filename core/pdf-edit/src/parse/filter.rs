//! Deciding whether a content stream can be decoded *provably*, and doing it.
//!
//! The rule this module exists to enforce: **if we cannot prove we understood
//! the whole stream, we do not touch it.** Everything downstream —
//! `crate::edit`'s splice, `crate::insert`'s placement — writes bytes back
//! into the file, and bytes we only half-understood are bytes we would
//! destroy.
//!
//! ## Why not `lopdf`
//!
//! `Stream::decompressed_content` cannot express the answer we need:
//!
//! - **A failed inflate is not reported.** `Stream::decompress_zlib` logs the
//!   error, retries raw deflate, and returns `Ok` with whatever came out —
//!   possibly a prefix, possibly nothing. A damaged stream therefore arrives
//!   looking like a page that legitimately paints very little, and appending
//!   to it or splicing it writes that truncation back as the page's content.
//! - **`/Filter` is not dereferenced.** It is perfectly legal for it to be an
//!   indirect reference, and a reader that only pattern-matches on the direct
//!   object concludes "no filter" and hands out compressed bytes as operators.
//!
//! So the decode happens here, against `flate2` directly, where the end of
//! the stream is something we can *require* rather than hope for.
//!
//! ## What is supported
//!
//! One `FlateDecode`, no `/DecodeParms`. That is what page content streams
//! use in practice. Everything else — LZW, ASCII85, a chain of two filters, a
//! predictor — is refused rather than guessed at, because supporting a format
//! means being able to prove a round trip through it, and an unproven round
//! trip is how a page gets silently rewritten into garbage.

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::error::EditError;

/// A page content stream we are allowed to read, and whether it arrived
/// compressed.
#[derive(Debug)]
pub(crate) struct DecodedStream {
    pub bytes: Vec<u8>,
    /// Whether the stream declared a filter — recorded at decode time rather
    /// than re-derived when writing, so the answer that governs
    /// re-compression is the same one that governed decoding. Re-reading the
    /// dictionary later would let the two disagree.
    pub filtered: bool,
}

/// Decodes a content stream, or refuses it.
///
/// # Errors
///
/// - [`EditError::UnsupportedContentStreamFilter`] when the stream is encoded
///   in a way this crate cannot prove it round-trips: a filter other than
///   `FlateDecode`, a chain of them, or any `/DecodeParms`.
/// - [`EditError::UndecodableContentStream`] when it *is* `FlateDecode` and
///   the data does not inflate to a complete stream.
/// - [`EditError::PageContentTooLarge`] when it decodes to more than
///   `budget` bytes. `budget` is what remains of the page's shared ceiling,
///   not a per-stream one — see [`MAX_PAGE_CONTENT_BYTES`].
pub(crate) fn decode(
    document: &Document,
    stream: &Stream,
    object_id: ObjectId,
    budget: usize,
) -> Result<DecodedStream, EditError> {
    match filter_of(document, &stream.dict, object_id)? {
        None => {
            if stream.content.len() > budget {
                return Err(too_large(object_id));
            }
            Ok(DecodedStream {
                bytes: stream.content.clone(),
                filtered: false,
            })
        }
        Some(Filter::Flate) => {
            reject_decode_parms(document, &stream.dict, object_id)?;
            Ok(DecodedStream {
                bytes: inflate(&stream.content, object_id, budget)?,
                filtered: true,
            })
        }
    }
}

/// Re-encodes edited bytes for a stream that arrived `FlateDecode`d.
///
/// The counterpart to [`decode`], and deliberately not `lopdf`'s
/// `Stream::compress`: that one is best-effort — it declines unless the
/// compressed form saves more than 19 bytes — so a small edited page would
/// come back out as plain bytes with the `/Filter` dropped. Valid, but no
/// longer the file the user had, and it makes "a compressed page stays
/// compressed" a promise that holds only for large pages.
///
/// Compressing unconditionally can leave a tiny stream a few bytes larger
/// than it went in. That is the right trade: the shape of the file is
/// preserved, and page content streams that are small enough for it to
/// matter are small enough for it not to.
///
/// The level is `default` (6) rather than `best` (9) because this does not
/// run once per save — it runs once per *command*. `pdf-save`'s
/// `replay_content_edits` walks the edit log one entry at a time, and each
/// entry re-reads the page (inflating every one of its streams) and writes it
/// back through here. Ten edits on one page are ten full round trips of the
/// whole stream, so the level buys a percent or two of size at a cost paid
/// over and over. Level 9's extra work is spent searching for matches in
/// operator text that has very few of them.
pub(crate) fn encode_flate(bytes: &[u8]) -> Result<Vec<u8>, EditError> {
    let mut compressor = Compress::new(Compression::default(), true);
    let mut output = Vec::with_capacity(bytes.len() / 2 + 64);

    loop {
        let before = compressor.total_out();
        let status = compressor
            .compress_vec(
                &bytes[compressor.total_in() as usize..],
                &mut output,
                FlushCompress::Finish,
            )
            .map_err(|error| EditError::MalformedContent {
                reason: format!("could not re-compress the edited content stream: {error}"),
                offset: 0,
            })?;

        match status {
            Status::StreamEnd => return Ok(output),
            Status::Ok | Status::BufError => {
                if output.len() == output.capacity() || compressor.total_out() == before {
                    output.reserve(output.capacity().max(INITIAL_CAPACITY));
                }
            }
        }
    }
}

/// The only filter this crate decodes.
enum Filter {
    Flate,
}

/// Reads `/Filter`, resolving it through indirection.
///
/// Returns `Ok(None)` for a stream with no filter — including the empty array
/// some producers write, which means the same thing.
fn filter_of(
    document: &Document,
    dict: &Dictionary,
    object_id: ObjectId,
) -> Result<Option<Filter>, EditError> {
    let Ok(entry) = dict.get(b"Filter") else {
        return Ok(None);
    };

    let names: Vec<Vec<u8>> = match dereference(document, entry) {
        Some(Object::Name(name)) => vec![name.clone()],
        Some(Object::Array(items)) => {
            let mut names = Vec::with_capacity(items.len());
            for item in items {
                // A name inside the array may itself be indirect.
                match dereference(document, item) {
                    Some(Object::Name(name)) => names.push(name.clone()),
                    // An entry we cannot even read as a name: the stream is
                    // encoded somehow and we cannot say how.
                    _ => return Err(unsupported(object_id, "unreadable /Filter entry")),
                }
            }
            names
        }
        Some(Object::Null) | None => {
            // `None` means an indirect `/Filter` pointing at an object that is
            // not there. That is not "no filter" — it is a filter we cannot
            // read, and treating it as plain hands out encoded bytes.
            return Err(unsupported(object_id, "unresolvable /Filter"));
        }
        Some(_) => {
            return Err(unsupported(
                object_id,
                "/Filter is neither a name nor an array",
            ))
        }
    };

    match names.as_slice() {
        [] => Ok(None),
        [only] if only == b"FlateDecode" => Ok(Some(Filter::Flate)),
        [only] => Err(unsupported(object_id, &String::from_utf8_lossy(only))),
        // A chain would have to be decoded *and re-encoded* in the same order
        // to write the page back, and proving that round trip for arbitrary
        // combinations is a different piece of work.
        _ => Err(unsupported(object_id, "a chain of filters")),
    }
}

/// Refuses any `/DecodeParms`.
///
/// A predictor changes what the inflated bytes mean, and re-applying it on
/// write is part of the round trip. Until that round trip is proven, a stream
/// carrying parameters is one we do not understand.
fn reject_decode_parms(
    document: &Document,
    dict: &Dictionary,
    object_id: ObjectId,
) -> Result<(), EditError> {
    for key in [b"DecodeParms".as_slice(), b"DP".as_slice()] {
        match dict.get(key).ok().and_then(|p| dereference(document, p)) {
            None | Some(Object::Null) => {}
            Some(_) => return Err(unsupported(object_id, "/DecodeParms")),
        }
    }
    Ok(())
}

/// Inflates `input`, requiring the compressed stream to actually *end*.
///
/// This is the whole point of not using `lopdf` here. `Status::StreamEnd` is
/// the decompressor saying it reached the end-of-stream marker and the check
/// held — which is the difference between "here is the page" and "here is as
/// much of the page as I got before it went wrong". A truncated stream
/// produces `Status::Ok` with a plausible-looking prefix, and that prefix is
/// exactly what must never reach a writer.
///
/// An empty result is fine when the stream ended cleanly: a page whose
/// content stream compresses to nothing is a page that paints nothing, which
/// is legal and not the same as a failure.
fn inflate(input: &[u8], object_id: ObjectId, budget: usize) -> Result<Vec<u8>, EditError> {
    // PDF's FlateDecode is zlib (RFC 1950). Raw deflate appears in files
    // written by producers that skipped the two-byte header, and is tried
    // second — but *only* when there is no valid header for the first attempt
    // to have failed on.
    //
    // That condition is the whole safety of the fallback. Retrying a merely
    // damaged zlib stream as raw deflate reads its header bytes as the start
    // of a stored block, and a stored block whose LEN/NLEN pair happens to
    // agree yields bytes that can run all the way to an end-of-stream marker
    // while meaning nothing. Raw deflate carries no Adler-32 to catch that —
    // the end of the stream is all it can prove — so the header check is what
    // stands between a damaged page and a plausible-looking rewrite of it.
    let outcome = match inflate_with(input, true, budget) {
        Err(InflateFailure::Incomplete) if !has_zlib_header(input) => {
            inflate_with(input, false, budget)
        }
        first => first,
    };

    outcome.map_err(|failure| match failure {
        InflateFailure::Incomplete => EditError::UndecodableContentStream { object_id },
        InflateFailure::TooLarge => too_large(object_id),
    })
}

/// Why an inflate did not produce a whole stream.
enum InflateFailure {
    /// Truncated, corrupt, or not deflate data at all — the stream never
    /// ended, so whatever came out is a prefix.
    Incomplete,
    /// It kept producing output past what the page is allowed to decode to.
    TooLarge,
}

/// Whether `input` opens with a well-formed zlib (RFC 1950) header: the
/// deflate compression method, and the check value FCHECK exists to make hold.
///
/// Cheap and decisive. A stream that passes this is zlib — if it then fails to
/// inflate, it is broken zlib, not raw deflate wearing a disguise.
fn has_zlib_header(input: &[u8]) -> bool {
    let [cmf, flg, ..] = input else {
        return false;
    };
    cmf & 0x0f == 8 && (u16::from(*cmf) << 8 | u16::from(*flg)) % 31 == 0
}

fn inflate_with(input: &[u8], zlib_header: bool, budget: usize) -> Result<Vec<u8>, InflateFailure> {
    let mut decompressor = Decompress::new(zlib_header);
    // Growth is bounded by `budget` rather than by trusting the stream's own
    // claims about its size.
    let mut output = Vec::with_capacity(
        input
            .len()
            .saturating_mul(4)
            .clamp(INITIAL_CAPACITY, MAX_INITIAL_CAPACITY)
            .min(budget),
    );

    loop {
        let before = decompressor.total_out();
        let status = decompressor
            .decompress_vec(
                &input[decompressor.total_in() as usize..],
                &mut output,
                // `None`, not `Finish`. `Finish` promises the output buffer is
                // big enough for what is left, and a decoder that has to grow
                // its buffer cannot make that promise — calling `Finish` again
                // after a partial write puts zlib in an error state, which
                // would surface here as "this stream is corrupt" for a stream
                // that is perfectly fine and merely large. `StreamEnd` still
                // arrives on its own when the stream really ends, which is the
                // only thing this function accepts.
                FlushDecompress::None,
            )
            .map_err(|_| InflateFailure::Incomplete)?;

        match status {
            Status::StreamEnd => return Ok(output),
            Status::Ok | Status::BufError => {
                // Checked on length, not capacity: an allocator that rounds a
                // request up must not become permission to decode further.
                if output.len() >= budget {
                    return Err(InflateFailure::TooLarge);
                }
                if output.len() == output.capacity() {
                    // Doubling, capped at the budget. `target` is strictly
                    // above `len` because `len < budget` was just established,
                    // so the buffer always grows and the loop always advances.
                    let target = output
                        .capacity()
                        .saturating_mul(2)
                        .max(INITIAL_CAPACITY)
                        .min(budget);
                    output.reserve_exact(target - output.len());
                } else if decompressor.total_out() == before {
                    // Room to spare and nothing more came out: the input ran
                    // out before the stream did. That is the truncation case,
                    // and the bytes gathered so far are a prefix of a page —
                    // exactly what must not be handed to a writer.
                    return Err(InflateFailure::Incomplete);
                }
            }
        }
    }
}

const INITIAL_CAPACITY: usize = 8 * 1024;
const MAX_INITIAL_CAPACITY: usize = 1024 * 1024;

/// A ceiling on everything **one page** decodes to, so a decompression bomb
/// cannot exhaust memory before anything has even been parsed.
///
/// Shared across the page rather than applied per stream, because `/Contents`
/// is an array of arbitrary length and nothing stops it from repeating the
/// same reference: a per-stream ceiling multiplies by the number of entries,
/// which is to say it is not a ceiling. [`super::page_streams`] threads what
/// is left of it through each decode.
///
/// Page content is operators, not image data. A heavy vector-graphics page
/// decodes to single-digit megabytes; this is one to two orders of magnitude
/// above anything real, which is where a memory guard belongs — far enough
/// out that it never argues with a legitimate file, close enough in that it
/// still bounds a hostile one.
pub(crate) const MAX_PAGE_CONTENT_BYTES: usize = 64 * 1024 * 1024;

fn unsupported(object_id: ObjectId, detail: &str) -> EditError {
    EditError::UnsupportedContentStreamFilter {
        object_id,
        detail: detail.to_string(),
    }
}

fn too_large(object_id: ObjectId) -> EditError {
    EditError::PageContentTooLarge {
        object_id,
        limit: MAX_PAGE_CONTENT_BYTES,
    }
}

fn dereference<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        direct => Some(direct),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::{DeflateEncoder, ZlibEncoder};
    use flate2::Compression;
    use lopdf::dictionary;
    use std::io::Write;

    fn zlib(payload: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(payload).expect("encode");
        encoder.finish().expect("finish")
    }

    /// Deflate with no zlib wrapper — what a producer that skipped the
    /// two-byte header emits, and the only thing the fallback is for.
    fn raw_deflate(payload: &[u8]) -> Vec<u8> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(payload).expect("encode");
        encoder.finish().expect("finish")
    }

    fn flate_stream(content: Vec<u8>) -> Stream {
        Stream::new(dictionary! { "Filter" => "FlateDecode" }, content)
    }

    /// The budget is a property of the page, not of a case under test, so
    /// every test but the one about the ceiling passes the whole thing.
    fn decode(
        document: &Document,
        stream: &Stream,
        object_id: ObjectId,
    ) -> Result<DecodedStream, EditError> {
        super::decode(document, stream, object_id, MAX_PAGE_CONTENT_BYTES)
    }

    const ID: ObjectId = (7, 0);

    #[test]
    fn a_stream_with_no_filter_is_its_own_content() {
        let document = Document::with_version("1.7");
        let stream = Stream::new(dictionary! {}, b"BT ET".to_vec());

        let decoded = decode(&document, &stream, ID).expect("plain");

        assert_eq!(decoded.bytes, b"BT ET");
        assert!(!decoded.filtered);
    }

    #[test]
    fn a_valid_flate_stream_decodes_and_is_marked_filtered() {
        let document = Document::with_version("1.7");
        let stream = flate_stream(zlib(b"BT /F1 12 Tf (hi) Tj ET"));

        let decoded = decode(&document, &stream, ID).expect("valid flate");

        assert_eq!(decoded.bytes, b"BT /F1 12 Tf (hi) Tj ET");
        assert!(decoded.filtered, "the writer has to know to re-compress");
    }

    /// The false positive the previous heuristic produced: a page that
    /// legitimately paints nothing, compressed. Empty output is only
    /// suspicious if the stream did not end — and this one did.
    #[test]
    fn a_flate_stream_that_decodes_to_nothing_is_a_legitimately_empty_page() {
        let document = Document::with_version("1.7");
        let stream = flate_stream(zlib(b""));

        let decoded = decode(&document, &stream, ID).expect("an empty page is a page");

        assert!(decoded.bytes.is_empty());
        assert!(decoded.filtered);
    }

    /// `/Filter` may be an indirect reference. A reader that only matches the
    /// direct object calls this unfiltered and hands out compressed bytes as
    /// operators.
    #[test]
    fn an_indirect_filter_is_resolved() {
        let mut document = Document::with_version("1.7");
        let filter_id = document.add_object(Object::Name(b"FlateDecode".to_vec()));
        let stream = Stream::new(
            dictionary! { "Filter" => Object::Reference(filter_id) },
            zlib(b"BT ET"),
        );

        let decoded = decode(&document, &stream, ID).expect("indirect but resolvable");

        assert_eq!(decoded.bytes, b"BT ET");
        assert!(decoded.filtered);
    }

    /// A single-element array is the same as the bare name, and its element
    /// may itself be indirect.
    #[test]
    fn a_single_element_filter_array_is_accepted_even_when_indirect() {
        let mut document = Document::with_version("1.7");
        let name_id = document.add_object(Object::Name(b"FlateDecode".to_vec()));
        let stream = Stream::new(
            dictionary! { "Filter" => vec![Object::Reference(name_id)] },
            zlib(b"BT ET"),
        );

        assert_eq!(
            decode(&document, &stream, ID).expect("resolvable").bytes,
            b"BT ET"
        );
    }

    /// The case the whole module exists for: enough of the stream inflates to
    /// look like content, but it never ends. Accepting the prefix is how a
    /// page gets truncated on save.
    #[test]
    fn a_truncated_flate_stream_is_refused_even_though_it_starts_readably() {
        let document = Document::with_version("1.7");
        let payload = b"BT /F1 12 Tf 0 700 Td (the first line, which survives) Tj \
                        0 -14 Td (the second line, which does not) Tj ET";
        let full = zlib(payload);
        let truncated = full[..full.len() - 12].to_vec();

        // The premise, asserted rather than assumed: a permissive decoder —
        // which is what `lopdf` effectively is — gets a readable prefix out
        // of these bytes. That prefix is plausible page content, which is
        // precisely why accepting it would silently truncate the page on the
        // next save.
        let prefix = permissively_inflate(&truncated);
        assert!(
            prefix.starts_with(b"BT /F1 12 Tf"),
            "the test is vacuous unless the truncated stream really does start readably"
        );
        assert!(
            prefix.len() < payload.len(),
            "and unless it really is missing the rest"
        );

        let error = decode(&document, &flate_stream(truncated), ID)
            .expect_err("an unfinished stream is not content");

        assert_eq!(error, EditError::UndecodableContentStream { object_id: ID });
    }

    /// Inflates without requiring the stream to end — the behaviour this
    /// module exists to *not* have. Test-only, to prove the danger is real.
    fn permissively_inflate(input: &[u8]) -> Vec<u8> {
        let mut decompressor = Decompress::new(true);
        let mut output = Vec::with_capacity(1024);
        let _ = decompressor.decompress_vec(input, &mut output, FlushDecompress::None);
        output
    }

    /// Some producers write the deflate data without zlib's two-byte header.
    /// The fallback exists for them, and until now nothing proved it worked.
    #[test]
    fn a_headerless_deflate_stream_is_accepted_by_the_fallback() {
        let document = Document::with_version("1.7");
        let payload = b"BT /F1 12 Tf (headerless) Tj ET";

        let decoded = decode(&document, &flate_stream(raw_deflate(payload)), ID)
            .expect("no zlib header, but a complete deflate stream");

        assert_eq!(decoded.bytes, payload);
        assert!(decoded.filtered);
    }

    /// The check that decides whether the fallback is even a plausible
    /// explanation. If this stops telling the two encodings apart, the
    /// fallback either stops working or starts rescuing damaged zlib — and
    /// the second is how garbage that reaches an end-of-stream marker gets
    /// written back as a page.
    #[test]
    fn the_zlib_header_check_tells_the_two_encodings_apart() {
        assert!(has_zlib_header(&zlib(b"BT ET")), "a real zlib stream");
        assert!(
            !has_zlib_header(&raw_deflate(b"BT ET")),
            "raw deflate must stay eligible for the fallback"
        );
        assert!(!has_zlib_header(b"this was never compressed"));
        assert!(!has_zlib_header(b"\x78"), "a header needs both its bytes");
        assert!(!has_zlib_header(b""));
    }

    /// A zlib stream whose Adler-32 no longer matches is damaged, not
    /// headerless — so it is refused rather than re-read as raw deflate,
    /// which is the one reading that could turn it into plausible content.
    #[test]
    fn a_zlib_stream_with_a_broken_checksum_is_refused() {
        let document = Document::with_version("1.7");
        let mut corrupt = zlib(b"BT /F1 12 Tf (hi) Tj ET");
        assert!(
            has_zlib_header(&corrupt),
            "the premise: these bytes still announce themselves as zlib"
        );
        *corrupt.last_mut().expect("non-empty") ^= 0xff;

        assert_eq!(
            decode(&document, &flate_stream(corrupt), ID)
                .expect_err("a stream that fails its own checksum is not content"),
            EditError::UndecodableContentStream { object_id: ID }
        );
    }

    /// The ceiling is the page's, not the stream's, and it is reported as a
    /// limit of ours rather than as a damaged file.
    #[test]
    fn content_past_the_page_ceiling_is_refused_as_a_limit_not_as_damage() {
        let document = Document::with_version("1.7");
        let stream = flate_stream(zlib(&vec![b'q'; 4096]));

        let error = super::decode(&document, &stream, ID, 1024)
            .expect_err("more than the budget left for this page");

        assert_eq!(
            error,
            EditError::PageContentTooLarge {
                object_id: ID,
                limit: MAX_PAGE_CONTENT_BYTES,
            }
        );
    }

    /// A stream that fits exactly is not over the line.
    #[test]
    fn content_that_exactly_fills_the_remaining_budget_is_accepted() {
        let document = Document::with_version("1.7");
        let payload = vec![b'q'; 4096];
        let stream = flate_stream(zlib(&payload));

        assert_eq!(
            super::decode(&document, &stream, ID, payload.len())
                .expect("exactly the budget is within it")
                .bytes,
            payload
        );
    }

    #[test]
    fn flate_bytes_that_are_not_flate_at_all_are_refused() {
        let document = Document::with_version("1.7");
        let stream = flate_stream(b"this was never compressed".to_vec());

        assert_eq!(
            decode(&document, &stream, ID).expect_err("not inflatable"),
            EditError::UndecodableContentStream { object_id: ID }
        );
    }

    /// Encode and decode are each other's inverse, and the decode half is
    /// the strict one — so a round trip proves the bytes we write back are
    /// bytes we would accept reading. Without that, an edit could produce a
    /// stream this crate then refuses to open.
    #[test]
    fn what_we_write_back_is_something_we_would_accept_reading() {
        let document = Document::with_version("1.7");

        for payload in [
            b"".as_slice(),
            b"BT ET".as_slice(),
            b"BT /F1 12 Tf 0 700 Td (round trip) Tj ET".as_slice(),
            &vec![b'q'; 200_000],
        ] {
            let encoded = encode_flate(payload).expect("encodable");
            let decoded =
                decode(&document, &flate_stream(encoded), ID).expect("strictly decodable");

            assert_eq!(decoded.bytes, payload, "payload of {} bytes", payload.len());
            assert!(decoded.filtered);
        }
    }

    #[test]
    fn an_unsupported_filter_is_refused_rather_than_guessed_at() {
        let document = Document::with_version("1.7");
        let stream = Stream::new(
            dictionary! { "Filter" => "LZWDecode" },
            b"whatever".to_vec(),
        );

        assert!(matches!(
            decode(&document, &stream, ID),
            Err(EditError::UnsupportedContentStreamFilter { .. })
        ));
    }

    /// An indirect `/Filter` whose target is missing is not "no filter" — the
    /// stream is encoded and we cannot say how.
    #[test]
    fn an_unresolvable_indirect_filter_is_refused_not_treated_as_plain() {
        let document = Document::with_version("1.7");
        let stream = Stream::new(
            dictionary! { "Filter" => Object::Reference((999, 0)) },
            zlib(b"BT ET"),
        );

        assert!(matches!(
            decode(&document, &stream, ID),
            Err(EditError::UnsupportedContentStreamFilter { .. })
        ));
    }

    #[test]
    fn a_chain_of_filters_is_refused() {
        let document = Document::with_version("1.7");
        let stream = Stream::new(
            dictionary! { "Filter" => vec!["ASCII85Decode".into(), "FlateDecode".into()] },
            zlib(b"BT ET"),
        );

        assert!(matches!(
            decode(&document, &stream, ID),
            Err(EditError::UnsupportedContentStreamFilter { .. })
        ));
    }

    /// A predictor changes what the inflated bytes mean, and re-applying it
    /// is part of writing the page back.
    #[test]
    fn decode_parms_are_refused_even_on_a_supported_filter() {
        let document = Document::with_version("1.7");
        let stream = Stream::new(
            dictionary! {
                "Filter" => "FlateDecode",
                "DecodeParms" => dictionary! { "Predictor" => 12 },
            },
            zlib(b"BT ET"),
        );

        assert!(matches!(
            decode(&document, &stream, ID),
            Err(EditError::UnsupportedContentStreamFilter { .. })
        ));
    }

    /// An empty `/Filter` array is how some producers write "plain".
    #[test]
    fn an_empty_filter_array_means_unfiltered() {
        let document = Document::with_version("1.7");
        let stream = Stream::new(
            dictionary! { "Filter" => Vec::<Object>::new() },
            b"BT ET".to_vec(),
        );

        let decoded = decode(&document, &stream, ID).expect("plain");

        assert_eq!(decoded.bytes, b"BT ET");
        assert!(!decoded.filtered, "nothing to re-compress on the way out");
    }
}
