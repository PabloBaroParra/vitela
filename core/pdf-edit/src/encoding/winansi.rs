//! WinAnsi (Windows-1252) encoding and Helvetica/Times-Roman/Courier widths
//! for the whole 0x20..=0xFF range.
//!
//! `tables::HELVETICA_ASCII_WIDTHS` stops at 0x7E on purpose (it exists for
//! T-161's content-edit sizing). Free-text annotations and form fields paint
//! Spanish text, so they need the accented Latin-1 block and the typographic
//! punctuation too.

use crate::encoding::tables::{
    AsciiWidths, COURIER_ASCII_WIDTH, HELVETICA_ASCII_WIDTHS, TIMES_ROMAN_ASCII_WIDTHS,
    WIN_ANSI_HIGH,
};

/// Encodes `text` as WinAnsi bytes: one byte per character.
///
/// Strict, like every encoder in this module: the first character with no
/// WinAnsi code is returned as `Err(character)` so the caller can name it,
/// and nothing is substituted. Control characters (including `\n`) are not
/// text and are refused; callers split lines before encoding.
pub fn encode_winansi(text: &str) -> Result<Vec<u8>, char> {
    text.chars().map(winansi_code).collect()
}

fn winansi_code(character: char) -> Result<u8, char> {
    match u32::from(character) {
        code @ (0x20..=0x7E | 0xA0..=0xFF) => Ok(code as u8),
        _ => WIN_ANSI_HIGH
            .iter()
            .find(|(_, mapped)| *mapped == character)
            .map(|(code, _)| *code)
            .ok_or(character),
    }
}

/// Helvetica's advance width of `character`, in 1/1000 em, or `None` when
/// WinAnsi cannot show it.
pub fn char_width_thousandths(character: char) -> Option<u16> {
    let code = winansi_code(character).ok()?;
    Some(HELVETICA_WINANSI_WIDTHS[code as usize])
}

/// `base_font`'s advance width of `character`, in 1/1000 em, for the
/// Standard-14 faces a form field can be styled with. `None` when WinAnsi
/// cannot show the character, or for any other font.
pub fn standard_14_char_width(base_font: &str, character: char) -> Option<u16> {
    let code = winansi_code(character).ok()?;
    match base_font {
        "Helvetica" => Some(HELVETICA_WINANSI_WIDTHS[code as usize]),
        "Times-Roman" => Some(TIMES_ROMAN_WINANSI_WIDTHS[code as usize]),
        // Monospaced: every code WinAnsi defines is one cell wide.
        "Courier" => Some(COURIER_ASCII_WIDTH),
        _ => None,
    }
}

/// Helvetica widths (Adobe Core14 `Helvetica.afm`) indexed by WinAnsi code.
/// Codes that WinAnsi leaves undefined, and the C0 controls, are 0.
pub const HELVETICA_WINANSI_WIDTHS: [u16; 256] = build_widths(&HELVETICA_ASCII_WIDTHS, &HIGH);

/// Times-Roman widths (Adobe Core14 `Times-Roman.afm`), same layout as
/// [`HELVETICA_WINANSI_WIDTHS`].
pub const TIMES_ROMAN_WINANSI_WIDTHS: [u16; 256] =
    build_widths(&TIMES_ROMAN_ASCII_WIDTHS, &TIMES_ROMAN_HIGH);

/// Times-Roman 0x80..=0xFF, in code order.
const TIMES_ROMAN_HIGH: [u16; 128] = [
    // 0x80
    500, 0, 333, 500, 444, 1000, 500, 500, 333, 1000, 556, 333, 889, 0, 611, 0, // 0x90
    0, 333, 333, 444, 444, 350, 500, 1000, 333, 980, 389, 333, 722, 0, 444, 722, // 0xA0
    250, 333, 500, 500, 500, 500, 200, 500, 333, 760, 276, 500, 564, 333, 760, 333,
    // 0xB0
    400, 564, 300, 300, 333, 500, 453, 250, 333, 300, 310, 500, 750, 750, 750, 444,
    // 0xC0
    722, 722, 722, 722, 722, 722, 889, 667, 611, 611, 611, 611, 333, 333, 333, 333,
    // 0xD0
    722, 722, 722, 722, 722, 722, 722, 564, 722, 722, 722, 722, 722, 722, 556, 500,
    // 0xE0
    444, 444, 444, 444, 444, 444, 667, 444, 444, 444, 444, 444, 278, 278, 278, 278,
    // 0xF0
    500, 500, 500, 500, 500, 500, 500, 564, 500, 500, 500, 500, 500, 500, 500, 500,
];

/// Helvetica 0x80..=0xFF, in code order.
const HIGH: [u16; 128] = [
    // 0x80
    556, 0, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, // 0x90
    0, 222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 0, 500, 667, // 0xA0
    278, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333,
    // 0xB0
    400, 584, 333, 333, 333, 556, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611,
    // 0xC0
    667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278,
    // 0xD0
    722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611,
    // 0xE0
    556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278,
    // 0xF0
    556, 556, 556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500,
];

