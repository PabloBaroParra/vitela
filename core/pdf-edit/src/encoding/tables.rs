//! Static encoding tables (T-153).
//!
//! Every table here is **conservative on purpose**. A code this module
//! cannot map is reported as unknown, which makes replacement text
//! containing that character an `EncodingGap` — the edit is refused. The
//! opposite failure, guessing a mapping and writing the wrong glyph, is the
//! one thing batch decision 3 forbids, so a missing entry costs a rejected
//! edit while a wrong entry costs a corrupted document.

/// The 14 standard font base names (PDF 32000-1 Annex D.1), which need no
/// embedded font program.
pub const STANDARD_14: &[&str] = &[
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];

/// The Windows-1252 block that WinAnsiEncoding places at 0x80..=0x9F. Codes
/// absent from this table are undefined in that encoding.
///
/// Outside this block WinAnsiEncoding is exact Unicode identity: 0x20..=0x7E
/// is ASCII and 0xA0..=0xFF is Latin-1, so neither needs a table.
pub const WIN_ANSI_HIGH: &[(u8, char)] = &[
    (0x80, '\u{20AC}'), // Euro
    (0x82, '\u{201A}'),
    (0x83, '\u{0192}'),
    (0x84, '\u{201E}'),
    (0x85, '\u{2026}'), // ellipsis
    (0x86, '\u{2020}'),
    (0x87, '\u{2021}'),
    (0x88, '\u{02C6}'),
    (0x89, '\u{2030}'),
    (0x8A, '\u{0160}'),
    (0x8B, '\u{2039}'),
    (0x8C, '\u{0152}'),
    (0x8E, '\u{017D}'),
    (0x91, '\u{2018}'), // curly quotes
    (0x92, '\u{2019}'),
    (0x93, '\u{201C}'),
    (0x94, '\u{201D}'),
    (0x95, '\u{2022}'), // bullet
    (0x96, '\u{2013}'), // en dash
    (0x97, '\u{2014}'), // em dash
    (0x98, '\u{02DC}'),
    (0x99, '\u{2122}'), // trademark
    (0x9A, '\u{0161}'),
    (0x9B, '\u{203A}'),
    (0x9C, '\u{0153}'),
    (0x9E, '\u{017E}'),
    (0x9F, '\u{0178}'),
];

/// Where StandardEncoding disagrees with ASCII in the printable range.
///
/// Its upper half is **not** mapped in v1: those slots hold typographic and
/// accent glyphs whose codes differ from every other encoding, and an
/// approximate table there would risk exactly the wrong-glyph write this
/// module refuses to make. Codes above 0x7E in a StandardEncoding font
/// therefore read back as unknown and cannot be written.
pub const STANDARD_OVERRIDES: &[(u8, char)] = &[(0x27, '\u{2019}'), (0x60, '\u{2018}')];

