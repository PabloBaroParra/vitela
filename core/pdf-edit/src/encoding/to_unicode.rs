//! Reading a font's `/ToUnicode` CMap (PDF 32000-1 §9.10.3).
//!
//! A subset font routinely gives its non-ASCII glyphs codes that only this
//! CMap explains — reportlab, for one, numbers `Ó` as `\001` and declares no
//! `/Encoding` at all — so without it those characters read back as U+FFFD
//! and the run cannot be retyped.
//!
//! Parsing is deliberately forgiving: a CMap is a PostScript fragment written
//! by many producers, and one bad entry must cost that entry, not the whole
//! map, and never a panic. Only the three sections that carry mappings are
//! read — `codespacerange`, `bfchar` and `bfrange` — everything else in the
//! program is skipped token by token.

use std::collections::BTreeMap;

/// The most codes one `bfrange` entry may expand to. A two-byte range spans
/// at most 65 536 codes; anything wider is malformed, and capping it keeps a
/// hostile `<00000000> <FFFFFFFF>` from allocating gigabytes.
const MAX_RANGE_CODES: u32 = 0x1_0000;

/// The most mappings one CMap may hold. A two-byte code space has 65 536
/// codes, so more is never meaningful.
const MAX_ENTRIES: usize = 0x1_0000;

/// The most inserts one CMap may attempt. Capping entries alone is not
/// enough: a flood of ranges that rewrite the same codes would keep the map
/// small while costing billions of inserts per page parse.
const MAX_INSERTS: u32 = 1 << 17;

/// A parsed `/ToUnicode` CMap: which byte lengths codes have, and the text
/// each mapped code stands for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToUnicodeMap {
    /// `(low, high)` byte strings of equal length, as declared.
    codespaces: Vec<(Vec<u8>, Vec<u8>)>,
    /// `(code length in bytes, code)` to the text it maps to. Empty and
    /// U+0000-only destinations are never stored: producers write
    /// `<0000>` for "no character" (reportlab fills every unused slot that
    /// way), and reading it as a real NUL would hide the code's fallback.
    entries: BTreeMap<(usize, u32), String>,
    /// Inserts attempted so far, against [`MAX_INSERTS`].
    inserts: u32,
}

impl ToUnicodeMap {
    /// Parses a decoded CMap stream. Never fails: what cannot be read is
    /// left out.
    pub fn parse(bytes: &[u8]) -> Self {
        let tokens = tokenize(bytes);
        let mut map = Self::default();
        let mut index = 0;
        while index < tokens.len() {
            index = match &tokens[index] {
                Token::Word(word) if word == "begincodespacerange" => {
                    map.read_codespaces(&tokens, index + 1)
                }
                Token::Word(word) if word == "beginbfchar" => map.read_bfchars(&tokens, index + 1),
                Token::Word(word) if word == "beginbfrange" => {
                    map.read_bfranges(&tokens, index + 1)
                }
                _ => index + 1,
            };
        }
        map
    }

    /// Whether the map has no mapping at all — a CMap this build could not
    /// read anything from is treated as if the font had none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The text a one-byte code of a simple font maps to.
    ///
    /// A simple font's codes are single bytes whatever the CMap declares, but
    /// some producers still write its entries two bytes wide (`<0041>`), so
    /// that width is accepted as the same code.
    pub fn simple(&self, code: u8) -> Option<&str> {
        self.entries
            .get(&(1, u32::from(code)))
            .or_else(|| self.entries.get(&(2, u32::from(code))))
            .map(String::as_str)
    }

