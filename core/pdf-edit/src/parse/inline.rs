//! Inline images (`BI … ID … EI`): where one ends, and what it declares.
//!
//! An inline image carries its samples in the content stream itself, which
//! makes finding its end the whole problem. `ID` is followed by raw binary
//! that can contain anything — `EI`, `(`, an entire fake operator — and the
//! format gives no length prefix before PDF 2.0, so a reader that scans for
//! the next `EI` is guessing, and a guess that lands early cuts the image in
//! half and resynchronises the operator stream on binary noise.
//!
//! So the end is derived, and only guessed at when it cannot be:
//!
//! 1. **`/L` (`/Length`)** — the explicit length PDF 2.0 added. Trusted only
//!    when an `EI` actually follows it, because a wrong `/L` must not be
//!    allowed to truncate the image silently.
//! 2. **Unfiltered samples** — exact arithmetic from the dictionary:
//!    `ceil(W × BPC × components / 8) × H`. No heuristic at all, and this is
//!    the common case for the small images inline encoding exists for.
//! 3. **Anything else** — the whitespace-delimited `EI` scan, the same
//!    heuristic viewers use. A filter's output length is not derivable from
//!    the dictionary, so there is nothing better available.
//!
//! Both derived ends are *verified* before they are used, and a failed
//! verification falls through to the scan rather than to an error: a
//! dictionary this module cannot read is not the same thing as a broken
//! stream.

use super::lexer::{self, Operand};
use crate::error::EditError;
use std::ops::Range;

/// One `BI … ID … EI` run, located in the stream that paints it.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImage {
    /// The image dictionary's entries, in file order and with the keys
    /// exactly as written — the abbreviations (`/W`, `/H`, `/BPC`, …) are
    /// deliberately not expanded, because anything that rewrites this image
    /// has to put back what it found, not a normalised version of it.
    pub entries: Vec<(String, Operand)>,
    /// The samples: the bytes between the single whitespace after `ID` and
    /// the `EI`, delimiting whitespace excluded.
    pub data: Range<usize>,
    /// Offset just past the `EI` — the end of the whole operation.
    pub end: usize,
}

impl InlineImage {
    /// The value of an entry, looked up under both spellings the format
    /// allows: `entry("W", "Width")`.
    ///
    /// Abbreviated first, because that is what producers write; a file that
    /// uses both spellings for one key is malformed, and this reads the
    /// abbreviation rather than pretending to adjudicate.
    pub fn entry(&self, abbreviated: &str, full: &str) -> Option<&Operand> {
        entry(&self.entries, abbreviated, full)
    }

    /// The same image as a standalone image XObject stream: every
    /// abbreviated key and colour space and filter name expanded to what an
    /// XObject dictionary spells it, and the samples as the stream's
    /// content.
    ///
    /// `stream_bytes` is the decoded content stream this image was parsed
    /// out of, which is what [`InlineImage::data`] indexes into.
    ///
    /// This exists so that a reader written against image XObjects
    /// ([`crate::image_source_bytes`]) can read an inline image without
    /// learning a second dictionary vocabulary. It is deliberately **not** a
    /// conversion for writing: `/L` is dropped, because a stream carries its
    /// own `/Length`, and nothing here resolves a colour space that names a
    /// page resource — the reader refuses those, and refusing beats guessing
    /// at a component count.
    pub fn as_image_stream(&self, stream_bytes: &[u8]) -> Option<lopdf::Stream> {
        let samples = stream_bytes.get(self.data.clone())?.to_vec();

        let mut dict = lopdf::Dictionary::new();
        dict.set("Type", lopdf::Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", lopdf::Object::Name(b"Image".to_vec()));
        for (key, value) in &self.entries {
            let key = expanded_key(key);
            if key == "Length" {
                continue;
            }
            let expand: fn(&str) -> &str = match key {
                "ColorSpace" => expanded_color_space,
                "Filter" => expanded_filter,
                _ => |name| name,
            };
            dict.set(key, object(value, expand));
        }

        Some(lopdf::Stream::new(dict, samples))
    }
}

/// Reads the inline image whose `BI` starts at `bi_offset`.
pub fn parse(bytes: &[u8], bi_offset: usize) -> Result<InlineImage, EditError> {
    if bytes.get(bi_offset..bi_offset.saturating_add(2)) != Some(b"BI".as_slice()) {
        return Err(malformed("not an inline image", bi_offset));
    }
    parse_body(bytes, bi_offset, bi_offset + 2)
}

/// The same read for the lexer, which has already consumed the `BI`.
pub(super) fn parse_body(
    bytes: &[u8],
    bi_offset: usize,
    dictionary_offset: usize,
) -> Result<InlineImage, EditError> {
    let (entries, data_start) = lexer::read_inline_entries(bytes, bi_offset, dictionary_offset)?;

    let derived = [explicit_length(&entries), unfiltered_length(&entries)]
        .into_iter()
        .flatten()
        .filter_map(|length| data_start.checked_add(length))
        .find_map(|data_end| ei_at(bytes, data_end).map(|end| (data_end, end)));

    let (data_end, end) = match derived {
        Some(found) => found,
        None => scan_for_ei(bytes, data_start)
            .ok_or_else(|| malformed("unterminated inline image", bi_offset))?,
    };

    Ok(InlineImage {
        entries,
        data: data_start..data_end,
        end,
    })
}

fn malformed(reason: &str, offset: usize) -> EditError {
    EditError::MalformedContent {
        reason: reason.to_string(),
        offset,
    }
}

fn entry<'a>(
    entries: &'a [(String, Operand)],
    abbreviated: &str,
    full: &str,
) -> Option<&'a Operand> {
    entries
        .iter()
        .find(|(key, _)| key == abbreviated)
        .or_else(|| entries.iter().find(|(key, _)| key == full))
        .map(|(_, value)| value)
}

