//! PDF text-string encoding shared by every writer that stores user-typed
//! text in a PDF dictionary (document info, annotation `/Contents`, ...).

use lopdf::{Object, StringFormat};

/// Encodes a PDF text string per ISO 32000-2 §7.9.2.2 (batch decision 7):
/// PDFDocEncoding when the text fits, UTF-16BE with a leading `FE FF`
/// byte-order mark otherwise. There is no error path: UTF-16BE covers all of
/// Unicode, so every `String` a user can type is representable one way or
/// the other.
///
/// "Fits" is deliberately conservative — printable ASCII (0x20-0x7E) only,
/// the exact range `pdf_manip::document`'s own decoder documents as the part
/// of PDFDocEncoding it (and this function) treats as unambiguous. Real
/// PDFDocEncoding also covers most of Latin-1's upper half, but remaps
/// 0x18-0x1F and 0x80-0x9F to typographic marks Latin-1 does not have at
/// those code points — writing a Latin-1 byte there on the assumption that
/// PDFDocEncoding agrees would silently corrupt the text on any reader that
/// implements the encoding correctly. Falling back to UTF-16BE for anything
/// outside the safe range costs a few extra bytes; it never costs
/// correctness.
pub fn encode_pdf_text_string(text: &str) -> Vec<u8> {
    if text.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
        return text.as_bytes().to_vec();
    }

    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

/// [`encode_pdf_text_string`] wrapped as a literal-string `lopdf::Object`,
/// ready to `set` on a dictionary.
pub fn pdf_text_string_object(text: &str) -> Object {
    Object::String(encode_pdf_text_string(text), StringFormat::Literal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_ascii_is_written_verbatim() {
        assert_eq!(encode_pdf_text_string("Report 1"), b"Report 1");
    }

    #[test]
    fn non_ascii_becomes_utf16_be_with_a_bom() {
        assert_eq!(
            encode_pdf_text_string("Ñ€"),
            vec![0xFE, 0xFF, 0x00, 0xD1, 0x20, 0xAC]
        );
    }

    #[test]
    fn control_characters_force_utf16_be() {
        assert_eq!(&encode_pdf_text_string("a\u{1f}")[..2], &[0xFE, 0xFF]);
    }
}