    /// Splits a composite font's string into codes by the declared
    /// codespace ranges, as `(code, length)` pairs. A byte sequence no range
    /// matches is taken `default_length` bytes at a time (§9.7.6.3 lets a
    /// reader pick any length for it; two is what Identity-H uses), and a
    /// short tail becomes one final code.
    pub fn split(&self, bytes: &[u8], default_length: usize) -> Vec<(u32, usize)> {
        let mut codes = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let length = self
                .codespace_length(&bytes[at..])
                .unwrap_or(default_length.max(1))
                .min(bytes.len() - at);
            codes.push((code_value(&bytes[at..at + length]), length));
            at += length;
        }
        codes
    }

    /// The text a code of `length` bytes maps to.
    pub fn lookup(&self, code: u32, length: usize) -> Option<&str> {
        self.entries.get(&(length, code)).map(String::as_str)
    }

    fn codespace_length(&self, bytes: &[u8]) -> Option<usize> {
        self.codespaces
            .iter()
            .filter(|(low, _)| low.len() <= bytes.len())
            .find(|(low, high)| {
                low.iter()
                    .zip(high)
                    .zip(bytes)
                    .all(|((low, high), byte)| (low..=high).contains(&byte))
            })
            .map(|(low, _)| low.len())
    }

    fn read_codespaces(&mut self, tokens: &[Token], mut index: usize) -> usize {
        while let Some(token) = tokens.get(index) {
            match (token, tokens.get(index + 1)) {
                (Token::Word(word), _) if word == "endcodespacerange" => return index + 1,
                (Token::Hex(low), Some(Token::Hex(high)))
                    if !low.is_empty() && low.len() == high.len() && low.len() <= 4 =>
                {
                    self.codespaces.push((low.clone(), high.clone()));
                    index += 2;
                }
                _ => index += 1,
            }
        }
        index
    }

    fn read_bfchars(&mut self, tokens: &[Token], mut index: usize) -> usize {
        while let Some(token) = tokens.get(index) {
            match (token, tokens.get(index + 1)) {
                (Token::Word(word), _) if word == "endbfchar" => return index + 1,
                (Token::Hex(source), Some(Token::Hex(destination))) => {
                    if let Some(text) = utf16_text(destination) {
                        self.insert(source, text);
                    }
                    index += 2;
                }
                _ => index += 1,
            }
        }
        index
    }

    fn read_bfranges(&mut self, tokens: &[Token], mut index: usize) -> usize {
        while let Some(token) = tokens.get(index) {
            match (token, tokens.get(index + 1), tokens.get(index + 2)) {
                (Token::Word(word), _, _) if word == "endbfrange" => return index + 1,
                (Token::Hex(low), Some(Token::Hex(high)), Some(destination)) => {
                    match destination {
                        Token::Hex(start) => self.insert_incrementing(low, high, start),
                        Token::Array(items) => self.insert_listed(low, high, items),
                        _ => {}
                    }
                    index += 3;
                }
                _ => index += 1,
            }
        }
        index
    }

    /// `<low> <high> <dst>`: each code maps to `dst` with its last UTF-16
    /// unit advanced by the code's offset from `low`.
    fn insert_incrementing(&mut self, low: &[u8], high: &[u8], start: &[u8]) {
        let Some(count) = range_count(low, high) else {
            return;
        };
        let Some(mut units) = utf16_units(start) else {
            return;
        };
        let Some(&last) = units.last() else {
            return;
        };
        let first = code_value(low);
        for offset in 0..count {
            let Some(unit) = u16::try_from(u32::from(last) + offset).ok() else {
                break;
            };
            *units.last_mut().expect("non-empty") = unit;
            if !self.store((low.len(), first + offset), text_of_units(&units)) {
                break;
            }
        }
    }

    /// `<low> <high> [<dst> ...]`: each code maps to its own listed string.
    fn insert_listed(&mut self, low: &[u8], high: &[u8], items: &[Token]) {
        let Some(count) = range_count(low, high) else {
            return;
        };
        let first = code_value(low);
        for (offset, item) in (0..count).zip(items) {
            let text = match item {
                Token::Hex(destination) => utf16_text(destination),
                _ => None,
            };
            if !self.store((low.len(), first + offset), text) {
                break;
            }
        }
    }

    fn insert(&mut self, source: &[u8], text: String) {
        if !source.is_empty() && source.len() <= 4 {
            self.store((source.len(), code_value(source)), Some(text));
        }
    }

    /// Counts one attempted insert and stores `text` under `key` when it is
    /// a mapping. `false` once the budget is spent: the rest is dropped.
    fn store(&mut self, key: (usize, u32), text: Option<String>) -> bool {
        if self.inserts >= MAX_INSERTS || self.entries.len() >= MAX_ENTRIES {
            return false;
        }
        self.inserts += 1;
        if let Some(text) = text {
            self.entries.insert(key, text);
        }
        true
    }
}

/// How many codes `low..=high` spans, or `None` when the pair is malformed
/// (different lengths, reversed, wider than four bytes or than the cap).
fn range_count(low: &[u8], high: &[u8]) -> Option<u32> {
    if low.is_empty() || low.len() != high.len() || low.len() > 4 {
        return None;
    }
    let (low, high) = (code_value(low), code_value(high));
    let count = high.checked_sub(low)?.checked_add(1)?;
    (count <= MAX_RANGE_CODES).then_some(count)
}

fn code_value(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |value, &byte| (value << 8) | u32::from(byte))
}

fn utf16_units(bytes: &[u8]) -> Option<Vec<u16>> {
    match bytes.len() {
        0 => None,
        // A one-byte destination is not UTF-16, but it is common enough in
        // the wild (`<41>`) that reading it as the code point it spells
        // beats throwing the entry away.
        1 => Some(vec![u16::from(bytes[0])]),
        length if length % 2 == 0 => Some(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| u16::from_be_bytes(pair))
                .collect(),
        ),
        _ => None,
    }
}

