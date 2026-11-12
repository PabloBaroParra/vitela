//! Resolving a font resource and encoding text against it (T-153).
//!
//! Two directions, and they are not symmetric. **Decoding** (codes to text)
//! is best-effort: a code this build cannot map reads back as U+FFFD so the
//! shell can still show the run. **Encoding** (text to codes) is strict: a
//! character with no code in this font is an [`EditError::EncodingGap`] and
//! the edit is refused before a single byte of the stream is touched
//! (batch decision 3).
//!
//! Editability is therefore a property of *this replacement text in this
//! font*, never a static flag on the run — the same run accepts `café` and
//! rejects `日本語`.
//!
//! A font's `/ToUnicode` CMap, when it has one, outranks its `/Encoding` in
//! both directions: it is what the producer wrote to say which character
//! each code paints, and a subset font routinely gives accented letters codes
//! nothing else explains (see [`to_unicode`]).

pub mod tables;
pub mod to_unicode;
pub mod winansi;
pub mod wrap;

use crate::error::EditError;
use lopdf::{Dictionary, Document, Object};
use pdf_document::FontKind;
use to_unicode::ToUnicodeMap;

/// The advance width assumed for a code the font gives no width for.
///
/// Half an em is the conventional fallback. It only affects the reported
/// bounding box of a run — never what gets written — so an imprecise box on
/// an unusual font costs hit-testing accuracy in the shell, not integrity.
const FALLBACK_WIDTH: f64 = 500.0;

/// The most decoded bytes a `/ToUnicode` stream may take. Real CMaps are a
/// few kilobytes even for large CID fonts; the cap only bounds a hostile one.
const MAX_TO_UNICODE_BYTES: usize = 4 * 1024 * 1024;

/// Everything this crate needs to know about one font resource: how its
/// codes map to characters, and how wide each code is.
#[derive(Debug, Clone, PartialEq)]
pub struct FontInfo {
    pub resource_name: String,
    pub kind: FontKind,
    /// Code to character by `/Encoding` and `/Differences`. `None` marks a
    /// code this build cannot map that way.
    to_unicode: Vec<Option<char>>,
    /// The font's `/ToUnicode` CMap, when it has a readable one. It wins
    /// over `to_unicode` for every code it maps.
    cmap: Option<ToUnicodeMap>,
    /// Code to the one character writing it paints: what [`Self::encode`]
    /// searches. `None` marks a code an edit must never write — the entry
    /// that makes an edit fail loudly instead of silently wrong.
    encodable: Vec<Option<char>>,
    /// Advance widths in glyph space (thousandths of an em), by code.
    widths: Vec<Option<f64>>,
    missing_width: f64,
}

impl FontInfo {
    /// Reads `codes` as text, substituting U+FFFD for codes this build
    /// cannot map. Never fails: a run that cannot be fully decoded must
    /// still be visible to the user, it just cannot be edited.
    pub fn decode(&self, codes: &[u8]) -> String {
        if self.kind == FontKind::EmbeddedComposite {
            return self.decode_composite(codes);
        }
        let mut text = String::new();
        for &code in codes {
            match self.cmap.as_ref().and_then(|cmap| cmap.simple(code)) {
                Some(mapped) => text.push_str(mapped),
                None => text.push(self.to_unicode[code as usize].unwrap_or('\u{FFFD}')),
            }
        }
        text
    }

    /// A composite font's codes are multi-byte; only its `/ToUnicode` says
    /// how wide each is and what it means. Without one every byte stays a
    /// placeholder, as before. Either way the run still refuses every
    /// in-place edit — this only makes its text readable.
    fn decode_composite(&self, codes: &[u8]) -> String {
        let Some(cmap) = &self.cmap else {
            return codes.iter().map(|_| '\u{FFFD}').collect();
        };
        cmap.split(codes, 2)
            .into_iter()
            .map(|(code, length)| cmap.lookup(code, length).unwrap_or("\u{FFFD}"))
            .collect()
    }

    /// Encodes `text` to codes in this font, or reports the first character
    /// that has none.
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, EditError> {
        if self.kind == FontKind::EmbeddedComposite {
            return Err(EditError::CompositeFontNotEditable {
                resource_font_name: self.resource_name.clone(),
            });
        }

        text.chars()
            .map(|character| {
                self.code_for(character)
                    .ok_or_else(|| EditError::EncodingGap {
                        character,
                        resource_font_name: self.resource_name.clone(),
                    })
            })
            .collect()
    }