const fn build_widths(ascii: &AsciiWidths, high: &[u16; 128]) -> [u16; 256] {
    let mut widths = [0u16; 256];
    let mut i = 0;
    while i < ascii.len() {
        widths[0x20 + i] = ascii[i];
        i += 1;
    }
    let mut j = 0;
    while j < high.len() {
        widths[0x80 + j] = high[j];
        j += 1;
    }
    widths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::tables::{HELVETICA_ASCII_WIDTHS, TIMES_ROMAN_ASCII_WIDTHS};

    #[test]
    fn encodes_ascii_as_itself() {
        assert_eq!(encode_winansi("Hola"), Ok(b"Hola".to_vec()));
    }

    #[test]
    fn encodes_latin1_accents_as_single_bytes() {
        // 'é' is 0xE9, not the UTF-8 pair C3 A9.
        assert_eq!(encode_winansi("café"), Ok(vec![b'c', b'a', b'f', 0xE9]));
        assert_eq!(encode_winansi("ñ¿Ü"), Ok(vec![0xF1, 0xBF, 0xDC]));
    }

    #[test]
    fn encodes_the_windows_1252_block() {
        assert_eq!(encode_winansi("€"), Ok(vec![0x80]));
        assert_eq!(encode_winansi("—"), Ok(vec![0x97]));
    }

    #[test]
    fn refuses_a_character_winansi_cannot_show() {
        assert_eq!(encode_winansi("Hola 日本"), Err('日'));
        assert_eq!(encode_winansi("ok\u{0081}"), Err('\u{0081}'));
    }

    #[test]
    fn newline_is_not_encodable_text() {
        // Layout splits on '\n' before encoding; a stray control is refused.
        assert_eq!(encode_winansi("a\nb"), Err('\n'));
    }

    #[test]
    fn width_spot_checks_match_the_afm() {
        let w = char_width_thousandths;
        assert_eq!(w('é'), Some(556));
        assert_eq!(w('ñ'), Some(556));
        assert_eq!(w('Á'), Some(667));
        assert_eq!(w('¿'), Some(611));
        assert_eq!(w('€'), Some(556));
        assert_eq!(w(' '), Some(278));
        assert_eq!(w('—'), Some(1000));
        assert_eq!(w('日'), None);
    }

    #[test]
    fn ascii_slice_equals_the_ascii_table() {
        assert_eq!(
            &HELVETICA_WINANSI_WIDTHS[0x20..=0x7E],
            &HELVETICA_ASCII_WIDTHS[..]
        );
    }

    #[test]
    fn every_defined_code_has_a_width() {
        let undefined = [0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D];
        for (code, &width) in HELVETICA_WINANSI_WIDTHS.iter().enumerate().skip(0x20) {
            if undefined.contains(&code) {
                assert_eq!(width, 0, "code {code:#04X} is undefined");
            } else {
                assert!(width > 0, "code {code:#04X} has no width");
            }
        }
    }

    #[test]
    fn standard_14_widths_cover_accents_in_every_form_font() {
        let w = standard_14_char_width;
        assert_eq!(w("Helvetica", 'é'), Some(556));
        assert_eq!(w("Times-Roman", 'é'), Some(444));
        assert_eq!(w("Times-Roman", 'ñ'), Some(500));
        assert_eq!(w("Times-Roman", 'Á'), Some(722));
        assert_eq!(w("Times-Roman", '¿'), Some(444));
        assert_eq!(w("Times-Roman", '€'), Some(500));
        assert_eq!(w("Courier", 'ñ'), Some(600));
        assert_eq!(w("Courier", '€'), Some(600));
    }

    #[test]
    fn standard_14_width_refuses_what_it_cannot_measure() {
        assert_eq!(standard_14_char_width("Times-Roman", '日'), None);
        assert_eq!(standard_14_char_width("Courier", '\u{0081}'), None);
        assert_eq!(standard_14_char_width("Symbol", 'a'), None);
    }

    #[test]
    fn times_ascii_slice_equals_the_ascii_table() {
        assert_eq!(
            &TIMES_ROMAN_WINANSI_WIDTHS[0x20..=0x7E],
            &TIMES_ROMAN_ASCII_WIDTHS[..]
        );
    }

    #[test]
    fn every_defined_times_code_has_a_width() {
        let undefined = [0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D];
        for (code, &width) in TIMES_ROMAN_WINANSI_WIDTHS.iter().enumerate().skip(0x20) {
            if undefined.contains(&code) {
                assert_eq!(width, 0, "code {code:#04X} is undefined");
            } else {
                assert!(width > 0, "code {code:#04X} has no width");
            }
        }
    }
}
