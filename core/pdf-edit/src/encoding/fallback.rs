//! Characters a run's own font cannot show, written in the closest
//! standard-14 font instead — and only those characters.
//!
//! An embedded subset carries the glyphs its producer used and nothing
//! else, so typing a letter the original text never had (a `ñ` into a quote
//! that had none) used to be an [`EditError::EncodingGap`]. Now the text is
//! split into stretches: every character the run's font can encode stays in
//! it, and each stretch it cannot is written in a standard-14 font picked to
//! look like it (sans, serif or fixed pitch; bold and italic kept), switched
//! to with `Tf` and back. A character neither font has — WinAnsi has no
//! `漢` — is still the run's own `EncodingGap`, named against its font.
//!
//! The substitute is registered under a reserved resource name
//! ([`FALLBACK_RESOURCE_PREFIX`]), which is how the grouping pass recognises
//! a run in it as part of the line around it rather than a different font.

use super::{resolved_name, tables, FontInfo};
use crate::error::EditError;
use lopdf::{Dictionary, Document, Object};
use pdf_document::FontKind;

/// The resource-name prefix of every fallback font this crate registers:
/// `VFb1.F1`, `VFb2.F1`, … Nothing else is written under it, so a run in a font
/// by such a name is a stretch of a line in the font after the dot.
pub const FALLBACK_RESOURCE_PREFIX: &str = "VFb";

/// `/FontDescriptor /Flags` bits (PDF 32000-1 §9.8.2), 1-based in the spec.
const FIXED_PITCH: i64 = 1 << 0;
const SERIF: i64 = 1 << 1;
const ITALIC: i64 = 1 << 6;
const FORCE_BOLD: i64 = 1 << 18;

/// The weight from which a `/FontWeight` reads as bold (600 is semibold).
const BOLD_WEIGHT: f64 = 600.0;

/// The font a fallback resource was written for (`F1` for `VFb2.F1`); `None`
/// for any other name, a source font that happens to be called `VFb1` too.
pub fn owner_of(resource_name: &str) -> Option<&str> {
    let rest = resource_name.strip_prefix(FALLBACK_RESOURCE_PREFIX)?;
    let (number, owner) = rest.split_once('.')?;
    let numbered = !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
    (numbered && !owner.is_empty()).then_some(owner)
}

/// Whether `resource_name` is one of the fallback fonts this crate writes —
/// see [`owner_of`].
pub fn is_fallback_resource(resource_name: &str) -> bool {
    owner_of(resource_name).is_some()
}

/// One stretch of text in one font: `fallback` is false for the run's own
/// font and true for its standard-14 substitute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub fallback: bool,
    pub codes: Vec<u8>,
}

/// What a font looks like, as far as picking a standard-14 face goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Look {
    fixed: bool,
    serif: bool,
    bold: bool,
    italic: bool,
}

/// The standard-14 face closest to the font `font_dict` describes. A
/// standard-14 text font is its own closest face.
pub(crate) fn closest_standard_font(document: &Document, font_dict: &Dictionary) -> &'static str {
    let base_font = resolved_name(document, font_dict, b"BaseFont").unwrap_or_default();
    if let Some(standard) = tables::STANDARD_14
        .iter()
        .find(|name| **name == base_font && !matches!(**name, "Symbol" | "ZapfDingbats"))
    {
        return standard;
    }
    let descriptor = descriptor(document, font_dict);
    let number = |key: &[u8]| {
        descriptor
            .as_ref()
            .and_then(|descriptor| descriptor.get(key).ok())
            .and_then(|value| super::number(super::resolve(document, value)))
    };
    let look = look_of(
        &base_font,
        number(b"Flags").map(|flags| flags as i64),
        number(b"FontWeight"),
        number(b"ItalicAngle"),
    );
    face_for(look)
}

fn descriptor(document: &Document, font_dict: &Dictionary) -> Option<Dictionary> {
    let object = super::resolve(document, font_dict.get(b"FontDescriptor").ok()?);
    match object {
        Object::Dictionary(dictionary) => Some(dictionary.clone()),
        _ => None,
    }
}

/// Reads the look from the font's name and its descriptor. The name wins
/// where it is explicit: producers set `/Flags` carelessly (a sans font
/// flagged serif is common), while `…Sans-Bold` says what it is.
fn look_of(
    base_font: &str,
    flags: Option<i64>,
    weight: Option<f64>,
    italic_angle: Option<f64>,
) -> Look {
    // `ABCDEF+Name`: the subset tag says nothing about the face.
    let name = base_font
        .split_once('+')
        .map_or(base_font, |(_, name)| name)
        .to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
    let flags = flags.unwrap_or(0);

    let fixed = flags & FIXED_PITCH != 0 || has(&["courier", "mono", "consol", "typewriter"]);
    let named_sans = has(&["sans", "arial", "helvetica", "verdana", "tahoma", "calibri"]);
    let named_serif = has(&["times", "serif", "georgia", "garamond", "cambria"]);
    let serif = !named_sans && (named_serif || flags & SERIF != 0);
    let bold = flags & FORCE_BOLD != 0
        || weight.is_some_and(|weight| weight >= BOLD_WEIGHT)
        || has(&["bold", "black", "heavy", "demi", "semibold"]);
    let italic = flags & ITALIC != 0
        || italic_angle.is_some_and(|angle| angle.abs() > f64::EPSILON)
        || has(&["italic", "oblique"]);
    Look {
        fixed,
        serif,
        bold,
        italic,
    }
}