    /// The total advance of `codes`, in text-space units where 1.0 is one
    /// em — multiply by the font size to get points.
    pub fn width_of(&self, codes: &[u8]) -> f64 {
        codes
            .iter()
            .map(|&code| self.widths[code as usize].unwrap_or(self.missing_width))
            .sum::<f64>()
            / 1000.0
    }

    /// The lowest code painting `character`. Lowest, not any: `/Differences`
    /// can map two codes to one glyph, and picking deterministically keeps
    /// save output reproducible. A space prefers code 32 when that paints
    /// one, since `Tw` word spacing only applies to that byte.
    fn code_for(&self, character: char) -> Option<u8> {
        if character == ' ' && self.encodable.get(32) == Some(&Some(' ')) {
            return Some(32);
        }
        self.encodable
            .iter()
            .position(|mapped| *mapped == Some(character))
            .map(|code| code as u8)
    }
}

/// Builds a [`FontInfo`] from a page's font resource.
pub fn resolve_font(
    document: &Document,
    font_dict: &Dictionary,
    resource_name: &str,
) -> Result<FontInfo, EditError> {
    let kind = font_kind(document, font_dict);
    let to_unicode = if kind == FontKind::EmbeddedComposite {
        // A CID font's codes are multi-byte and mapped through a CMap. This
        // version does not read them: the run is reported with placeholder
        // text and refuses every edit, rather than being decoded wrongly.
        vec![None; 256]
    } else {
        simple_encoding(document, font_dict)
    };

    let (widths, missing_width) = simple_widths(document, font_dict);
    let base_font = resolved_name(document, font_dict, b"BaseFont").unwrap_or_default();
    let widths = if kind == FontKind::Standard14 {
        apply_standard_14_widths(widths, &base_font, &to_unicode)
    } else {
        widths
    };

    let cmap = to_unicode_map(document, font_dict);
    let subset = base_font.contains('+');
    let encodable = match &cmap {
        // A full font that declares its `/Encoding` paints by that encoding:
        // a disagreeing CMap still decodes, but writing its code would paint
        // the encoding's glyph instead of the typed one.
        Some(_) if !subset && font_dict.has(b"Encoding") => to_unicode.clone(),
        Some(cmap) if kind != FontKind::EmbeddedComposite => encodable_codes(
            &to_unicode,
            cmap,
            &widths,
            last_char(document, font_dict),
            subset,
        ),
        _ => to_unicode.clone(),
    };

    Ok(FontInfo {
        resource_name: resource_name.to_string(),
        kind,
        to_unicode,
        cmap,
        encodable,
        widths,
        missing_width,
    })
}

/// Reads and parses the font's `/ToUnicode` stream, or `None` when it has
/// none this build can read. Best-effort like all decoding: a CMap that is
/// missing, undecodable or empty leaves the font exactly as `/Encoding`
/// describes it.
fn to_unicode_map(document: &Document, font_dict: &Dictionary) -> Option<ToUnicodeMap> {
    let Ok(Object::Reference(object_id)) = font_dict.get(b"ToUnicode") else {
        // A stream is always indirect, so anything else (commonly the name
        // `/Identity-H`, which some producers write here) is not a CMap.
        return None;
    };
    let stream = document.get_object(*object_id).ok()?.as_stream().ok()?;
    let decoded =
        crate::parse::decode_stream(document, stream, *object_id, MAX_TO_UNICODE_BYTES).ok()?;
    let map = ToUnicodeMap::parse(&decoded.bytes);
    (!map.is_empty()).then_some(map)
}

/// The codes a simple font with a `/ToUnicode` CMap may write, and the one
/// character each paints.
///
/// - A code the CMap maps to exactly one character paints that character,
///   whatever `/Encoding` claims: the two disagreeing means the encoding is
///   the one that is wrong. A ligature code (`fi`) is never written, since
///   one typed character cannot select it.
/// - A code the CMap leaves unmapped keeps its `/Encoding` character, except
///   in a **subset** font: there the CMap is the only record of which glyphs
///   the subset kept, and an encoding-only code may name a glyph that is not
///   in the file.
/// - A code only the CMap vouches for (a subset font's, or one where the CMap
///   overrides the encoding) also needs a `/Widths` entry inside
///   `FirstChar..=LastChar`: without one the producer declared no glyph
///   there, and its advance would be a guess. Where the two agree on a full
///   font, the encoding's own rules stand — a standard-14 font may omit
///   `/Widths` altogether.
fn encodable_codes(
    encoding: &[Option<char>],
    cmap: &ToUnicodeMap,
    widths: &[Option<f64>],
    last_char: Option<usize>,
    subset: bool,
) -> Vec<Option<char>> {
    (0..encoding.len())
        .map(|code| {
            let has_width = widths[code].is_some() && last_char.is_none_or(|last| code <= last);
            match cmap.simple(code as u8) {
                Some(mapped) => {
                    let mut characters = mapped.chars();
                    let character = match (characters.next(), characters.next()) {
                        (Some(character), None) => character,
                        _ => return None,
                    };
                    let agrees = encoding[code] == Some(character);
                    (has_width || (agrees && !subset)).then_some(character)
                }
                None if subset => None,
                None => encoding[code],
            }
        })
        .collect()
}

