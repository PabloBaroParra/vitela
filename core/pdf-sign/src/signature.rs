//! AcroForm signature dictionaries and serialized byte-range preparation.

use std::ops::Range;

use lopdf::{Dictionary, Object, ObjectId, StringFormat};

use crate::SignError;

/// Default number of DER bytes reserved for a detached CMS signature.
///
/// The PDF stores each byte as two hexadecimal characters, so this reserves
/// 32 KiB in the serialized file. Callers with unusually large certificate
/// chains can select a larger capacity through [`SignatureFieldBuilder`].
pub const DEFAULT_SIGNATURE_CAPACITY: usize = 16 * 1024;

const BYTE_RANGE_SLOT_WIDTH: usize = 10;
const BYTE_RANGE_SENTINEL: u64 = 9_999_999_999;
const BYTE_RANGE_MARKER: &[u8] = b"[0 9999999999 9999999999 9999999999]";
const CONTENTS_PREFIX: &[u8] = b"/Contents<";

/// The four integers stored in a PDF signature dictionary's `/ByteRange`.
///
/// The two covered regions are `[values[0], values[0] + values[1])` and
/// `[values[2], values[2] + values[3])`. The gap between them is the complete
/// hexadecimal `/Contents` string, including its angle brackets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    values: [u64; 4],
}

impl ByteRange {
    /// Returns the four PDF `/ByteRange` integers.
    #[must_use]
    pub const fn values(self) -> [u64; 4] {
        self.values
    }
}

/// AcroForm field and signature dictionaries containing an unsigned value.
///
/// The field is a combined field/widget dictionary. Its `/V` entry is an
/// indirect-reference placeholder `(0, 0)`; the save layer replaces it with
/// the real object id assigned to `signature_dictionary`, then appends the
/// field to both `/AcroForm /Fields` and the page's `/Annots` array.
#[derive(Clone, Debug)]
pub struct SignaturePlaceholder {
    /// Combined `/FT /Sig` field and `/Subtype /Widget` annotation.
    pub field_dictionary: Dictionary,
    /// `/Type /Sig` value dictionary with `/ByteRange` and `/Contents` slots.
    pub signature_dictionary: Dictionary,
    /// Maximum number of DER signature bytes that fit in `/Contents`.
    pub contents_capacity: usize,
}

/// Builder for an unsigned AcroForm signature field.
#[derive(Clone, Debug)]
pub struct SignatureFieldBuilder {
    name: String,
    page_object_id: ObjectId,
    rect: [f32; 4],
    contents_capacity: usize,
}

impl SignatureFieldBuilder {
    /// Creates a signature widget using the default `/Contents` capacity.
    ///
    /// `rect` uses PDF coordinates `[x0, y0, x1, y1]`. An all-zero rectangle
    /// creates an invisible signature field.
    #[must_use]
    pub fn new(name: impl Into<String>, page_object_id: ObjectId, rect: [f32; 4]) -> Self {
        Self {
            name: name.into(),
            page_object_id,
            rect,
            contents_capacity: DEFAULT_SIGNATURE_CAPACITY,
        }
    }

    /// Overrides the maximum DER signature size reserved in `/Contents`.
    #[must_use]
    pub const fn contents_capacity(mut self, contents_capacity: usize) -> Self {
        self.contents_capacity = contents_capacity;
        self
    }

    /// Builds the field/widget and unsigned signature dictionaries.
    ///
    /// # Errors
    ///
    /// Returns [`SignError::InvalidPlaceholderCapacity`] when the requested
    /// `/Contents` capacity is zero.
    pub fn build(self) -> Result<SignaturePlaceholder, SignError> {
        if self.contents_capacity == 0 {
            return Err(SignError::InvalidPlaceholderCapacity);
        }

        let mut signature_dictionary = Dictionary::new();
        signature_dictionary.set("Type", "Sig");
        signature_dictionary.set("Filter", "Adobe.PPKLite");
        signature_dictionary.set("SubFilter", "adbe.pkcs7.detached");
        signature_dictionary.set(
            "ByteRange",
            vec![
                Object::Integer(0),
                Object::Integer(BYTE_RANGE_SENTINEL as i64),
                Object::Integer(BYTE_RANGE_SENTINEL as i64),
                Object::Integer(BYTE_RANGE_SENTINEL as i64),
            ],
        );
        signature_dictionary.set(
            "Contents",
            Object::String(vec![0; self.contents_capacity], StringFormat::Hexadecimal),
        );

        let mut field_dictionary = Dictionary::new();
        field_dictionary.set("Type", "Annot");
        field_dictionary.set("Subtype", "Widget");
        field_dictionary.set("FT", "Sig");
        field_dictionary.set("T", Object::string_literal(self.name));
        field_dictionary.set(
            "Rect",
            self.rect.into_iter().map(Object::Real).collect::<Vec<_>>(),
        );
        field_dictionary.set("P", Object::Reference(self.page_object_id));
        field_dictionary.set("F", Object::Integer(4));
        field_dictionary.set("V", Object::Reference((0, 0)));

        Ok(SignaturePlaceholder {
            field_dictionary,
            signature_dictionary,
            contents_capacity: self.contents_capacity,
        })
    }
}