/// Glyph names resolvable in an `/Encoding /Differences` array.
///
/// Single ASCII letters (`/A`, `/z`) are handled programmatically rather
/// than listed, as are the `uniXXXX` and `uXXXX` forms. What remains is the
/// Latin punctuation and accented set, which is what documents people
/// actually edit in place use.
pub const GLYPH_NAMES: &[(&str, char)] = &[
    ("space", ' '),
    ("exclam", '!'),
    ("quotedbl", '"'),
    ("numbersign", '#'),
    ("dollar", '$'),
    ("percent", '%'),
    ("ampersand", '&'),
    ("quotesingle", '\''),
    ("quoteright", '\u{2019}'),
    ("parenleft", '('),
    ("parenright", ')'),
    ("asterisk", '*'),
    ("plus", '+'),
    ("comma", ','),
    ("hyphen", '-'),
    ("period", '.'),
    ("slash", '/'),
    ("zero", '0'),
    ("one", '1'),
    ("two", '2'),
    ("three", '3'),
    ("four", '4'),
    ("five", '5'),
    ("six", '6'),
    ("seven", '7'),
    ("eight", '8'),
    ("nine", '9'),
    ("colon", ':'),
    ("semicolon", ';'),
    ("less", '<'),
    ("equal", '='),
    ("greater", '>'),
    ("question", '?'),
    ("at", '@'),
    ("bracketleft", '['),
    ("backslash", '\\'),
    ("bracketright", ']'),
    ("asciicircum", '^'),
    ("underscore", '_'),
    ("grave", '`'),
    ("quoteleft", '\u{2018}'),
    ("braceleft", '{'),
    ("bar", '|'),
    ("braceright", '}'),
    ("asciitilde", '~'),
    ("bullet", '\u{2022}'),
    ("endash", '\u{2013}'),
    ("emdash", '\u{2014}'),
    ("quotedblleft", '\u{201C}'),
    ("quotedblright", '\u{201D}'),
    ("quotesinglbase", '\u{201A}'),
    ("quotedblbase", '\u{201E}'),
    ("dagger", '\u{2020}'),
    ("daggerdbl", '\u{2021}'),
    ("ellipsis", '\u{2026}'),
    ("perthousand", '\u{2030}'),
    ("guilsinglleft", '\u{2039}'),
    ("guilsinglright", '\u{203A}'),
    ("guillemotleft", '\u{00AB}'),
    ("guillemotright", '\u{00BB}'),
    ("trademark", '\u{2122}'),
    ("Euro", '\u{20AC}'),
    ("fi", '\u{FB01}'),
    ("fl", '\u{FB02}'),
    ("exclamdown", '\u{00A1}'),
    ("cent", '\u{00A2}'),
    ("sterling", '\u{00A3}'),
    ("currency", '\u{00A4}'),
    ("yen", '\u{00A5}'),
    ("brokenbar", '\u{00A6}'),
    ("section", '\u{00A7}'),
    ("dieresis", '\u{00A8}'),
    ("copyright", '\u{00A9}'),
    ("ordfeminine", '\u{00AA}'),
    ("logicalnot", '\u{00AC}'),
    ("registered", '\u{00AE}'),
    ("macron", '\u{00AF}'),
    ("degree", '\u{00B0}'),
    ("plusminus", '\u{00B1}'),
    ("acute", '\u{00B4}'),
    ("mu", '\u{00B5}'),
    ("paragraph", '\u{00B6}'),
    ("periodcentered", '\u{00B7}'),
    ("cedilla", '\u{00B8}'),
    ("ordmasculine", '\u{00BA}'),
    ("onequarter", '\u{00BC}'),
    ("onehalf", '\u{00BD}'),
    ("threequarters", '\u{00BE}'),
    ("questiondown", '\u{00BF}'),
    ("Agrave", '\u{00C0}'),
    ("Aacute", '\u{00C1}'),
    ("Acircumflex", '\u{00C2}'),
    ("Atilde", '\u{00C3}'),
    ("Adieresis", '\u{00C4}'),
    ("Aring", '\u{00C5}'),
    ("AE", '\u{00C6}'),
    ("Ccedilla", '\u{00C7}'),
    ("Egrave", '\u{00C8}'),
    ("Eacute", '\u{00C9}'),
    ("Ecircumflex", '\u{00CA}'),
    ("Edieresis", '\u{00CB}'),
    ("Igrave", '\u{00CC}'),
    ("Iacute", '\u{00CD}'),
    ("Icircumflex", '\u{00CE}'),
    ("Idieresis", '\u{00CF}'),
    ("Eth", '\u{00D0}'),
    ("Ntilde", '\u{00D1}'),
    ("Ograve", '\u{00D2}'),
    ("Oacute", '\u{00D3}'),
    ("Ocircumflex", '\u{00D4}'),
    ("Otilde", '\u{00D5}'),
    ("Odieresis", '\u{00D6}'),
    ("multiply", '\u{00D7}'),
    ("Oslash", '\u{00D8}'),
    ("Ugrave", '\u{00D9}'),
    ("Uacute", '\u{00DA}'),
    ("Ucircumflex", '\u{00DB}'),
    ("Udieresis", '\u{00DC}'),
    ("Yacute", '\u{00DD}'),
    ("Thorn", '\u{00DE}'),
    ("germandbls", '\u{00DF}'),
    ("agrave", '\u{00E0}'),
    ("aacute", '\u{00E1}'),
    ("acircumflex", '\u{00E2}'),
    ("atilde", '\u{00E3}'),
    ("adieresis", '\u{00E4}'),
    ("aring", '\u{00E5}'),
    ("ae", '\u{00E6}'),
    ("ccedilla", '\u{00E7}'),
    ("egrave", '\u{00E8}'),
    ("eacute", '\u{00E9}'),
    ("ecircumflex", '\u{00EA}'),
    ("edieresis", '\u{00EB}'),
    ("igrave", '\u{00EC}'),
    ("iacute", '\u{00ED}'),
    ("icircumflex", '\u{00EE}'),
    ("idieresis", '\u{00EF}'),
    ("eth", '\u{00F0}'),
    ("ntilde", '\u{00F1}'),
    ("ograve", '\u{00F2}'),
    ("oacute", '\u{00F3}'),
    ("ocircumflex", '\u{00F4}'),
    ("otilde", '\u{00F5}'),
    ("odieresis", '\u{00F6}'),
    ("divide", '\u{00F7}'),
    ("oslash", '\u{00F8}'),
    ("ugrave", '\u{00F9}'),
    ("uacute", '\u{00FA}'),
    ("ucircumflex", '\u{00FB}'),
    ("udieresis", '\u{00FC}'),
    ("yacute", '\u{00FD}'),
    ("thorn", '\u{00FE}'),
    ("ydieresis", '\u{00FF}'),
];

