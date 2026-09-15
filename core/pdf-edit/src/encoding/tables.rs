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
/// Both slots hold the *curly* quote, not the typewriter one: Adobe numbers
/// 0x27 `quoteright` and 0x60 `quoteleft`. The straight apostrophe and grave
/// are not missing from the encoding, they live up in [`STANDARD_HIGH`] at
/// 0xA9 and 0xC1 — so a font that decodes `'` at 0x27 is a WinAnsi font, and
/// mixing the two tables up swaps a reader's quotes without failing anything.
pub const STANDARD_ASCII_OVERRIDES: &[(u8, char)] = &[(0x27, '\u{2019}'), (0x60, '\u{2018}')];

/// StandardEncoding above 0x7E (PDF 32000-1 Annex D.2, the `STD` column).
///
/// Nothing here is Latin-1: StandardEncoding's high half is accents,
/// typographic punctuation and the handful of extended letters Adobe's
/// original text faces carried, at codes that match no other encoding. The
/// gaps between entries (0xB0, 0xB5, 0xC0, and the whole 0x80..=0xA0 block)
/// are unused in the encoding itself, not omissions here.
pub const STANDARD_HIGH: &[(u8, char)] = &[
    (0xA1, '\u{00A1}'), // exclamdown
    (0xA2, '\u{00A2}'), // cent
    (0xA3, '\u{00A3}'), // sterling
    (0xA4, '\u{2044}'), // fraction
    (0xA5, '\u{00A5}'), // yen
    (0xA6, '\u{0192}'), // florin
    (0xA7, '\u{00A7}'), // section
    (0xA8, '\u{00A4}'), // currency
    (0xA9, '\u{0027}'), // quotesingle — the straight one, not 0x27
    (0xAA, '\u{201C}'), // quotedblleft
    (0xAB, '\u{00AB}'), // guillemotleft
    (0xAC, '\u{2039}'), // guilsinglleft
    (0xAD, '\u{203A}'), // guilsinglright
    (0xAE, '\u{FB01}'), // fi
    (0xAF, '\u{FB02}'), // fl
    (0xB1, '\u{2013}'), // endash
    (0xB2, '\u{2020}'), // dagger
    (0xB3, '\u{2021}'), // daggerdbl
    (0xB4, '\u{00B7}'), // periodcentered
    (0xB6, '\u{00B6}'), // paragraph
    (0xB7, '\u{2022}'), // bullet
    (0xB8, '\u{201A}'), // quotesinglbase
    (0xB9, '\u{201E}'), // quotedblbase
    (0xBA, '\u{201D}'), // quotedblright
    (0xBB, '\u{00BB}'), // guillemotright
    (0xBC, '\u{2026}'), // ellipsis
    (0xBD, '\u{2030}'), // perthousand
    (0xBF, '\u{00BF}'), // questiondown
    (0xC1, '\u{0060}'), // grave — the straight one, not 0x60
    (0xC2, '\u{00B4}'), // acute
    (0xC3, '\u{02C6}'), // circumflex
    (0xC4, '\u{02DC}'), // tilde
    (0xC5, '\u{00AF}'), // macron
    (0xC6, '\u{02D8}'), // breve
    (0xC7, '\u{02D9}'), // dotaccent
    (0xC8, '\u{00A8}'), // dieresis
    (0xCA, '\u{02DA}'), // ring
    (0xCB, '\u{00B8}'), // cedilla
    (0xCD, '\u{02DD}'), // hungarumlaut
    (0xCE, '\u{02DB}'), // ogonek
    (0xCF, '\u{02C7}'), // caron
    (0xD0, '\u{2014}'), // emdash
    (0xE1, '\u{00C6}'), // AE
    (0xE3, '\u{00AA}'), // ordfeminine
    (0xE8, '\u{0141}'), // Lslash
    (0xE9, '\u{00D8}'), // Oslash
    (0xEA, '\u{0152}'), // OE
    (0xEB, '\u{00BA}'), // ordmasculine
    (0xF1, '\u{00E6}'), // ae
    (0xF5, '\u{0131}'), // dotlessi
    (0xF8, '\u{0142}'), // lslash
    (0xF9, '\u{00F8}'), // oslash
    (0xFA, '\u{0153}'), // oe
    (0xFB, '\u{00DF}'), // germandbls
];

