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
//! fallback.

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

/// Parses a `/DA` string into a `TextStyle`, falling back to Helvetica/12pt/
/// black (decision 3's default) whenever the string does not contain both a
/// recognized color operator and a `Tf` naming one of this crate's own
/// Standard-14 resource names.
pub fn parse_da(da: &str) -> TextStyle {
    try_parse_da(da).unwrap_or_else(default_style)
}

fn try_parse_da(da: &str) -> Option<TextStyle> {
    let tokens: Vec<&str> = da.split_whitespace().collect();
    let mut numbers: Vec<f64> = Vec::new();
    let mut color: Option<Color> = None;
    let mut font: Option<FontFamily> = None;
    let mut size_pt: Option<f64> = None;

    for (index, token) in tokens.iter().enumerate() {
        match *token {
            "rg" => {
                if numbers.len() < 3 {
                    return None;
                }
                let b = numbers.pop()?;
                let g = numbers.pop()?;
                let r = numbers.pop()?;
                color = Some(Color {
                    r: channel_to_byte(r),
                    g: channel_to_byte(g),
                    b: channel_to_byte(b),
                });
                numbers.clear();
            }
            "g" => {
                let gray = channel_to_byte(numbers.pop()?);
                color = Some(Color {
                    r: gray,
                    g: gray,
                    b: gray,
                });
                numbers.clear();
            }
            "Tf" => {
                size_pt = numbers.pop();
                numbers.clear();
                font = index
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

    Some(TextStyle {
        font: font?,
        size_pt: size_pt?,
        color: color?,
    })
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
    fn falls_back_to_default_on_unrecognized_font_resource() {
        // A custom embedded font (not one of this crate's Standard-14
        // resource names) — decision 3 does not model it, so the whole
        // style defaults rather than guessing a substitute font.
        assert_eq!(parse_da("0 0 0 rg /CustomFont1 12 Tf"), default_style());
    }

    #[test]
    fn falls_back_to_default_when_the_font_operator_is_missing() {
        assert_eq!(parse_da("0 0 0 rg"), default_style());
    }

    #[test]
    fn falls_back_to_default_when_the_color_operator_is_missing() {
        assert_eq!(parse_da("/Helv 12 Tf"), default_style());
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