/// Per-character advance widths (AFM units, thousandths of an em) for the
/// printable ASCII range (0x20 `space` .. 0x7E `asciitilde`) of a Standard-14
/// non-symbolic face, indexed by `character as usize - 0x20`. Sourced from
/// Adobe's published Core 14 AFM metrics (`Helvetica.afm` et al.,
/// `tecnickcom/tc-font-core14-afms`) — exactly the numbers a Standard-14 font
/// is allowed to omit its own `/Widths` array for, because every conforming
/// reader is assumed to already know them (PDF 32000-1 §9.6.2.2).
///
/// Two positions need a deliberate substitution rather than a straight copy
/// of the AFM's own `C 39`/`C 96` entries: Adobe's own numbering for those
/// slots is `quoteright`/`quoteleft` (the curly punctuation StandardEncoding
/// places there), not the plain ASCII apostrophe/grave that WinAnsiEncoding —
/// the common case — actually decodes at 0x27/0x60. The values baked in here
/// are `quotesingle`'s and `grave`'s AFM widths instead, so a lookup keyed by
/// the *decoded character* (which is what the caller has) is correct for the
/// common WinAnsi case. A font that genuinely decodes to a curly quote there
/// (StandardEncoding) still falls through safely: `\u{2019}`/`\u{2018}` are
/// outside 0x20..=0x7E, so the lookup simply misses and the caller's existing
/// `missing_width` fallback applies — never a wrong width, only an
/// unimproved one.
///
/// Deliberately ASCII-only: accented Latin-1 and typographic punctuation
/// (the AFM's `C 161`..`C 251` block) are not covered. Adding them means
/// resolving each AFM glyph name to a character via [`char_for_glyph_name`]
/// and is future work — this table exists to fix the bug that motivated it
/// (T-161: an inline editor sized from the old flat-500-per-character
/// fallback didn't cover its own run's plain-ASCII text), not to be a
/// complete AFM port.
pub type AsciiWidths = [u16; 95];

pub const HELVETICA_ASCII_WIDTHS: AsciiWidths = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

pub const HELVETICA_BOLD_ASCII_WIDTHS: AsciiWidths = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

pub const TIMES_ROMAN_ASCII_WIDTHS: AsciiWidths = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444, 921, 722, 667, 667, 722, 611,
    556, 722, 722, 333, 389, 722, 611, 889, 722, 722, 556, 722, 667, 556, 611, 722, 722, 944, 722,
    722, 611, 333, 278, 333, 469, 500, 333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500,
    278, 778, 500, 500, 500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541,
];