fn last_char(document: &Document, font_dict: &Dictionary) -> Option<usize> {
    font_dict
        .get(b"LastChar")
        .ok()
        .and_then(|value| resolve(document, value).as_i64().ok())
        .map(|value| value.max(0) as usize)
}

/// Fills the gaps `simple_widths` left (there being no `/Widths` array is the
/// common case for a Standard-14 font, which is allowed to omit one) from the
/// real AFM metrics in [`tables::standard_14_ascii_widths`], for whichever
/// codes decode to plain ASCII.
///
/// Never overrides a width the font's own `/Widths` array actually supplied
/// — that is the document's own authoritative data and wins over a table
/// this crate ships. Only ASCII gets filled; see the table's own doc for why.
fn apply_standard_14_widths(
    mut widths: Vec<Option<f64>>,
    base_font: &str,
    to_unicode: &[Option<char>],
) -> Vec<Option<f64>> {
    let Some(table) = tables::standard_14_ascii_widths(base_font) else {
        return widths;
    };
    for code in 0..widths.len() {
        if widths[code].is_some() {
            continue;
        }
        let Some(character) = to_unicode[code] else {
            continue;
        };
        if ('\u{20}'..='\u{7E}').contains(&character) {
            widths[code] = Some(f64::from(table[character as usize - 0x20]));
        }
    }
    widths
}

/// The base font of a standard-14 **text** font that can be drawn again under
/// WinAnsiEncoding, or `None` for any other font.
///
/// Such a font carries no program of its own — every viewer supplies the
/// glyphs — so a copy of it naming `/WinAnsiEncoding` paints the same
/// typeface with the whole Latin-1 set within reach. That is what lets a
/// replacement show a `ç` in a Helvetica the file declared with no
/// `/Encoding` (StandardEncoding, which has none). Symbol and ZapfDingbats
/// are excluded: their built-in encodings are the only ones their glyphs
/// have names in, so WinAnsi would paint nothing sensible. An embedded font
/// is excluded because its program may not contain the glyph at all.
pub(crate) fn winansi_capable_standard_font(
    document: &Document,
    font_dict: &Dictionary,
) -> Option<String> {
    if font_kind(document, font_dict) != FontKind::Standard14
        || resolved_name(document, font_dict, b"Subtype").as_deref() != Some("Type1")
    {
        return None;
    }
    let base_font = resolved_name(document, font_dict, b"BaseFont")?;
    (base_font != "Symbol" && base_font != "ZapfDingbats").then_some(base_font)
}

fn font_kind(document: &Document, font_dict: &Dictionary) -> FontKind {
    if resolved_name(document, font_dict, b"Subtype").as_deref() == Some("Type0") {
        return FontKind::EmbeddedComposite;
    }

    let base_font = resolved_name(document, font_dict, b"BaseFont").unwrap_or_default();
    // Subset fonts carry a six-letter tag: `ABCDEF+Helvetica`. The tag says
    // the program is embedded and subsetted, so such a font is never one of
    // the standard 14 even when the rest of the name matches.
    if !base_font.contains('+') && tables::STANDARD_14.contains(&base_font.as_str()) {
        return FontKind::Standard14;
    }

    FontKind::EmbeddedSimple
}

pub(crate) fn resolved_name(
    document: &Document,
    dictionary: &Dictionary,
    key: &[u8],
) -> Option<String> {
    let object = resolve(document, dictionary.get(key).ok()?);
    object
        .as_name()
        .ok()
        .map(|name| String::from_utf8_lossy(name).into_owned())
}

/// Follows an indirect reference to the object it names, or returns the
/// object unchanged. Content-stream resources are routinely indirect.
fn resolve<'a>(document: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => document.get_object(*id).unwrap_or(object),
        direct => direct,
    }
}