/// `/L`, the length the file states. Absent from everything written before
/// PDF 2.0, which is why it is one source of truth rather than the source.
fn explicit_length(entries: &[(String, Operand)]) -> Option<usize> {
    match entry(entries, "L", "Length")? {
        Operand::Integer(length) if *length >= 0 => usize::try_from(*length).ok(),
        _ => None,
    }
}

/// The exact byte count of unfiltered samples, or `None` when the dictionary
/// does not say enough to compute one.
///
/// `None` is the honest answer for a colour space named out of the page's
/// `/Resources /ColorSpace`: how many components it has is knowable, but not
/// from here — this module reads the stream, not the document — and a wrong
/// component count would produce a confidently wrong end.
fn unfiltered_length(entries: &[(String, Operand)]) -> Option<usize> {
    if is_filtered(entries) {
        return None;
    }

    let width = positive(entry(entries, "W", "Width")?)?;
    let height = positive(entry(entries, "H", "Height")?)?;

    // A stencil mask paints coverage, not colour: one bit per sample, and
    // `/BPC` and `/CS` are not written at all (PDF 32000-1 8.9.6.2).
    let is_mask = matches!(
        entry(entries, "IM", "ImageMask"),
        Some(Operand::Boolean(true))
    );
    let (bits, components) = if is_mask {
        (1, 1)
    } else {
        (
            positive(entry(entries, "BPC", "BitsPerComponent")?)?,
            components(entry(entries, "CS", "ColorSpace")?)?,
        )
    };

    // Each row is padded to a byte boundary, and the padding is part of the
    // data.
    let row = width
        .checked_mul(bits)?
        .checked_mul(components)?
        .checked_add(7)?
        / 8;
    usize::try_from(row.checked_mul(height)?).ok()
}

fn is_filtered(entries: &[(String, Operand)]) -> bool {
    match entry(entries, "F", "Filter") {
        None | Some(Operand::Null) => false,
        Some(Operand::Array(filters)) => !filters.is_empty(),
        Some(_) => true,
    }
}

fn positive(value: &Operand) -> Option<u64> {
    match value {
        Operand::Integer(value) if *value > 0 => u64::try_from(*value).ok(),
        _ => None,
    }
}

/// How many components one sample of this colour space carries.
///
/// The device spaces and their inline abbreviations, plus `Indexed`, whose
/// samples are one index each whatever the base space is. A CIE-based space
/// written out as an array (`[/CalRGB << … >>]`) is deliberately absent: its
/// component count lives in a dictionary this module would have to start
/// interpreting, and getting it wrong is worse than falling back to the scan.
fn components(space: &Operand) -> Option<u64> {
    match space {
        Operand::Name(name) => match name.as_str() {
            "G" | "DeviceGray" => Some(1),
            "RGB" | "DeviceRGB" => Some(3),
            "CMYK" | "DeviceCMYK" => Some(4),
            _ => None,
        },
        Operand::Array(items) => match items.first() {
            Some(Operand::Name(name)) if name == "I" || name == "Indexed" => Some(1),
            _ => None,
        },
        _ => None,
    }
}