/// Serialized PDF prepared for digesting and later signature insertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedSignature {
    /// PDF bytes with the fixed-width `/ByteRange` values patched in place.
    pub bytes: Vec<u8>,
    /// The two signed byte regions surrounding `/Contents`.
    pub byte_range: ByteRange,
    /// Range of hexadecimal digits inside `/Contents`, excluding `<` and `>`.
    pub contents_hex_range: Range<usize>,
}

/// Locates one unsigned signature placeholder and patches its `/ByteRange`.
///
/// The builder writes three ten-digit sentinel integers. This function
/// replaces them with zero-padded offsets of the same width, so serialization
/// length and all previously calculated offsets remain unchanged.
///
/// # Errors
///
/// Returns [`SignError`] if the placeholder is absent or ambiguous, its
/// `/Contents` token does not match `contents_capacity`, or an offset exceeds
/// the ten-digit slots.
pub fn prepare_signature_bytes(
    mut bytes: Vec<u8>,
    contents_capacity: usize,
) -> Result<PreparedSignature, SignError> {
    if contents_capacity == 0 {
        return Err(SignError::InvalidPlaceholderCapacity);
    }

    let markers = matching_offsets(&bytes, BYTE_RANGE_MARKER);
    let marker_offset = match markers.as_slice() {
        [offset] => *offset,
        [] => return Err(SignError::PlaceholderNotFound),
        many => return Err(SignError::AmbiguousPlaceholder { count: many.len() }),
    };

    let search_start = marker_offset + BYTE_RANGE_MARKER.len();
    let contents_relative = find_subslice(&bytes[search_start..], CONTENTS_PREFIX)
        .ok_or(SignError::MalformedPlaceholder)?;
    let contents_open = search_start + contents_relative + CONTENTS_PREFIX.len() - 1;
    let hex_start = contents_open + 1;
    let hex_length = contents_capacity
        .checked_mul(2)
        .ok_or(SignError::MalformedPlaceholder)?;
    let hex_end = hex_start
        .checked_add(hex_length)
        .ok_or(SignError::MalformedPlaceholder)?;
    let contents_close = hex_end;

    if bytes.get(contents_close) != Some(&b'>')
        || bytes
            .get(hex_start..hex_end)
            .is_none_or(|hex| hex.iter().any(|byte| *byte != b'0'))
    {
        return Err(SignError::MalformedPlaceholder);
    }

    let second_offset = contents_close + 1;
    let range = ByteRange {
        values: [
            0,
            contents_open as u64,
            second_offset as u64,
            (bytes.len() - second_offset) as u64,
        ],
    };
    patch_byte_range(&mut bytes, marker_offset, range)?;

    Ok(PreparedSignature {
        bytes,
        byte_range: range,
        contents_hex_range: hex_start..hex_end,
    })
}

fn patch_byte_range(
    bytes: &mut [u8],
    marker_offset: usize,
    byte_range: ByteRange,
) -> Result<(), SignError> {
    if byte_range
        .values
        .into_iter()
        .any(|value| value > BYTE_RANGE_SENTINEL)
    {
        return Err(SignError::DocumentTooLarge {
            length: bytes.len(),
        });
    }

    let [_, first_length, second_offset, second_length] = byte_range.values;
    let replacement = format!(
        "[0 {first_length:0width$} {second_offset:0width$} {second_length:0width$}]",
        width = BYTE_RANGE_SLOT_WIDTH
    );
    let marker_end = marker_offset + BYTE_RANGE_MARKER.len();
    bytes[marker_offset..marker_end].copy_from_slice(replacement.as_bytes());
    Ok(())
}