fn utf16_text(bytes: &[u8]) -> Option<String> {
    text_of_units(&utf16_units(bytes)?)
}

/// Decodes UTF-16 units, refusing unpaired surrogates and the NUL-only
/// "no character" destination.
fn text_of_units(units: &[u16]) -> Option<String> {
    let text: String = char::decode_utf16(units.iter().copied())
        .collect::<Result<_, _>>()
        .ok()?;
    (!text.is_empty() && text.chars().any(|character| character != '\0')).then_some(text)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Hex(Vec<u8>),
    Array(Vec<Token>),
    Word(String),
    Literal,
}

/// Splits a CMap into the tokens the three mapping sections use. A literal
/// string's content and dictionary brackets are skipped; only arrays nest, because a
/// `bfrange` destination array is the one place a token holds others.
fn tokenize(bytes: &[u8]) -> Vec<Token> {
    let mut stack: Vec<Vec<Token>> = vec![Vec::new()];
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        match byte {
            b'%' => {
                while at < bytes.len() && bytes[at] != b'\n' && bytes[at] != b'\r' {
                    at += 1;
                }
            }
            b'<' if bytes.get(at + 1) == Some(&b'<') => at += 2,
            b'>' if bytes.get(at + 1) == Some(&b'>') => at += 2,
            b'<' => {
                let (hex, next) = hex_string(bytes, at + 1);
                push(&mut stack, Token::Hex(hex));
                at = next;
            }
            b'(' => {
                // Kept as a token of its own so a string where a hex code
                // belongs reads as malformed instead of shifting the pairs.
                push(&mut stack, Token::Literal);
                at = skip_literal(bytes, at);
            }
            b'[' => {
                stack.push(Vec::new());
                at += 1;
            }
            b']' => {
                if stack.len() > 1 {
                    let items = stack.pop().expect("nested array");
                    push(&mut stack, Token::Array(items));
                }
                at += 1;
            }
            _ if is_whitespace(byte) || is_delimiter(byte) => at += 1,
            _ => {
                let start = at;
                while at < bytes.len() && !is_whitespace(bytes[at]) && !is_delimiter(bytes[at]) {
                    at += 1;
                }
                push(
                    &mut stack,
                    Token::Word(String::from_utf8_lossy(&bytes[start..at]).into_owned()),
                );
            }
        }
    }
    // An unclosed array still contributes what it held.
    while stack.len() > 1 {
        let items = stack.pop().expect("nested array");
        push(&mut stack, Token::Array(items));
    }
    stack.pop().unwrap_or_default()
}

fn push(stack: &mut [Vec<Token>], token: Token) {
    if let Some(top) = stack.last_mut() {
        top.push(token);
    }
}

/// Reads hex digits up to `>`, ignoring whitespace and padding an odd final
/// digit with zero. Returns the bytes and the index after the `>`.
fn hex_string(bytes: &[u8], mut at: usize) -> (Vec<u8>, usize) {
    let mut digits = Vec::new();
    while at < bytes.len() && bytes[at] != b'>' {
        if let Some(digit) = (bytes[at] as char).to_digit(16) {
            digits.push(digit as u8);
        }
        at += 1;
    }
    if digits.len() % 2 == 1 {
        digits.push(0);
    }
    let decoded = digits
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[high, low]| (high << 4) | low)
        .collect();
    (decoded, (at + 1).min(bytes.len()))
}