/// Builds the 256-entry code table for a simple font: a base encoding,
/// then `/Differences` applied over it.
fn simple_encoding(document: &Document, font_dict: &Dictionary) -> Vec<Option<char>> {
    let encoding = font_dict
        .get(b"Encoding")
        .ok()
        .map(|e| resolve(document, e));

    let base_name = match encoding {
        Some(Object::Name(name)) => String::from_utf8_lossy(name).into_owned(),
        Some(Object::Dictionary(dict)) => dict
            .get(b"BaseEncoding")
            .ok()
            .and_then(|base| resolve(document, base).as_name().ok())
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .unwrap_or_default(),
        _ => String::new(),
    };

    let mut table = base_table(&base_name);

    if let Some(Object::Dictionary(dict)) = encoding {
        if let Ok(differences) = dict.get(b"Differences") {
            apply_differences(&mut table, resolve(document, differences));
        }
    }

    table
}

fn base_table(base_encoding: &str) -> Vec<Option<char>> {
    let mut table = vec![None; 256];

    // The printable ASCII range is shared by every simple encoding.
    for (code, slot) in table.iter_mut().enumerate().take(0x7F).skip(0x20) {
        *slot = Some(code as u8 as char);
    }

    match base_encoding {
        "WinAnsiEncoding" => {
            // Latin-1 identity above the control block...
            for (code, slot) in table.iter_mut().enumerate().skip(0xA0) {
                *slot = char::from_u32(code as u32);
            }
            // ...and the Windows-1252 block below it.
            for &(code, character) in tables::WIN_ANSI_HIGH {
                table[code as usize] = Some(character);
            }
        }
        "MacRomanEncoding" => {
            for &(code, character) in tables::MAC_ROMAN_HIGH {
                table[code as usize] = Some(character);
            }
        }
        _ => {
            // StandardEncoding is the fallback for a font that names no
            // encoding at all, so this arm runs far more often than the
            // name suggests. Its two ASCII overrides come first for reading
            // order only — the two tables do not overlap.
            for &(code, character) in tables::STANDARD_ASCII_OVERRIDES {
                table[code as usize] = Some(character);
            }
            for &(code, character) in tables::STANDARD_HIGH {
                table[code as usize] = Some(character);
            }
        }
    }

    table
}

fn apply_differences(table: &mut [Option<char>], differences: &Object) {
    let Ok(items) = differences.as_array() else {
        return;
    };

    let mut code = 0usize;
    for item in items {
        match item {
            Object::Integer(start) => code = (*start).max(0) as usize,
            Object::Real(start) => code = (*start).max(0.0) as usize,
            Object::Name(name) => {
                if code < table.len() {
                    // An unknown glyph name clears the slot rather than
                    // leaving the base encoding's character there: the font
                    // paints something this build cannot name, and claiming
                    // otherwise would put the wrong glyph on the page.
                    table[code] = tables::char_for_glyph_name(&String::from_utf8_lossy(name));
                }
                code += 1;
            }
            _ => {}
        }
    }
}