fn matching_offsets(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut search_start = 0;
    while let Some(relative) = find_subslice(&haystack[search_start..], needle) {
        let offset = search_start + relative;
        offsets.push(offset);
        search_start = offset + needle.len();
    }
    offsets
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serialized_placeholder(capacity: usize) -> Vec<u8> {
        let placeholder = SignatureFieldBuilder::new("Signature_1", (1, 0), [0.0; 4])
            .contents_capacity(capacity)
            .build()
            .expect("test placeholder should build");
        let mut document = lopdf::Document::with_version("1.7");
        document.add_object(Object::Dictionary(placeholder.signature_dictionary));
        let mut bytes = Vec::new();
        document
            .save_to(&mut bytes)
            .expect("test placeholder should serialize");
        bytes
    }

    #[test]
    fn builder_uses_default_contents_capacity() {
        let placeholder = SignatureFieldBuilder::new("Signature_1", (1, 0), [0.0; 4])
            .build()
            .expect("default placeholder should build");

        assert_eq!(placeholder.contents_capacity, DEFAULT_SIGNATURE_CAPACITY);
    }

    #[test]
    fn builder_rejects_zero_contents_capacity() {
        let error = SignatureFieldBuilder::new("Signature_1", (1, 0), [0.0; 4])
            .contents_capacity(0)
            .build()
            .expect_err("zero capacity should fail");

        assert_eq!(error, SignError::InvalidPlaceholderCapacity);
    }

    #[test]
    fn builder_creates_combined_signature_widget() {
        let placeholder = SignatureFieldBuilder::new("Approval", (7, 0), [10.0, 20.0, 110.0, 60.0])
            .build()
            .expect("signature widget should build");

        assert_eq!(
            placeholder
                .field_dictionary
                .get(b"FT")
                .and_then(Object::as_name)
                .expect("field type should be a name"),
            b"Sig"
        );
    }

    #[test]
    fn builder_reserves_hexadecimal_zero_contents() {
        let placeholder = SignatureFieldBuilder::new("Signature_1", (1, 0), [0.0; 4])
            .contents_capacity(4)
            .build()
            .expect("signature placeholder should build");
        let contents = placeholder
            .signature_dictionary
            .get(b"Contents")
            .expect("signature dictionary should contain /Contents");

        assert_eq!(
            contents,
            &Object::String(vec![0; 4], StringFormat::Hexadecimal)
        );
    }

    #[test]
    fn prepare_signature_bytes_patches_offsets_without_changing_length() {
        let original = serialized_placeholder(8);
        let original_length = original.len();

        let prepared = prepare_signature_bytes(original, 8)
            .expect("serialized placeholder should be prepared");

        assert_eq!(prepared.bytes.len(), original_length);
    }

    #[test]
    fn prepared_byte_range_excludes_complete_contents_token() {
        let prepared = prepare_signature_bytes(serialized_placeholder(8), 8)
            .expect("serialized placeholder should be prepared");
        let [first_offset, first_length, second_offset, second_length] =
            prepared.byte_range.values();

        assert_eq!(
            (first_offset, first_length, second_offset, second_length),
            (
                0,
                (prepared.contents_hex_range.start - 1) as u64,
                (prepared.contents_hex_range.end + 1) as u64,
                (prepared.bytes.len() - prepared.contents_hex_range.end - 1) as u64,
            )
        );
    }

    #[test]
    fn prepare_signature_bytes_rejects_missing_placeholder() {
        let error = prepare_signature_bytes(b"%PDF-1.7\n%%EOF".to_vec(), 8)
            .expect_err("missing placeholder should fail");

        assert_eq!(error, SignError::PlaceholderNotFound);
    }

    #[test]
    fn prepare_signature_bytes_rejects_multiple_placeholders() {
        let one = serialized_placeholder(8);
        let mut two = one.clone();
        two.extend_from_slice(&one);

        let error =
            prepare_signature_bytes(two, 8).expect_err("ambiguous placeholders should fail safely");

        assert_eq!(error, SignError::AmbiguousPlaceholder { count: 2 });
    }
}