/// Confirms an `EI` sits at `at`, allowing the whitespace the format
/// recommends between the samples and it. Returns the offset just past it.
fn ei_at(bytes: &[u8], at: usize) -> Option<usize> {
    let mut position = at;
    while bytes.get(position).is_some_and(|byte| is_whitespace(*byte)) {
        position += 1;
    }
    if bytes.get(position..position.saturating_add(2)) != Some(b"EI".as_slice()) {
        return None;
    }
    let end = position + 2;
    // An `EI` that runs straight into a regular character is the start of a
    // longer token, not the terminator.
    match bytes.get(end) {
        None => Some(end),
        Some(byte) if is_whitespace(*byte) || is_delimiter(*byte) => Some(end),
        Some(_) => None,
    }
}

/// The fallback: the first `EI` preceded by whitespace and followed by
/// whitespace, a delimiter or the end of the stream.
///
/// Returns where the samples end and where the operation ends — the
/// delimiting whitespace belongs to neither.
fn scan_for_ei(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut position = from;
    while position < bytes.len() {
        if position > 0 && is_whitespace(bytes[position - 1]) {
            if let Some(end) = ei_at(bytes, position) {
                // The delimiter before an `EI` that sits at the very start
                // of the samples is `ID`'s own separator, not part of them.
                return Some((position.saturating_sub(1).max(from), end));
            }
        }
        position += 1;
    }
    None
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// The key an image XObject's dictionary spells this inline key with
/// (PDF 32000-1 Table 93). Anything not abbreviated is already the full
/// name and passes through.
fn expanded_key(key: &str) -> &str {
    match key {
        "BPC" => "BitsPerComponent",
        "CS" => "ColorSpace",
        "D" => "Decode",
        "DP" => "DecodeParms",
        "F" => "Filter",
        "H" => "Height",
        "IM" => "ImageMask",
        "I" => "Interpolate",
        "L" => "Length",
        "W" => "Width",
        other => other,
    }
}

/// The colour space abbreviations (PDF 32000-1 Table 94). A space named out
/// of the page's `/Resources /ColorSpace` is not one of these and passes
/// through as the name it is — whoever reads the result resolves it or
/// refuses, which is not this module's call to make.
fn expanded_color_space(name: &str) -> &str {
    match name {
        "G" => "DeviceGray",
        "RGB" => "DeviceRGB",
        "CMYK" => "DeviceCMYK",
        "I" => "Indexed",
        other => other,
    }
}

/// The filter abbreviations (PDF 32000-1 Table 94).
fn expanded_filter(name: &str) -> &str {
    match name {
        "AHx" => "ASCIIHexDecode",
        "A85" => "ASCII85Decode",
        "LZW" => "LZWDecode",
        "Fl" => "FlateDecode",
        "RL" => "RunLengthDecode",
        "CCF" => "CCITTFaxDecode",
        "DCT" => "DCTDecode",
        other => other,
    }
}

/// An operand as the object a dictionary holds, expanding every `/Name` it
/// meets — at the top level and inside arrays — through `expand`.
///
/// A nested dictionary (`/DP << … >>`) is copied verbatim: its keys belong
/// to the filter's own vocabulary, which shares no abbreviations with the
/// image dictionary's.
fn object(value: &Operand, expand: fn(&str) -> &str) -> lopdf::Object {
    use lopdf::Object;

    match value {
        Operand::Integer(value) => Object::Integer(*value),
        // `lopdf` stores a real as `f32`; nothing in an image dictionary is
        // a real to begin with, so the narrowing costs nothing here.
        Operand::Real(value) => Object::Real(*value as f32),
        Operand::Name(name) => Object::Name(expand(name).as_bytes().to_vec()),
        Operand::LiteralString(bytes) | Operand::HexString(bytes) => {
            Object::String(bytes.clone(), lopdf::StringFormat::Literal)
        }
        Operand::Array(items) => {
            Object::Array(items.iter().map(|item| object(item, expand)).collect())
        }
        Operand::Dictionary(entries) => {
            let mut dict = lopdf::Dictionary::new();
            for (key, value) in entries {
                dict.set(key.as_str(), object(value, |name| name));
            }
            Object::Dictionary(dict)
        }
        Operand::Boolean(value) => Object::Boolean(*value),
        Operand::Null => Object::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(source: &[u8]) -> InlineImage {
        let bi = source
            .windows(2)
            .position(|pair| pair == b"BI")
            .expect("the fixture contains a BI");
        parse(source, bi).expect("a readable inline image")
    }

    #[test]
    fn the_dictionary_is_read_under_both_spellings() {
        let parsed = image(b"BI /W 2 /Height 3 /BPC 8 /CS /G ID \x00\x01\x02\x03\x04\x05 EI");

        assert_eq!(parsed.entry("W", "Width"), Some(&Operand::Integer(2)));
        assert_eq!(parsed.entry("H", "Height"), Some(&Operand::Integer(3)));
        assert_eq!(
            parsed.entry("CS", "ColorSpace"),
            Some(&Operand::Name("G".to_string()))
        );
    }

    /// The case the byte scan gets wrong: samples that *contain* a
    /// whitespace-delimited `EI`. 2×2 grey at 8 bits is exactly four bytes,
    /// and the arithmetic says so — so the `EI` inside them is data.
    #[test]
    fn unfiltered_samples_end_where_the_dictionary_says_they_do() {
        let source: &[u8] = b"BI /W 2 /H 2 /BPC 8 /CS /G ID \x20EI\x20 EI Q";

        let parsed = image(source);

        assert_eq!(&source[parsed.data.clone()], b"\x20EI\x20");
        assert_eq!(&source[parsed.end..], b" Q");
    }

    /// One bit per sample, no `/BPC` and no `/CS` — and every row padded to
    /// a byte, so 17 pixels across still cost three bytes a row.
    #[test]
    fn a_stencil_mask_is_one_padded_bit_per_sample() {
        let source: &[u8] = b"BI /W 17 /H 2 /IM true ID \xff EI\x00\x01 EI Q";

        let parsed = image(source);

        assert_eq!(&source[parsed.data.clone()], b"\xff EI\x00\x01");
        assert_eq!(&source[parsed.end..], b" Q");
    }

    #[test]
    fn an_explicit_length_is_used_when_an_ei_follows_it() {
        let source: &[u8] = b"BI /W 9 /H 9 /F /Fl /L 5 ID ab EI EI Q";

        let parsed = image(source);

        assert_eq!(&source[parsed.data.clone()], b"ab EI");
    }

    /// A `/L` that does not land on an `EI` is a lie, and believing it would
    /// cut the image at an arbitrary byte. The scan is the answer, not an
    /// error — the image itself is fine.
    #[test]
    fn an_explicit_length_that_lands_nowhere_falls_back_to_the_scan() {
        let source: &[u8] = b"BI /W 9 /H 9 /F /Fl /L 400 ID abcd EI Q";

        let parsed = image(source);

        assert_eq!(&source[parsed.data.clone()], b"abcd");
    }

    /// Filtered samples have no derivable length: the dictionary describes
    /// the image, not the compressed bytes.
    #[test]
    fn filtered_samples_fall_back_to_the_scan() {
        let source: &[u8] = b"BI /W 2 /H 2 /BPC 8 /CS /G /F /AHx ID 00112233> EI Q";

        let parsed = image(source);

        assert_eq!(&source[parsed.data.clone()], b"00112233>");
    }

    /// A colour space named out of the page's resources: the component count
    /// is not knowable from the stream, so the arithmetic declines instead
    /// of assuming one.
    #[test]
    fn a_named_colour_space_falls_back_to_the_scan() {
        let source: &[u8] = b"BI /W 2 /H 2 /BPC 8 /CS /Spot ID abcdef EI Q";

        let parsed = image(source);

        assert_eq!(&source[parsed.data.clone()], b"abcdef");
    }

    /// Indexed samples are one index each, whatever the base space is —
    /// three components' worth of lookup table does not make three bytes.
    #[test]
    fn an_indexed_space_is_one_component() {
        let source: &[u8] = b"BI /W 2 /H 3 /BPC 8 /CS [/I /RGB 1 <000000ffffff>] ID ab EI\x00 EI Q";

        let parsed = image(source);

        assert_eq!(&source[parsed.data.clone()], b"ab EI\x00");
        assert_eq!(&source[parsed.end..], b" Q");
    }

    /// `EIx` is a token that starts with `EI`, not a terminator.
    #[test]
    fn an_ei_that_runs_into_a_regular_character_is_not_the_end() {
        let source: &[u8] = b"BI /W 9 /H 9 /F /Fl ID ab EIx cd EI Q";

        let parsed = image(source);

        assert_eq!(&source[parsed.data.clone()], b"ab EIx cd");
    }

    #[test]
    fn an_unterminated_inline_image_is_an_error() {
        let source: &[u8] = b"q BI /W 2 /H 2 /F /Fl ID abcdef";

        let error = parse(source, 2).expect_err("there is no EI");

        assert_eq!(
            error,
            EditError::MalformedContent {
                reason: "unterminated inline image".to_string(),
                offset: 2,
            }
        );
    }

    #[test]
    fn parsing_something_that_is_not_an_inline_image_is_an_error() {
        let error = parse(b"q Q", 0).expect_err("that is not a BI");

        assert_eq!(
            error,
            EditError::MalformedContent {
                reason: "not an inline image".to_string(),
                offset: 0,
            }
        );
    }

    // --- as_image_stream --------------------------------------------------

    /// Every abbreviation in PDF 32000-1 Table 93, in one dictionary. A key
    /// left unexpanded reads as absent to anything that speaks XObject — and
    /// "absent `/Width`" is indistinguishable from "unreadable image" by the
    /// time the refusal reaches a user.
    #[test]
    fn as_image_stream_expands_every_abbreviated_key() {
        let source: &[u8] = b"BI /W 2 /H 2 /BPC 8 /CS /G /D [0 1] /DP null /F /Fl \
                              /IM false /I true /L 4 ID \x20EI\x20 EI";
        let stream = image(source)
            .as_image_stream(source)
            .expect("the data range is inside the source");

        let keys: Vec<&str> = stream
            .dict
            .iter()
            .map(|(key, _)| std::str::from_utf8(key).expect("ascii"))
            .collect();
        assert_eq!(
            keys,
            vec![
                "Type",
                "Subtype",
                "Width",
                "Height",
                "BitsPerComponent",
                "ColorSpace",
                "Decode",
                "DecodeParms",
                "Filter",
                "ImageMask",
                "Interpolate",
                "Length",
            ]
        );
        assert_eq!(
            stream.dict.get(b"Length").expect("a stream length"),
            &lopdf::Object::Integer(4),
            "the length is the stream's own, not the `/L` the file wrote"
        );
    }

    /// Names carry abbreviations of their own, and only in the two entries
    /// that have a vocabulary: `/I` is `Indexed` as a colour space and
    /// `Interpolate` as a key, and expanding the wrong one is how that goes
    /// wrong.
    #[test]
    fn as_image_stream_expands_colour_space_and_filter_names() {
        let source: &[u8] =
            b"BI /W 1 /H 1 /BPC 8 /CS [/I /RGB 1 <000000>] /F [/A85 /DCT] /L 1 ID x EI";
        let stream = image(source)
            .as_image_stream(source)
            .expect("the data range is inside the source");

        assert_eq!(
            stream.dict.get(b"ColorSpace").expect("a colour space"),
            &lopdf::Object::Array(vec![
                lopdf::Object::Name(b"Indexed".to_vec()),
                lopdf::Object::Name(b"DeviceRGB".to_vec()),
                lopdf::Object::Integer(1),
                lopdf::Object::String(vec![0, 0, 0], lopdf::StringFormat::Literal),
            ])
        );
        assert_eq!(
            stream.dict.get(b"Filter").expect("a filter"),
            &lopdf::Object::Array(vec![
                lopdf::Object::Name(b"ASCII85Decode".to_vec()),
                lopdf::Object::Name(b"DCTDecode".to_vec()),
            ])
        );
    }

    /// A colour space named out of the page's `/Resources /ColorSpace` is
    /// not an abbreviation and must not be mangled into one.
    #[test]
    fn as_image_stream_passes_a_named_colour_space_through() {
        let source: &[u8] = b"BI /W 1 /H 1 /BPC 8 /CS /Cs1 /L 3 ID abc EI";
        let stream = image(source)
            .as_image_stream(source)
            .expect("the data range is inside the source");

        assert_eq!(
            stream.dict.get(b"ColorSpace").expect("a colour space"),
            &lopdf::Object::Name(b"Cs1".to_vec())
        );
        assert_eq!(stream.content, b"abc");
    }
}