/// MacRomanEncoding above 0x7E.
///
/// The Latin entries are Annex D.2's `MAC` column. Annex D stops there — it
/// tabulates a Latin character set, not an encoding in full — so the slots
/// Mac OS Roman fills with mathematical symbols (0xAD `notequal`, 0xB0
/// `infinity`, 0xB6..0xBA, 0xC3, 0xC5, 0xC6, 0xD7) are taken from Apple's own
/// `ROMAN.TXT`. That is not a guess: no second authority claims those codes,
/// and a font declaring `/MacRomanEncoding` paints them.
///
/// **Two codes are deliberately absent**, and both are cases where two
/// authorities disagree — the one situation this module always resolves by
/// refusing:
///
/// - **0xDB.** Annex D names it `currency` (U+00A4); Mac OS Roman puts the
///   Euro (U+20AC) there. Picking either writes the other document's glyph.
/// - **0xF0.** Apple's logo, at U+F8FF in the private use area. A private
///   code point is not a character any other font can be asked to paint.
pub const MAC_ROMAN_HIGH: &[(u8, char)] = &[
    (0x80, '\u{00C4}'), // Adieresis
    (0x81, '\u{00C5}'), // Aring
    (0x82, '\u{00C7}'), // Ccedilla
    (0x83, '\u{00C9}'), // Eacute
    (0x84, '\u{00D1}'), // Ntilde
    (0x85, '\u{00D6}'), // Odieresis
    (0x86, '\u{00DC}'), // Udieresis
    (0x87, '\u{00E1}'), // aacute
    (0x88, '\u{00E0}'), // agrave
    (0x89, '\u{00E2}'), // acircumflex
    (0x8A, '\u{00E4}'), // adieresis
    (0x8B, '\u{00E3}'), // atilde
    (0x8C, '\u{00E5}'), // aring
    (0x8D, '\u{00E7}'), // ccedilla
    (0x8E, '\u{00E9}'), // eacute
    (0x8F, '\u{00E8}'), // egrave
    (0x90, '\u{00EA}'), // ecircumflex
    (0x91, '\u{00EB}'), // edieresis
    (0x92, '\u{00ED}'), // iacute
    (0x93, '\u{00EC}'), // igrave
    (0x94, '\u{00EE}'), // icircumflex
    (0x95, '\u{00EF}'), // idieresis
    (0x96, '\u{00F1}'), // ntilde
    (0x97, '\u{00F3}'), // oacute
    (0x98, '\u{00F2}'), // ograve
    (0x99, '\u{00F4}'), // ocircumflex
    (0x9A, '\u{00F6}'), // odieresis
    (0x9B, '\u{00F5}'), // otilde
    (0x9C, '\u{00FA}'), // uacute
    (0x9D, '\u{00F9}'), // ugrave
    (0x9E, '\u{00FB}'), // ucircumflex
    (0x9F, '\u{00FC}'), // udieresis
    (0xA0, '\u{2020}'), // dagger
    (0xA1, '\u{00B0}'), // degree
    (0xA2, '\u{00A2}'), // cent
    (0xA3, '\u{00A3}'), // sterling
    (0xA4, '\u{00A7}'), // section
    (0xA5, '\u{2022}'), // bullet
    (0xA6, '\u{00B6}'), // paragraph
    (0xA7, '\u{00DF}'), // germandbls
    (0xA8, '\u{00AE}'), // registered
    (0xA9, '\u{00A9}'), // copyright
    (0xAA, '\u{2122}'), // trademark
    (0xAB, '\u{00B4}'), // acute
    (0xAC, '\u{00A8}'), // dieresis
    (0xAD, '\u{2260}'), // notequal
    (0xAE, '\u{00C6}'), // AE
    (0xAF, '\u{00D8}'), // Oslash
    (0xB0, '\u{221E}'), // infinity
    (0xB1, '\u{00B1}'), // plusminus
    (0xB2, '\u{2264}'), // lessequal
    (0xB3, '\u{2265}'), // greaterequal
    (0xB4, '\u{00A5}'), // yen
    (0xB5, '\u{00B5}'), // mu
    (0xB6, '\u{2202}'), // partialdiff
    (0xB7, '\u{2211}'), // summation
    (0xB8, '\u{220F}'), // product
    (0xB9, '\u{03C0}'), // pi
    (0xBA, '\u{222B}'), // integral
    (0xBB, '\u{00AA}'), // ordfeminine
    (0xBC, '\u{00BA}'), // ordmasculine
    (0xBD, '\u{03A9}'), // Omega
    (0xBE, '\u{00E6}'), // ae
    (0xBF, '\u{00F8}'), // oslash
    (0xC0, '\u{00BF}'), // questiondown
    (0xC1, '\u{00A1}'), // exclamdown
    (0xC2, '\u{00AC}'), // logicalnot
    (0xC3, '\u{221A}'), // radical
    (0xC4, '\u{0192}'), // florin
    (0xC5, '\u{2248}'), // approxequal
    (0xC6, '\u{2206}'), // Delta
    (0xC7, '\u{00AB}'), // guillemotleft
    (0xC8, '\u{00BB}'), // guillemotright
    (0xC9, '\u{2026}'), // ellipsis
    (0xCA, '\u{00A0}'), // space (no-break)
    (0xCB, '\u{00C0}'), // Agrave
    (0xCC, '\u{00C3}'), // Atilde
    (0xCD, '\u{00D5}'), // Otilde
    (0xCE, '\u{0152}'), // OE
    (0xCF, '\u{0153}'), // oe
    (0xD0, '\u{2013}'), // endash
    (0xD1, '\u{2014}'), // emdash
    (0xD2, '\u{201C}'), // quotedblleft
    (0xD3, '\u{201D}'), // quotedblright
    (0xD4, '\u{2018}'), // quoteleft
    (0xD5, '\u{2019}'), // quoteright
    (0xD6, '\u{00F7}'), // divide
    (0xD7, '\u{25CA}'), // lozenge
    (0xD8, '\u{00FF}'), // ydieresis
    (0xD9, '\u{0178}'), // Ydieresis
    (0xDA, '\u{2044}'), // fraction
    // 0xDB: contested — see this table's own doc.
    (0xDC, '\u{2039}'), // guilsinglleft
    (0xDD, '\u{203A}'), // guilsinglright
    (0xDE, '\u{FB01}'), // fi
    (0xDF, '\u{FB02}'), // fl
    (0xE0, '\u{2021}'), // daggerdbl
    (0xE1, '\u{00B7}'), // periodcentered
    (0xE2, '\u{201A}'), // quotesinglbase
    (0xE3, '\u{201E}'), // quotedblbase
    (0xE4, '\u{2030}'), // perthousand
    (0xE5, '\u{00C2}'), // Acircumflex
    (0xE6, '\u{00CA}'), // Ecircumflex
    (0xE7, '\u{00C1}'), // Aacute
    (0xE8, '\u{00CB}'), // Edieresis
    (0xE9, '\u{00C8}'), // Egrave
    (0xEA, '\u{00CD}'), // Iacute
    (0xEB, '\u{00CE}'), // Icircumflex
    (0xEC, '\u{00CF}'), // Idieresis
    (0xED, '\u{00CC}'), // Igrave
    (0xEE, '\u{00D3}'), // Oacute
    (0xEF, '\u{00D4}'), // Ocircumflex
    // 0xF0: contested — see this table's own doc.
    (0xF1, '\u{00D2}'), // Ograve
    (0xF2, '\u{00DA}'), // Uacute
    (0xF3, '\u{00DB}'), // Ucircumflex
    (0xF4, '\u{00D9}'), // Ugrave
    (0xF5, '\u{0131}'), // dotlessi
    (0xF6, '\u{02C6}'), // circumflex
    (0xF7, '\u{02DC}'), // tilde
    (0xF8, '\u{00AF}'), // macron
    (0xF9, '\u{02D8}'), // breve
    (0xFA, '\u{02D9}'), // dotaccent
    (0xFB, '\u{02DA}'), // ring
    (0xFC, '\u{00B8}'), // cedilla
    (0xFD, '\u{02DD}'), // hungarumlaut
    (0xFE, '\u{02DB}'), // ogonek
    (0xFF, '\u{02C7}'), // caron
];

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
    // Latin letters outside Latin-1, and the superscript digits. Every one of
    // these is reachable by *code* through some base encoding, so leaving it
    // unnameable would mean a font that restates its own encoding through
    // `/Differences` came out worse than one that did not say anything.
    ("Ydieresis", '\u{0178}'),
    ("Lslash", '\u{0141}'),
    ("lslash", '\u{0142}'),
    ("OE", '\u{0152}'),
    ("oe", '\u{0153}'),
    ("Scaron", '\u{0160}'),
    ("scaron", '\u{0161}'),
    ("Zcaron", '\u{017D}'),
    ("zcaron", '\u{017E}'),
    ("dotlessi", '\u{0131}'),
    ("florin", '\u{0192}'),
    ("onesuperior", '\u{00B9}'),
    ("twosuperior", '\u{00B2}'),
    ("threesuperior", '\u{00B3}'),
    ("fraction", '\u{2044}'),
    // The spacing accents. `grave`, `acute`, `macron`, `dieresis` and
    // `cedilla` are already above, in the Latin-1 block where their codes
    // are; these are the ones whose code points sit in the modifier-letter
    // range instead.
    ("circumflex", '\u{02C6}'),
    ("caron", '\u{02C7}'),
    ("breve", '\u{02D8}'),
    ("dotaccent", '\u{02D9}'),
    ("ring", '\u{02DA}'),
    ("ogonek", '\u{02DB}'),
    ("tilde", '\u{02DC}'),
    ("hungarumlaut", '\u{02DD}'),
    // Mac OS Roman's non-Latin slots (see [`MAC_ROMAN_HIGH`]). Adobe's glyph
    // list names them, so `/Differences` can too.
    ("Omega", '\u{03A9}'),
    ("pi", '\u{03C0}'),
    ("partialdiff", '\u{2202}'),
    ("Delta", '\u{2206}'),
    ("product", '\u{220F}'),
    ("summation", '\u{2211}'),
    ("radical", '\u{221A}'),
    ("infinity", '\u{221E}'),
    ("integral", '\u{222B}'),
    ("approxequal", '\u{2248}'),
    ("notequal", '\u{2260}'),
    ("lessequal", '\u{2264}'),
    ("greaterequal", '\u{2265}'),
    ("lozenge", '\u{25CA}'),
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
    //
    // A name that merely *looks* like one of those forms falls through to the
    // table rather than resolving to nothing: `uacute` strips to `acute`,
    // five characters, which is the right length for the `uXXXX` form and not
    // remotely hexadecimal. Returning early there cost the encoding its own
    // `uacute` and `ugrave` entries.
    let code_point = name
        .strip_prefix("uni")
        .filter(|hex| hex.len() == 4)
        .or_else(|| {
            name.strip_prefix('u')
                .filter(|hex| (4..=6).contains(&hex.len()))
        })
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .and_then(char::from_u32);
    if code_point.is_some() {
        return code_point;
    }

    GLYPH_NAMES
        .iter()
        .find(|(glyph, _)| *glyph == name)
        .map(|(_, character)| *character)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every table here is reviewed by eye against Annex D, so each must stay
    /// in code order with no slot claimed twice — a duplicate would make the
    /// later entry win silently, which is the kind of edit that survives
    /// review and not production.
    fn assert_sorted_and_unique(table: &[(u8, char)], name: &str) {
        for pair in table.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "{name} is out of order or repeats a code at {:#04X}",
                pair[1].0
            );
        }
    }

    #[test]
    fn the_encoding_tables_are_ordered_and_claim_each_code_once() {
        assert_sorted_and_unique(WIN_ANSI_HIGH, "WIN_ANSI_HIGH");
        assert_sorted_and_unique(STANDARD_ASCII_OVERRIDES, "STANDARD_ASCII_OVERRIDES");
        assert_sorted_and_unique(STANDARD_HIGH, "STANDARD_HIGH");
        assert_sorted_and_unique(MAC_ROMAN_HIGH, "MAC_ROMAN_HIGH");
    }

    #[test]
    fn no_glyph_name_is_listed_twice() {
        let mut names: Vec<&str> = GLYPH_NAMES.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        let mut unique = names.clone();
        unique.dedup();

        assert_eq!(names, unique, "GLYPH_NAMES repeats a name");
    }

    /// A `/Differences` array names glyphs; a base encoding numbers them.
    /// Both feed the same 256-slot table, so anything a base encoding can
    /// paint must also be nameable — otherwise a font that merely *restates*
    /// its own encoding through `/Differences` would lose codes it already
    /// had.
    #[test]
    fn every_character_a_base_encoding_paints_is_reachable_by_glyph_name() {
        // U+00A0 is the exception, and a naming collision rather than a gap:
        // Annex D calls MacRoman 0xCA `space`, the same name it gives U+0020,
        // so the no-break space is reachable only as `/uni00A0`.
        let named_elsewhere = '\u{00A0}';

        for (table, label) in [
            (WIN_ANSI_HIGH, "WIN_ANSI_HIGH"),
            (STANDARD_ASCII_OVERRIDES, "STANDARD_ASCII_OVERRIDES"),
            (STANDARD_HIGH, "STANDARD_HIGH"),
            (MAC_ROMAN_HIGH, "MAC_ROMAN_HIGH"),
        ] {
            for &(code, character) in table {
                if character == named_elsewhere {
                    continue;
                }
                assert_eq!(
                    char_for_glyph_name_of(character),
                    Some(character),
                    "{label} paints {character:?} at {code:#04X} with no glyph name for it"
                );
            }
        }
    }

    /// Round-trips `character` through whatever name `GLYPH_NAMES` gives it.
    fn char_for_glyph_name_of(character: char) -> Option<char> {
        let name = GLYPH_NAMES
            .iter()
            .find(|(_, mapped)| *mapped == character)
            .map(|(name, _)| *name)?;
        char_for_glyph_name(name)
    }
}