pub const TIMES_BOLD_ASCII_WIDTHS: AsciiWidths = [
    250, 333, 555, 500, 500, 1000, 833, 278, 333, 333, 500, 570, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500, 930, 722, 667, 722, 722, 667,
    611, 778, 778, 389, 500, 778, 667, 944, 722, 778, 611, 778, 722, 556, 667, 722, 722, 1000, 722,
    722, 667, 333, 278, 333, 581, 500, 333, 500, 556, 444, 556, 444, 333, 500, 556, 278, 333, 556,
    278, 833, 556, 500, 556, 556, 444, 389, 333, 556, 500, 722, 500, 500, 444, 394, 220, 394, 520,
];

pub const TIMES_ITALIC_ASCII_WIDTHS: AsciiWidths = [
    250, 333, 420, 500, 500, 833, 778, 214, 333, 333, 500, 675, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 333, 333, 675, 675, 675, 500, 920, 611, 611, 667, 722, 611,
    611, 722, 722, 333, 444, 667, 556, 833, 667, 722, 611, 722, 611, 500, 556, 722, 611, 833, 611,
    556, 556, 389, 278, 389, 422, 500, 333, 500, 500, 444, 500, 444, 278, 500, 500, 278, 278, 444,
    278, 722, 500, 500, 500, 500, 389, 389, 278, 500, 444, 667, 444, 444, 389, 400, 275, 400, 541,
];

pub const TIMES_BOLDITALIC_ASCII_WIDTHS: AsciiWidths = [
    250, 389, 555, 500, 500, 833, 778, 278, 333, 333, 500, 570, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500, 832, 667, 667, 667, 722, 667,
    667, 722, 778, 389, 500, 667, 611, 889, 722, 722, 611, 722, 667, 556, 611, 722, 667, 889, 667,
    611, 611, 333, 278, 333, 570, 500, 333, 500, 500, 444, 500, 444, 333, 500, 556, 278, 278, 500,
    278, 778, 556, 500, 500, 500, 389, 389, 278, 556, 444, 667, 500, 444, 389, 348, 220, 348, 570,
];

/// Every glyph in [`Courier`, `Courier-Bold`, `Courier-Oblique`,
/// `Courier-BoldOblique`] is exactly this wide — the family is fixed-pitch by
/// design, per its own AFM (`IsFixedPitch true`).
pub const COURIER_ASCII_WIDTH: u16 = 600;

/// The ASCII width table for one Standard-14 `/BaseFont` name.
///
/// `None` for `Symbol`/`ZapfDingbats` (not Latin text — a different glyph set
/// entirely, out of scope here) and for any name that is not one of the 14.
pub fn standard_14_ascii_widths(base_font: &str) -> Option<AsciiWidths> {
    match base_font {
        "Helvetica" | "Helvetica-Oblique" => Some(HELVETICA_ASCII_WIDTHS),
        "Helvetica-Bold" | "Helvetica-BoldOblique" => Some(HELVETICA_BOLD_ASCII_WIDTHS),
        "Times-Roman" => Some(TIMES_ROMAN_ASCII_WIDTHS),
        "Times-Bold" => Some(TIMES_BOLD_ASCII_WIDTHS),
        "Times-Italic" => Some(TIMES_ITALIC_ASCII_WIDTHS),
        "Times-BoldItalic" => Some(TIMES_BOLDITALIC_ASCII_WIDTHS),
        "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => {
            Some([COURIER_ASCII_WIDTH; 95])
        }
        _ => None,
    }
}

/// Resolves a glyph name to the character it paints, if this build knows it.
pub fn char_for_glyph_name(name: &str) -> Option<char> {
    // `/A`..`/z`: the name of a Latin letter glyph is the letter itself.
    let mut chars = name.chars();
    if let (Some(single), None) = (chars.next(), chars.next()) {
        if single.is_ascii_alphabetic() {
            return Some(single);
        }
    }

    // `/uni0041` and `/u0041` name a code point directly.
    if let Some(hex) = name.strip_prefix("uni").filter(|hex| hex.len() == 4) {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    if let Some(hex) = name
        .strip_prefix('u')
        .filter(|hex| (4..=6).contains(&hex.len()))
    {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }

    GLYPH_NAMES
        .iter()
        .find(|(glyph, _)| *glyph == name)
        .map(|(_, character)| *character)
}
