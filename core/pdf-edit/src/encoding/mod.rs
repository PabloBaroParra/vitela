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

pub mod tables;

use crate::error::EditError;
use lopdf::{Dictionary, Document, Object};
use pdf_document::FontKind;

/// The advance width assumed for a code the font gives no width for.
///
/// Half an em is the conventional fallback. It only affects the reported
/// bounding box of a run — never what gets written — so an imprecise box on
/// an unusual font costs hit-testing accuracy in the shell, not integrity.
const FALLBACK_WIDTH: f64 = 500.0;

/// Everything this crate needs to know about one font resource: how its
/// codes map to characters, and how wide each code is.
#[derive(Debug, Clone, PartialEq)]
pub struct FontInfo {
    pub resource_name: String,
    pub kind: FontKind,
    /// Code to character. `None` marks a code this build cannot map — the
    /// entry that makes an edit fail loudly instead of silently wrong.
    to_unicode: Vec<Option<char>>,
    /// Advance widths in glyph space (thousandths of an em), by code.
    widths: Vec<Option<f64>>,
    missing_width: f64,
}

impl FontInfo {
    /// Reads `codes` as text, substituting U+FFFD for codes this build
    /// cannot map. Never fails: a run that cannot be fully decoded must
    /// still be visible to the user, it just cannot be edited.
    pub fn decode(&self, codes: &[u8]) -> String {
        codes
            .iter()
            .map(|&code| self.to_unicode[code as usize].unwrap_or('\u{FFFD}'))
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
    /// save output reproducible.
    fn code_for(&self, character: char) -> Option<u8> {
        self.to_unicode
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
    let widths = if kind == FontKind::Standard14 {
        let base_font = resolved_name(document, font_dict, b"BaseFont").unwrap_or_default();
        apply_standard_14_widths(widths, &base_font, &to_unicode)
    } else {
        widths
    };

    Ok(FontInfo {
        resource_name: resource_name.to_string(),
        kind,
        to_unicode,
        widths,
        missing_width,
    })
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

fn resolved_name(document: &Document, dictionary: &Dictionary, key: &[u8]) -> Option<String> {
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
        // MacRomanEncoding's upper half is intentionally unmapped in v1 —
        // see `tables::STANDARD_OVERRIDES`. It behaves as ASCII-only, which
        // rejects edits it cannot make rather than guessing them.
        "MacRomanEncoding" => {}
        _ => {
            for &(code, character) in tables::STANDARD_OVERRIDES {
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

    /// A documented v1 limitation, pinned by a test so it is a decision and
    /// not a surprise: MacRomanEncoding's upper half is unmapped, so those
    /// characters are refused rather than guessed.
    #[test]
    fn macroman_handles_ascii_and_refuses_its_unmapped_upper_half() {
        let font = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Times-Roman",
            "Encoding" => "MacRomanEncoding",
        };
        let font = resolve_in(font);

        assert_eq!(font.encode("Hi").expect("ascii"), b"Hi".to_vec());
        assert!(font.encode("é").is_err());
    }
}