fn skip_literal(bytes: &[u8], mut at: usize) -> usize {
    let mut depth = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 1,
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return at + 1;
                }
            }
            _ => {}
        }
        at += 1;
    }
    at
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'\0')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> ToUnicodeMap {
        ToUnicodeMap::parse(text.as_bytes())
    }

    #[test]
    fn bfchar_maps_single_codes_to_their_characters() {
        let map = parse(
            "1 begincodespacerange <00> <FF> endcodespacerange\n\
             2 beginbfchar\n<01> <00D3>\n<41> <0041>\nendbfchar",
        );

        assert_eq!(map.simple(0x01), Some("Ó"));
        assert_eq!(map.simple(0x41), Some("A"));
        assert_eq!(map.simple(0x02), None);
    }

    #[test]
    fn a_nul_destination_means_no_character_not_a_real_nul() {
        let map = parse("1 beginbfchar <04> <0000> endbfchar");

        assert_eq!(map.simple(0x04), None);
        assert!(map.is_empty());
    }

    #[test]
    fn bfrange_with_a_string_destination_increments_it() {
        let map = parse("1 beginbfrange <20> <22> <00E0> endbfrange");

        assert_eq!(map.simple(0x20), Some("à"));
        assert_eq!(map.simple(0x21), Some("á"));
        assert_eq!(map.simple(0x22), Some("â"));
        assert_eq!(map.simple(0x23), None);
    }

    #[test]
    fn bfrange_with_an_array_destination_maps_each_code_to_its_entry() {
        let map = parse("1 beginbfrange <05> <07> [<00F1> <00660069> <00E9>] endbfrange");

        assert_eq!(map.simple(0x05), Some("ñ"));
        assert_eq!(
            map.simple(0x06),
            Some("fi"),
            "a ligature maps to two characters"
        );
        assert_eq!(map.simple(0x07), Some("é"));
    }

    #[test]
    fn a_two_byte_codespace_splits_and_maps_composite_codes() {
        let map = parse(
            "1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
             1 beginbfchar <0102> <00C1> endbfchar\n\
             1 beginbfrange <0200> <0201> <0061> endbfrange",
        );

        assert_eq!(
            map.split(&[0x01, 0x02, 0x02, 0x01], 2),
            vec![(0x0102, 2), (0x0201, 2)]
        );
        assert_eq!(map.lookup(0x0102, 2), Some("Á"));
        assert_eq!(map.lookup(0x0201, 2), Some("b"));
    }

    #[test]
    fn mixed_codespaces_pick_the_length_whose_range_matches() {
        let map = parse("2 begincodespacerange <00> <80> <8140> <9FFC> endcodespacerange");

        assert_eq!(
            map.split(&[0x41, 0x81, 0x40, 0x42], 2),
            vec![(0x41, 1), (0x8140, 2), (0x42, 1)]
        );
    }

    #[test]
    fn a_surrogate_pair_destination_decodes_to_one_character() {
        let map = parse("1 beginbfchar <0001> <D83DDE00> endbfchar");

        assert_eq!(map.lookup(1, 2), Some("😀"));
    }

    #[test]
    fn comments_and_irregular_whitespace_are_ignored() {
        let map = parse(
            "%!PS-Adobe-3.0 Resource-CMap\n/CIDInit /ProcSet findresource begin\n\
             12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) >> def\n\
             1 beginbfchar%comment\r<4 1>\t\t< 00 42 > % trailing\nendbfchar endcmap",
        );

        assert_eq!(map.simple(0x41), Some("B"));
    }

    #[test]
    fn malformed_entries_are_skipped_and_the_rest_still_maps() {
        let map = parse(
            "3 beginbfchar\n<41> <D800>\n<42> (B)\n<43> <0043> endbfchar\n\
             3 beginbfrange <10> <0F> <0041> <0000> <FFFFFF> <0041> <50> <51> <0061> endbfrange\n\
             1 beginbfrange <60> <61> [<0078",
        );

        assert_eq!(map.simple(0x41), None, "an unpaired surrogate is refused");
        assert_eq!(map.simple(0x42), None, "a non-hex destination is refused");
        assert_eq!(map.simple(0x43), Some("C"));
        assert_eq!(map.simple(0x10), None, "a reversed range is refused");
        assert_eq!(map.lookup(0, 2), None, "mismatched lengths are refused");
        assert_eq!(map.simple(0x50), Some("a"));
        assert_eq!(map.simple(0x51), Some("b"));
        assert_eq!(
            map.simple(0x60),
            Some("x"),
            "an unclosed array keeps its items"
        );
    }

    #[test]
    fn garbage_never_panics() {
        for input in [
            &b""[..],
            b"<",
            b"beginbfchar",
            b"beginbfrange <00> <FF>",
            b"begincodespacerange <> <> endcodespacerange",
            b"]]]][[[ (unclosed",
            b"beginbfrange <00000000> <FFFFFFFF> <0041> endbfrange",
            &[0xFF, 0xFE, b'<', 0x80, b'>'],
        ] {
            let map = ToUnicodeMap::parse(input);
            let _ = map.split(&[1, 2, 3], 2);
        }
    }

    #[test]
    fn an_oversized_range_is_refused_rather_than_expanded() {
        let map = parse("1 beginbfrange <00000000> <FFFFFFFF> <0041> endbfrange");

        assert!(map.is_empty());
    }

    #[test]
    fn a_flood_of_full_ranges_stops_at_the_total_budget() {
        let cmap = "<0000> <FFFF> <0041>\n".repeat(100_000);
        let map = parse(&format!("beginbfrange\n{cmap}endbfrange"));

        assert!(map.entries.len() <= 0x1_0000);
        assert_eq!(map.simple(0x00), Some("A"), "the first range still maps");
    }
}