fn face_for(look: Look) -> &'static str {
    match (look.fixed, look.serif, look.bold, look.italic) {
        (true, _, false, false) => "Courier",
        (true, _, true, false) => "Courier-Bold",
        (true, _, false, true) => "Courier-Oblique",
        (true, _, true, true) => "Courier-BoldOblique",
        (false, true, false, false) => "Times-Roman",
        (false, true, true, false) => "Times-Bold",
        (false, true, false, true) => "Times-Italic",
        (false, true, true, true) => "Times-BoldItalic",
        (false, false, false, false) => "Helvetica",
        (false, false, true, false) => "Helvetica-Bold",
        (false, false, false, true) => "Helvetica-Oblique",
        (false, false, true, true) => "Helvetica-BoldOblique",
    }
}

/// `base_font` as the WinAnsi-encoded standard-14 font every fallback is
/// written as — built without a document, since it has no indirect parts.
pub(crate) fn standard_font_info(base_font: &str, resource_name: &str) -> FontInfo {
    let to_unicode = super::base_table("WinAnsiEncoding");
    let widths = super::apply_standard_14_widths(vec![None; 256], base_font, &to_unicode);
    FontInfo {
        resource_name: resource_name.to_string(),
        kind: FontKind::Standard14,
        encodable: to_unicode.clone(),
        to_unicode,
        cmap: None,
        widths,
        missing_width: super::FALLBACK_WIDTH,
        fallback_base_font: face_for(Look::default()),
    }
}

