//! `/DA` (default appearance string) serialization (T-135).
//!
//! A `/DA` value is a tiny fragment of content-stream-like syntax, e.g.
//! `"0 0 0 rg /Helv 12 Tf"`: a color-setting operator (`rg` for RGB, `g` for
//! gray — decision 3 restricts *this crate's own writes* to `rg`, but
//! reading tolerates `g` too since existing AcroForms may use it) followed
//! by a font-selection operator naming a `/DR /Font` resource and a size.
//!
//! Parsing is intentionally infallible (`parse_da` always returns a
//! `TextStyle`, never a `Result`): a `/DA` on a foreign field may use syntax
//! this crate does not model (CMYK `k`, extra operators like `Tr`), and
//! T-137's read path cannot let one malformed field abort reading the whole
//! AcroForm. Decision 3's own default — Helvetica, 12pt, black — is the
//! fallback, applied **per attribute**: a `/DA` this crate only half
//! understands gives up only the half it does not (T-205).
//!
//! A `/Btn` field's `/DA` is written by [`format_button_da`], not
//! [`format_da`] — see its doc for why the two shapes differ.

use pdf_document::{Color, FontFamily, TextStyle};

fn default_style() -> TextStyle {
    TextStyle {
        font: FontFamily::Helvetica,
        size_pt: 12.0,
        color: Color { r: 0, g: 0, b: 0 },
    }
}

/// The `/DR /Font` resource name this crate registers for each family —
/// Adobe's own conventional abbreviations, also what `pdf-form::forms`
/// (T-138) writes into `/DR` when it ensures the AcroForm resource dict.
pub fn resource_name(font: FontFamily) -> &'static str {
    match font {
        FontFamily::Helvetica => "Helv",
        FontFamily::TimesRoman => "TiRo",
        FontFamily::Courier => "Cour",
    }
}

/// The `/BaseFont` name for each family — also the key
/// `pdf_edit::encoding::tables::standard_14_ascii_widths` expects, which is
/// how `appearance.rs` (T-136) measures text for word-wrap without a second,
/// independently-maintained copy of the AFM metrics.
pub fn base_font_name(font: FontFamily) -> &'static str {
    match font {
        FontFamily::Helvetica => "Helvetica",
        FontFamily::TimesRoman => "Times-Roman",
        FontFamily::Courier => "Courier",
    }
}

fn family_from_resource_name(name: &str) -> Option<FontFamily> {
    match name {
        "Helv" => Some(FontFamily::Helvetica),
        "TiRo" => Some(FontFamily::TimesRoman),
        "Cour" => Some(FontFamily::Courier),
        _ => None,
    }
}

/// Formats a PDF real number the way this crate's own `/DA` writer does:
/// integral values with no decimal point, fractional values trimmed of
/// trailing zeros. Not a general-purpose PDF number formatter — just enough
/// to make `format_da`'s output match the fixture example byte-for-byte.
pub(crate) fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value:.5}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn byte_to_channel(byte: u8) -> f64 {
    byte as f64 / 255.0
}