/// Reads `/FirstChar` + `/Widths`, falling back to `/MissingWidth` from the
/// descriptor and finally to half an em.
fn simple_widths(document: &Document, font_dict: &Dictionary) -> (Vec<Option<f64>>, f64) {
    let mut widths = vec![None; 256];

    let first_char = font_dict
        .get(b"FirstChar")
        .ok()
        .and_then(|value| resolve(document, value).as_i64().ok())
        .unwrap_or(0)
        .max(0) as usize;

    if let Some(array) = font_dict
        .get(b"Widths")
        .ok()
        .and_then(|value| resolve(document, value).as_array().ok())
    {
        for (offset, entry) in array.iter().enumerate() {
            let code = first_char + offset;
            if code >= widths.len() {
                break;
            }
            widths[code] = number(resolve(document, entry));
        }
    }

    let missing_width = font_dict
        .get(b"FontDescriptor")
        .ok()
        .and_then(|descriptor| resolve(document, descriptor).as_dict().ok())
        .and_then(|descriptor| descriptor.get(b"MissingWidth").ok())
        .and_then(|value| number(resolve(document, value)))
        .unwrap_or(FALLBACK_WIDTH);

    (widths, missing_width)
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(*value as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn empty_doc() -> Document {
        Document::with_version("1.7")
    }

    fn resolve_in(font: Dictionary) -> FontInfo {
        resolve_font(&empty_doc(), &font, "F1").expect("resolvable font")
    }

    fn helvetica() -> Dictionary {
        dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        }
    }

    #[test]
    fn a_base_14_name_without_a_subset_tag_is_a_standard_font() {
        assert_eq!(resolve_in(helvetica()).kind, FontKind::Standard14);
    }

    /// A subset tag means an embedded font program, whatever the rest of the
    /// name says — treating `ABCDEF+Helvetica` as standard would encode
    /// against a character set the embedded subset may not carry.
    #[test]
    fn a_subset_tagged_font_is_embedded_even_when_named_after_a_standard_one() {
        let font = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "ABCDEF+Helvetica",
        };

        assert_eq!(resolve_in(font).kind, FontKind::EmbeddedSimple);
    }

    #[test]
    fn a_type0_font_is_composite() {
        let font = dictionary! {
            "Subtype" => "Type0",
            "BaseFont" => "ABCDEF+NotoSans",
        };

        assert_eq!(resolve_in(font).kind, FontKind::EmbeddedComposite);
    }

    #[test]
    fn winansi_encodes_ascii_unchanged() {
        assert_eq!(
            resolve_in(helvetica())
                .encode("Hello, world!")
                .expect("ascii"),
            b"Hello, world!".to_vec()
        );
    }

    #[test]
    fn standard14_widths_come_from_the_real_afm_metrics_not_a_flat_fallback() {
        let font = resolve_in(helvetica());
        let i_width = font.width_of(&font.encode("i").expect("ascii"));
        let m_width = font.width_of(&font.encode("m").expect("ascii"));

        // Real Helvetica: 'i' is narrow (222/1000em), 'm' is wide (833/1000em).
        // The old flat fallback reported both alike, at 500/1000em.
        assert!((i_width - 0.222).abs() < 1e-9);
        assert!((m_width - 0.833).abs() < 1e-9);
        assert!(i_width < m_width);
    }

    /// The font's own `/Widths` array is the document's authoritative data
    /// and must win over the table this crate ships.
    #[test]
    fn a_documents_own_widths_array_is_never_overridden() {
        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
            "FirstChar" => 105, // 'i'
            "LastChar" => 105,
            "Widths" => vec![999.into()],
        };
        let resolved = resolve_in(font);
        let width = resolved.width_of(&resolved.encode("i").expect("ascii"));

        assert!((width - 0.999).abs() < 1e-9);
    }

    #[test]
    fn courier_is_flat_600_per_glyph() {
        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
            "Encoding" => "WinAnsiEncoding",
        };
        let resolved = resolve_in(font);
        let narrow = resolved.width_of(&resolved.encode("i").expect("ascii"));
        let wide = resolved.width_of(&resolved.encode("m").expect("ascii"));

        assert_eq!(narrow, wide, "Courier is monospaced");
        assert!((narrow - 0.6).abs() < 1e-9);
    }

    /// Embedded fonts have their own `/Widths` semantics; the Standard-14
    /// AFM table must never kick in for them, standard-14-named or not.
    #[test]
    fn a_non_standard14_font_keeps_the_generic_fallback() {
        let font = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "ABCDEF+Helvetica",
            "Encoding" => "WinAnsiEncoding",
        };
        let resolved = resolve_in(font);
        let width = resolved.width_of(&resolved.encode("i").expect("ascii"));

        assert!((width - 0.5).abs() < 1e-9);
    }

    #[test]
    fn winansi_encodes_latin1_accents() {
        assert_eq!(
            resolve_in(helvetica()).encode("café").expect("latin-1"),
            vec![b'c', b'a', b'f', 0xE9]
        );
    }

    #[test]
    fn winansi_encodes_the_windows_1252_block() {
        let font = resolve_in(helvetica());

        assert_eq!(font.encode("€").expect("euro"), vec![0x80]);
        assert_eq!(font.encode("—").expect("em dash"), vec![0x97]);
        assert_eq!(font.encode("“quoted”").expect("curly quotes")[0], 0x93);
    }

    /// The heart of decision 3: the failure names the character, so a shell
    /// can tell the user *which* one it could not write.
    #[test]
    fn an_unrepresentable_character_is_reported_by_name_not_as_a_blanket_refusal() {
        let error = resolve_in(helvetica())
            .encode("日本語")
            .expect_err("a Latin font cannot write these");

        assert_eq!(
            error,
            EditError::EncodingGap {
                character: '日',
                resource_font_name: "F1".to_string(),
            }
        );
    }

    /// The same font, two replacement strings, two different answers — which
    /// is why `FontKind` is not an `editable` flag.
    #[test]
    fn the_same_font_accepts_one_replacement_and_refuses_another() {
        let font = resolve_in(helvetica());

        assert!(font.encode("café").is_ok());
        assert!(font.encode("日本語").is_err());
    }

    #[test]
    fn a_composite_font_refuses_every_replacement_including_plain_ascii() {
        let font = dictionary! { "Subtype" => "Type0", "BaseFont" => "AAAAAA+Noto" };
        let error = resolve_in(font)
            .encode("abc")
            .expect_err("v1 refuses Type0");

        assert_eq!(
            error,
            EditError::CompositeFontNotEditable {
                resource_font_name: "F1".to_string(),
            }
        );
    }

    #[test]
    fn differences_override_the_base_encoding_in_both_directions() {
        let font = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => dictionary! {
                "BaseEncoding" => "WinAnsiEncoding",
                "Differences" => vec![65.into(), "bullet".into()],
            },
        };
        let font = resolve_in(font);

        assert_eq!(font.decode(&[65]), "•");
        assert_eq!(font.encode("•").expect("remapped"), vec![65]);
        assert!(
            font.encode("A").is_err(),
            "code 65 no longer paints an A, so writing one must be refused"
        );
    }

    #[test]
    fn differences_accept_the_uni_glyph_name_form() {
        let font = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => dictionary! {
                "Differences" => vec![200.into(), "uni00E9".into()],
            },
        };

        assert_eq!(resolve_in(font).decode(&[200]), "é");
    }

    /// A glyph name this build does not know clears the slot. The
    /// alternative — leaving the base encoding's character in place — would
    /// report text the page does not actually show.
    #[test]
    fn an_unknown_glyph_name_leaves_the_code_unmapped() {
        let font = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => dictionary! {
                "BaseEncoding" => "WinAnsiEncoding",
                "Differences" => vec![65.into(), "someUnknownGlyph".into()],
            },
        };
        let font = resolve_in(font);

        assert_eq!(font.decode(&[65]), "\u{FFFD}");
        assert!(font.encode("A").is_err());
    }

    #[test]
    fn decoding_never_fails_even_on_codes_it_cannot_map() {
        // 0x81 is undefined in WinAnsiEncoding.
        assert_eq!(resolve_in(helvetica()).decode(&[b'A', 0x81]), "A\u{FFFD}");
    }

    #[test]
    fn a_font_with_no_encoding_entry_still_handles_ascii() {
        let font = dictionary! { "Subtype" => "Type1", "BaseFont" => "Times-Roman" };

        assert_eq!(
            resolve_in(font).encode("Hi").expect("ascii"),
            b"Hi".to_vec()
        );
    }

    #[test]
    fn width_of_reads_the_widths_array_offset_by_first_char() {
        let font = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
            "FirstChar" => 65,
            "Widths" => vec![700.into(), 300.into()],
        };
        let font = resolve_in(font);

        assert!((font.width_of(b"AB") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn width_of_falls_back_to_missing_width_then_to_half_an_em() {
        let with_descriptor = dictionary! {
            "Subtype" => "TrueType",
            "BaseFont" => "AAAAAA+Arial",
            "FontDescriptor" => dictionary! { "MissingWidth" => 250 },
        };
        assert!((resolve_in(with_descriptor).width_of(b"AB") - 0.5).abs() < 1e-9);

        // Standard-14 ASCII now comes from the real AFM table (see
        // `standard14_widths_come_from_the_real_afm_metrics_not_a_flat_fallback`),
        // but the half-em fallback still applies outside that table's ASCII
        // scope — an accented character, here.
        let helvetica = resolve_in(helvetica());
        let accented = helvetica.encode("é").expect("winansi covers this");
        assert!((helvetica.width_of(&accented) - 0.5).abs() < 1e-9);
    }

    fn mac_roman() -> FontInfo {
        resolve_in(dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Times-Roman",
            "Encoding" => "MacRomanEncoding",
        })
    }

    fn standard() -> FontInfo {
        resolve_in(dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Times-Roman",
            "Encoding" => "StandardEncoding",
        })
    }

    #[test]
    fn macroman_maps_its_upper_half_in_both_directions() {
        let font = mac_roman();

        assert_eq!(font.encode("Hi").expect("ascii"), b"Hi".to_vec());
        // Its accented block sits nowhere near Latin-1's: `é` is 0x8E here
        // and 0xE9 in WinAnsi, which is exactly why the two need separate
        // tables rather than one shared "high half".
        assert_eq!(font.encode("é").expect("mac roman"), vec![0x8E]);
        assert_eq!(font.decode(&[0x8E]), "é");
        assert_eq!(font.decode(&[0xD0, 0xD1]), "–—");
        assert_eq!(font.encode("Œ").expect("OE"), vec![0xCE]);
        assert_eq!(font.encode("\u{00A0}").expect("nbsp"), vec![0xCA]);
    }

    /// Mac OS Roman's non-Latin slots are not in Annex D's table — it only
    /// enumerates the Latin set — but no second authority claims them, so
    /// they are mapped rather than left as holes.
    #[test]
    fn macroman_maps_the_mac_os_slots_annex_d_does_not_enumerate() {
        let font = mac_roman();

        assert_eq!(font.decode(&[0xAD]), "≠");
        assert_eq!(font.encode("π").expect("pi"), vec![0xB9]);
        assert_eq!(font.encode("√").expect("radical"), vec![0xC3]);
    }

    /// The two codes where the authorities genuinely disagree stay unmapped:
    /// Annex D calls 0xDB `currency`, Mac OS Roman calls it the Euro, and
    /// 0xF0 is Apple's private-use logo. Refusing both is this module's whole
    /// policy — an unwritable code costs an edit, a wrong one costs the page.
    #[test]
    fn macroman_leaves_the_two_contested_codes_unmapped() {
        let font = mac_roman();

        assert_eq!(font.decode(&[0xDB, 0xF0]), "\u{FFFD}\u{FFFD}");
        // `currency` is still writable — through WinAnsi's own code for it,
        // not through the contested one.
        assert!(font.encode("¤").is_err());
    }

    #[test]
    fn standard_encoding_maps_its_upper_half_in_both_directions() {
        let font = standard();

        assert_eq!(font.decode(&[0xD0]), "—");
        assert_eq!(font.encode("—").expect("em dash"), vec![0xD0]);
        assert_eq!(font.encode("Œ").expect("OE"), vec![0xEA]);
        assert_eq!(font.encode("Ł").expect("Lslash"), vec![0xE8]);
        assert_eq!(font.decode(&[0xFB]), "ß");
    }

    /// StandardEncoding's own trap: 0x27/0x60 paint curly quotes, and the
    /// plain apostrophe and grave live up in the high half. Getting this
    /// backwards would silently swap a straight quote for a curly one.
    #[test]
    fn standard_encoding_keeps_the_straight_quotes_in_its_upper_half() {
        let font = standard();

        assert_eq!(font.decode(&[0x27, 0x60]), "’‘");
        assert_eq!(font.encode("'").expect("quotesingle"), vec![0xA9]);
        assert_eq!(font.encode("`").expect("grave"), vec![0xC1]);
    }

    /// StandardEncoding has no Latin-1 block: `é` is simply not in it, and
    /// must stay a refusal even now that the rest of the high half is mapped.
    #[test]
    fn standard_encoding_still_refuses_what_it_genuinely_cannot_paint() {
        assert!(standard().encode("é").is_err());
    }

    /// `uacute` strips to `acute` — the right length for the `uXXXX` code
    /// point form and not hexadecimal — so the shortcut has to fall through
    /// to the glyph table instead of deciding the name is unknown.
    #[test]
    fn a_glyph_name_that_only_looks_like_a_code_point_still_resolves() {
        let font = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => dictionary! {
                "BaseEncoding" => "WinAnsiEncoding",
                "Differences" => vec![200.into(), "uacute".into(), "ugrave".into()],
            },
        };

        assert_eq!(resolve_in(font).decode(&[200, 201]), "úù");
    }

    /// Resolves `font` with `cmap` attached as its `/ToUnicode` stream.
    fn resolve_with_cmap(mut font: Dictionary, cmap: &str) -> FontInfo {
        let mut document = empty_doc();
        let stream =
            document.add_object(lopdf::Stream::new(dictionary! {}, cmap.as_bytes().to_vec()));
        font.set("ToUnicode", stream);
        resolve_font(&document, &font, "F1").expect("resolvable font")
    }

    /// A reportlab-style subset: no `/Encoding`, the accented letters on
    /// low codes only the CMap explains, and `/Widths` for codes 0..=0x7F.
    fn subset_with_cmap(cmap: &str) -> FontInfo {
        let widths: Vec<Object> = (0..=0x7F).map(|_| 600.into()).collect();
        resolve_with_cmap(
            dictionary! {
                "Subtype" => "TrueType",
                "BaseFont" => "AAAAAA+Vera",
                "FirstChar" => 0,
                "LastChar" => 0x7F,
                "Widths" => widths,
            },
            cmap,
        )
    }

    const SUBSET_CMAP: &str = "1 begincodespacerange <00> <7F> endcodespacerange\n\
        3 beginbfchar <00> <0000> <01> <00D3> <02> <00660069> endbfchar\n\
        1 beginbfrange <20> <7E> <0020> endbfrange";

    #[test]
    fn to_unicode_decodes_codes_the_encoding_cannot() {
        let font = subset_with_cmap(SUBSET_CMAP);

        assert_eq!(font.decode(b"COMPOSICI\x01N"), "COMPOSICIÓN");
        assert_eq!(
            font.decode(&[0x02]),
            "fi",
            "a ligature decodes to both letters"
        );
    }

    #[test]
    fn to_unicode_outranks_differences_and_falls_back_per_code() {
        let font = resolve_with_cmap(
            dictionary! {
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica",
                "Encoding" => dictionary! {
                    "BaseEncoding" => "WinAnsiEncoding",
                    "Differences" => vec![65.into(), "B".into()],
                },
            },
            "1 beginbfchar <41> <0058> endbfchar",
        );

        assert_eq!(font.decode(b"AC"), "XC", "A by the CMap, C by the encoding");
        assert_eq!(
            font.encode("X").expect("WinAnsi"),
            vec![b'X'],
            "a full font with an /Encoding writes by its encoding"
        );
    }

    #[test]
    fn a_subset_encodes_through_the_reverse_to_unicode_map() {
        let font = subset_with_cmap(SUBSET_CMAP);

        assert_eq!(font.encode("NÓ").expect("in the subset"), vec![0x4E, 0x01]);
        assert!(font.width_of(&[0x01]) > 0.0);
    }

    #[test]
    fn a_space_encodes_as_code_32_so_word_spacing_applies() {
        let font = subset_with_cmap(
            "2 beginbfchar <03> <0020> <20> <0020> endbfchar\n\
             1 beginbfrange <21> <7E> <0021> endbfrange",
        );

        assert_eq!(font.encode("a b").expect("in the subset"), b"a b".to_vec());
    }

    #[test]
    fn a_character_the_subset_lacks_is_an_encoding_gap_naming_it() {
        let font = subset_with_cmap(SUBSET_CMAP);

        assert_eq!(
            font.encode("Ñ").expect_err("not in the subset"),
            EditError::EncodingGap {
                character: 'Ñ',
                resource_font_name: "F1".to_string(),
            }
        );
        // A ligature code cannot be selected by typing one of its letters,
        // and an unmapped StandardEncoding code is not trusted in a subset.
        assert!(font.encode("\u{FB01}").is_err());
        assert!(
            font.encode("¡").is_err(),
            "StandardEncoding's 0xA1 is past LastChar"
        );
    }

    #[test]
    fn a_reverse_mapped_code_needs_a_declared_width() {
        let font = resolve_with_cmap(
            dictionary! {
                "Subtype" => "TrueType",
                "BaseFont" => "AAAAAA+Vera",
                "FirstChar" => 0x41,
                "LastChar" => 0x41,
                "Widths" => vec![600.into()],
            },
            "2 beginbfchar <41> <0041> <05> <00E9> endbfchar",
        );

        assert_eq!(font.encode("A").expect("declared"), vec![0x41]);
        assert!(font.encode("é").is_err(), "code 5 has no width");
        assert_eq!(font.decode(&[0x05]), "é", "decoding is still best-effort");
    }

    #[test]
    fn a_non_subset_font_keeps_its_encoding_for_codes_the_cmap_omits() {
        let font = resolve_with_cmap(helvetica(), "1 beginbfchar <41> <0041> endbfchar");

        assert_eq!(font.encode("Bé").expect("WinAnsi"), vec![b'B', 0xE9]);
    }

    #[test]
    fn a_composite_font_decodes_two_byte_codes_through_to_unicode_but_stays_uneditable() {
        let font = resolve_with_cmap(
            dictionary! { "Subtype" => "Type0", "BaseFont" => "AAAAAA+Noto" },
            "1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
             2 beginbfchar <0003> <00C1> <0004> <0062> endbfchar",
        );

        assert_eq!(
            font.decode(&[0x00, 0x03, 0x00, 0x04, 0x00, 0x09]),
            "Áb\u{FFFD}"
        );
        assert!(matches!(
            font.encode("b"),
            Err(EditError::CompositeFontNotEditable { .. })
        ));
    }

    #[test]
    fn an_unreadable_to_unicode_leaves_the_encoding_in_charge() {
        let mut font = helvetica();
        font.set("ToUnicode", "Identity-H");

        assert_eq!(resolve_in(font.clone()).decode(b"A"), "A");
        assert_eq!(
            resolve_with_cmap(font, "garbage").encode("é").unwrap(),
            vec![0xE9]
        );
    }
}