impl FontInfo {
    /// The standard-14 face this font's missing characters are written in.
    pub fn fallback_base_font(&self) -> &'static str {
        self.fallback_base_font
    }

    /// Encodes `text`, splitting it into maximal stretches this font can
    /// show and stretches only its fallback can. A character neither has is
    /// an [`EditError::EncodingGap`] naming this font.
    pub fn encode_with_fallback(&self, text: &str) -> Result<Vec<Segment>, EditError> {
        if self.kind == FontKind::EmbeddedComposite {
            return Err(EditError::CompositeFontNotEditable {
                resource_font_name: self.resource_name.clone(),
            });
        }
        let fallback = standard_font_info(self.fallback_base_font, &self.resource_name);
        let mut segments: Vec<Segment> = Vec::new();
        for character in text.chars() {
            let (in_fallback, code) = match self.code_for(character) {
                Some(code) => (false, code),
                None => match fallback.code_for(character) {
                    Some(code) => (true, code),
                    None => {
                        return Err(EditError::EncodingGap {
                            character,
                            resource_font_name: self.resource_name.clone(),
                        })
                    }
                },
            };
            match segments.last_mut() {
                Some(last) if last.fallback == in_fallback => last.codes.push(code),
                _ => segments.push(Segment {
                    fallback: in_fallback,
                    codes: vec![code],
                }),
            }
        }
        Ok(segments)
    }

    /// The advance of `text` as [`Self::encode_with_fallback`] writes it, in
    /// ems — the measure every box and wrap of such a text must use.
    pub fn width_with_fallback(&self, text: &str) -> Result<f64, EditError> {
        let fallback = standard_font_info(self.fallback_base_font, &self.resource_name);
        Ok(self
            .encode_with_fallback(text)?
            .iter()
            .map(|segment| {
                if segment.fallback {
                    fallback.width_of(&segment.codes)
                } else {
                    self.width_of(&segment.codes)
                }
            })
            .sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::resolve_font;
    use crate::encoding::winansi::encode_winansi;
    use lopdf::dictionary;

    fn face(base_font: &str, flags: Option<i64>, weight: Option<f64>, angle: Option<f64>) -> &str {
        face_for(look_of(base_font, flags, weight, angle))
    }

    #[test]
    fn a_sans_subset_falls_back_to_helvetica() {
        assert_eq!(
            face("AAAAAA+BitstreamVeraSans-Roman", Some(4), None, Some(0.0)),
            "Helvetica"
        );
        assert_eq!(
            face("ABCDEF+Arial-BoldMT", None, None, None),
            "Helvetica-Bold"
        );
        assert_eq!(
            face("ABCDEF+OpenSans-Italic", None, None, None),
            "Helvetica-Oblique"
        );
    }

    #[test]
    fn a_name_or_flag_saying_serif_falls_back_to_times() {
        assert_eq!(
            face("ABCDEF+TimesNewRomanPSMT", None, None, None),
            "Times-Roman"
        );
        assert_eq!(
            face("ABCDEF+Minion", Some(SERIF), None, None),
            "Times-Roman"
        );
        assert_eq!(
            face("ABCDEF+Minion", Some(SERIF | ITALIC), Some(700.0), None),
            "Times-BoldItalic"
        );
    }

    /// A sans name outranks a careless serif flag.
    #[test]
    fn a_sans_name_outranks_a_serif_flag() {
        assert_eq!(
            face("ABCDEF+DejaVuSans", Some(SERIF), None, None),
            "Helvetica"
        );
    }

    #[test]
    fn fixed_pitch_falls_back_to_courier() {
        assert_eq!(face("ABCDEF+Foo", Some(FIXED_PITCH), None, None), "Courier");
        assert_eq!(
            face("ABCDEF+DejaVuSansMono-Bold", None, None, None),
            "Courier-Bold"
        );
        assert_eq!(
            face("ABCDEF+Consolas", None, None, Some(-12.0)),
            "Courier-Oblique"
        );
    }

    #[test]
    fn weight_and_force_bold_read_as_bold() {
        assert_eq!(
            face("ABCDEF+Foo", None, Some(600.0), None),
            "Helvetica-Bold"
        );
        assert_eq!(
            face("ABCDEF+Foo", Some(FORCE_BOLD), None, None),
            "Helvetica-Bold"
        );
        assert_eq!(face("ABCDEF+Foo", None, Some(400.0), None), "Helvetica");
    }

    #[test]
    fn an_unknown_font_falls_back_to_helvetica() {
        assert_eq!(face("", None, None, None), "Helvetica");
    }

    #[test]
    fn only_numbered_vfb_names_with_an_owner_are_fallback_resources() {
        assert_eq!(owner_of("VFb1.F1"), Some("F1"));
        assert_eq!(owner_of("VFb12.F1.Bold"), Some("F1.Bold"));
        assert_eq!(owner_of("VFb1"), None);
        for name in ["VFb", "VFb.F1", "VFb1.", "VFbX.F1", "F1"] {
            assert!(!is_fallback_resource(name), "{name}");
        }
    }

    /// A serif subset font that only has `a` and `b`.
    fn subset_of_ab() -> FontInfo {
        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "ABCDEF+Times-Subset",
            "FirstChar" => 97,
            "LastChar" => 98,
            "Widths" => vec![Object::Integer(400), Object::Integer(450)],
        };
        let mut info = resolve_font(&Document::with_version("1.7"), &font, "F9").unwrap();
        // Only `a` and `b`: everything else is a gap in this subset.
        info.encodable = (0..256)
            .map(|code| matches!(code, 97 | 98).then(|| code as u8 as char))
            .collect();
        info
    }

    #[test]
    fn the_split_keeps_every_character_the_font_has_in_it() {
        let font = subset_of_ab();
        assert_eq!(font.fallback_base_font(), "Times-Roman");

        let segments = font.encode_with_fallback("abñab c").unwrap();

        assert_eq!(
            segments,
            vec![
                Segment {
                    fallback: false,
                    codes: b"ab".to_vec()
                },
                Segment {
                    fallback: true,
                    codes: vec![0xF1]
                },
                Segment {
                    fallback: false,
                    codes: b"ab".to_vec()
                },
                Segment {
                    fallback: true,
                    codes: b" c".to_vec()
                },
            ]
        );
    }

    #[test]
    fn text_the_font_has_whole_is_one_segment() {
        let segments = subset_of_ab().encode_with_fallback("abba").unwrap();
        assert_eq!(
            segments,
            vec![Segment {
                fallback: false,
                codes: b"abba".to_vec()
            }]
        );
    }

    #[test]
    fn a_character_neither_font_has_is_the_runs_own_gap() {
        assert_eq!(
            subset_of_ab().encode_with_fallback("a漢"),
            Err(EditError::EncodingGap {
                character: '漢',
                resource_font_name: "F9".to_string(),
            })
        );
    }

    /// Times-Roman's `c` is 444 thousandths, the subset's `a` 400.
    #[test]
    fn the_width_adds_each_segment_in_its_own_font() {
        let width = subset_of_ab().width_with_fallback("ac").unwrap();
        assert!((width - (0.400 + 0.444)).abs() < 1e-9, "{width}");
    }

    /// A fallback measures accented and typographic characters at the
    /// face's AFM widths, not at the half-em default for a missing width.
    #[test]
    fn a_fallback_measures_non_ascii_at_its_afm_width() {
        let helvetica = standard_font_info("Helvetica", "VFb1.F1");
        let codes = encode_winansi("ñé€Ñ").unwrap();
        let expected = 0.556 * 3.0 + 0.722;
        assert!((helvetica.width_of(&codes) - expected).abs() < 1e-9);

        let times = standard_font_info("Times-Roman", "VFb1.F1");
        assert!((times.width_of(&encode_winansi("é").unwrap()) - 0.444).abs() < 1e-9);
    }
}