fn channel_to_byte(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Serializes a `TextStyle` as a `/DA` string, e.g. `"0 0 0 rg /Helv 12 Tf"`.
///
/// This is the *variable text* shape, and so belongs on a `Tx` or `Ch` field
/// only. A `/Btn` gets [`format_button_da`] instead.
pub fn format_da(style: &TextStyle) -> String {
    format!(
        "{} {} {} rg /{} {} Tf",
        format_number(byte_to_channel(style.color.r)),
        format_number(byte_to_channel(style.color.g)),
        format_number(byte_to_channel(style.color.b)),
        resource_name(style.font),
        format_number(style.size_pt),
    )
}

/// Serializes a `/Btn` field's `/DA`, e.g. `"/ZaDb 0 Tf 1 0 0 rg"` (T-205).
///
/// Acrobat's own shape, and deliberately not [`format_da`]'s: a button's
/// appearance is a ZapfDingbats glyph (`appearance::glyph_stream`), so the
/// resource named here is `/ZaDb` — the font a downstream tool regenerating
/// that appearance has to find — and the size is `0`, "chosen by the
/// viewer", because the glyph is sized from the control's own rect and the
/// user's point size never reaches it.
///
/// Colour is therefore the only part of a button's `TextStyle` this carries,
/// which is also the only part anything consumes: `build_field_appearance`
/// paints the check mark and the radio dot with it.
pub fn format_button_da(color: Color) -> String {
    format!(
        "/{} 0 Tf {} {} {} rg",
        crate::appearance::ZAPF_DINGBATS_RESOURCE,
        format_number(byte_to_channel(color.r)),
        format_number(byte_to_channel(color.g)),
        format_number(byte_to_channel(color.b)),
    )
}

/// Parses a `/DA` string into a `TextStyle`, filling in decision 3's default
/// (Helvetica, 12pt, black) **per attribute** for whatever the string does
/// not state in terms this crate models.
///
/// Attribute-by-attribute rather than all-or-nothing (T-205): a `/DA` that
/// names a font outside this crate's three families still states its colour
/// plainly, and throwing the colour away with the font is how a red checkbox
/// — whose `/DA` says `/ZaDb`, which is never a `FontFamily` — used to read
/// back black.
pub fn parse_da(da: &str) -> TextStyle {
    let scanned = scan_da(da);
    let default = default_style();
    TextStyle {
        font: scanned.font.unwrap_or(default.font),
        // `0 Tf` means "size chosen by the viewer" (ISO 32000-1 12.7.3.3),
        // not a 0pt font — taken literally it would hand `appearance.rs` a
        // zero-height glyph to draw.
        size_pt: scanned
            .size_pt
            .filter(|size| *size > 0.0)
            .unwrap_or(default.size_pt),
        color: scanned.color.unwrap_or(default.color),
    }
}

/// What a `/DA` actually stated, attribute by attribute. Each is independent
/// so that one unmodelled operand cannot take the others down with it.
#[derive(Default)]
struct ScannedDa {
    color: Option<Color>,
    font: Option<FontFamily>,
    size_pt: Option<f64>,
}

fn scan_da(da: &str) -> ScannedDa {
    let tokens: Vec<&str> = da.split_whitespace().collect();
    let mut numbers: Vec<f64> = Vec::new();
    let mut scanned = ScannedDa::default();

    for (index, token) in tokens.iter().enumerate() {
        match *token {
            "rg" => {
                if numbers.len() >= 3 {
                    let b = numbers.pop().expect("length checked");
                    let g = numbers.pop().expect("length checked");
                    let r = numbers.pop().expect("length checked");
                    scanned.color = Some(Color {
                        r: channel_to_byte(r),
                        g: channel_to_byte(g),
                        b: channel_to_byte(b),
                    });
                }
                numbers.clear();
            }
            "g" => {
                if let Some(gray) = numbers.pop().map(channel_to_byte) {
                    scanned.color = Some(Color {
                        r: gray,
                        g: gray,
                        b: gray,
                    });
                }
                numbers.clear();
            }
            "Tf" => {
                scanned.size_pt = numbers.pop();
                numbers.clear();
                scanned.font = index
                    .checked_sub(2)
                    .and_then(|i| tokens.get(i))
                    .and_then(|t| t.strip_prefix('/'))
                    .and_then(family_from_resource_name);
            }
            other => {
                if let Ok(number) = other.parse::<f64>() {
                    numbers.push(number);
                }
            }
        }
    }

    scanned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_default_style_exactly_like_the_spec_example() {
        assert_eq!(format_da(&default_style()), "0 0 0 rg /Helv 12 Tf");
    }

    #[test]
    fn parses_the_spec_example_back_into_the_default_style() {
        assert_eq!(parse_da("0 0 0 rg /Helv 12 Tf"), default_style());
    }

    #[test]
    fn round_trips_a_non_default_style() {
        let style = TextStyle {
            font: FontFamily::Courier,
            size_pt: 10.5,
            color: Color {
                r: 200,
                g: 0,
                b: 128,
            },
        };
        let da = format_da(&style);
        assert_eq!(parse_da(&da), style);
    }

    #[test]
    fn round_trips_times_roman() {
        let style = TextStyle {
            font: FontFamily::TimesRoman,
            size_pt: 9.0,
            color: Color {
                r: 255,
                g: 255,
                b: 255,
            },
        };
        assert_eq!(parse_da(&format_da(&style)), style);
    }

    #[test]
    fn tolerates_font_operator_before_color_operator() {
        let expected = TextStyle {
            font: FontFamily::TimesRoman,
            size_pt: 8.0,
            color: Color { r: 255, g: 0, b: 0 },
        };
        assert_eq!(parse_da("/TiRo 8 Tf 1 0 0 rg"), expected);
    }

    #[test]
    fn parses_a_gray_color_operator() {
        let parsed = parse_da("0.5 g /Helv 12 Tf");
        assert_eq!(
            parsed.color,
            Color {
                r: 128,
                g: 128,
                b: 128
            }
        );
    }

    #[test]
    fn falls_back_to_default_on_garbage() {
        assert_eq!(parse_da("not a DA string at all"), default_style());
    }

    #[test]
    fn keeps_the_color_when_the_font_resource_is_not_one_of_ours() {
        // A custom embedded font (not one of this crate's Standard-14
        // resource names): decision 3 does not model it, so the family
        // defaults rather than guessing a substitute — but the color and
        // the size are stated plainly and survive on their own (T-205).
        let parsed = parse_da("1 0 0 rg /CustomFont1 8 Tf");
        assert_eq!(parsed.font, FontFamily::Helvetica);
        assert_eq!(parsed.size_pt, 8.0);
        assert_eq!(parsed.color, Color { r: 255, g: 0, b: 0 });
    }

    #[test]
    fn keeps_the_color_when_the_font_operator_is_missing() {
        let parsed = parse_da("1 0 0 rg");
        assert_eq!(parsed.font, FontFamily::Helvetica);
        assert_eq!(parsed.size_pt, 12.0);
        assert_eq!(parsed.color, Color { r: 255, g: 0, b: 0 });
    }

    #[test]
    fn falls_back_to_the_default_color_when_the_color_operator_is_missing() {
        assert_eq!(parse_da("/Helv 12 Tf"), default_style());
    }

    #[test]
    fn an_auto_sized_da_reads_back_at_the_default_size() {
        // `0 Tf` is "size chosen by the viewer" (ISO 32000-1 12.7.3.3), not
        // a 0pt font. Modeling it literally would hand `appearance.rs` a
        // zero-height glyph to draw.
        assert_eq!(parse_da("0 0 0 rg /Helv 0 Tf").size_pt, 12.0);
    }

    #[test]
    fn formats_a_buttons_da_the_way_acrobat_writes_one() {
        let red = Color { r: 255, g: 0, b: 0 };
        assert_eq!(format_button_da(red), "/ZaDb 0 Tf 1 0 0 rg");
    }

    #[test]
    fn a_buttons_da_round_trips_its_color() {
        let color = Color {
            r: 200,
            g: 16,
            b: 64,
        };
        let parsed = parse_da(&format_button_da(color));
        assert_eq!(parsed.color, color);
        assert_eq!(
            (parsed.font, parsed.size_pt),
            (FontFamily::Helvetica, 12.0),
            "a button's /DA states no family and no size of ours, so both default"
        );
    }

    #[test]
    fn base_font_name_matches_pdf_edits_afm_table_keys() {
        for family in [
            FontFamily::Helvetica,
            FontFamily::TimesRoman,
            FontFamily::Courier,
        ] {
            assert!(
                pdf_edit::encoding::tables::standard_14_ascii_widths(base_font_name(family))
                    .is_some(),
                "{family:?}'s base font name must be one pdf-edit's AFM table recognizes"
            );
        }
    }
}
