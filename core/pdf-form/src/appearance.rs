//! Builds `/AP` appearance content streams per field kind (T-136): text
//! (with clipping and greedy word-wrap for multiline), checkbox (two named
//! states drawn with a ZapfDingbats glyph), radio group (one two-state pair
//! per kid widget), and dropdown (shows the selected value).
//!
//! Every stream's content is local to the field's own box — coordinates
//! start at `(0, 0)` in the bottom-left corner, matching the `/BBox` the
//! stream's own dict carries, so placing the field on the page is entirely
//! the widget's `/Rect` and `/Matrix` (identity here). No `Object::Reference`
//! placeholders are needed the way `pdf-annotate::appearance` needs one for
//! `/SMask`: nothing in a field's own appearance stream refers to another
//! object.
//!
//! Decision 2 ("Generar `/AP` siempre") means every field gets one of these,
//! never relying on `/NeedAppearances`. Decision 3 (Standard-14 only, no
//! embedded fonts) is why word-wrap measurement can reuse
//! `pdf_edit::encoding::tables` — every font this crate ever draws with has
//! a full ASCII AFM table already in that crate.

use crate::da::{base_font_name, format_number};
use crate::error::FormError;
use lopdf::{Dictionary, Object, Stream};
use pdf_document::{Color, FieldValue, FontFamily, FormField, FormFieldKind, RadioOption};

/// The `/DR /Font` resource name for the ZapfDingbats glyphs checkbox and
/// radio "on" appearances draw with — Standard-14, so it needs no embedding,
/// but it is never one of this crate's own `FontFamily` choices (a user
/// cannot select "ZapfDingbats" as a field's style).
pub const ZAPF_DINGBATS_RESOURCE: &str = "ZaDb";

/// The `/AP /N` state name for a checked checkbox, fixed for every checkbox
/// this crate creates (matches the ficha's own example dictionary shape).
pub const CHECKBOX_ON_STATE: &str = "Yes";

/// ZapfDingbats character code for a checkmark (Adobe/reportlab's own
/// long-standing convention for a default-style checkbox "on" appearance).
const CHECKMARK_GLYPH: u8 = 0x34;
/// ZapfDingbats character code for a solid circle (the matching convention
/// for a default-style radio button "on" appearance).
const CIRCLE_GLYPH: u8 = 0x6C;

/// Padding, in points, kept between a text field's own box edge and its
/// text — matches the small constant Acrobat's own generated appearances
/// use.
const TEXT_PADDING_PT: f64 = 2.0;

/// A field's built `/AP` content, shaped per kind — `pdf-save` (T-138)
/// assigns indirect object ids to the streams inside and links them from
/// the widget's `/AP` dictionary; this module never opens or numbers a
/// `lopdf::Document`.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldAppearance {
    /// A `Text` or `Dropdown` field's single `/AP /N` stream.
    Single(Stream),
    Checkbox {
        on_state: &'static str,
        on: Stream,
        off: Stream,
    },
    /// One `(export_value, on, off)` triple per kid widget, in the same
    /// order as the field's `RadioGroup` options.
    Radio(Vec<RadioButtonAppearance>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadioButtonAppearance {
    pub export_value: String,
    pub on: Stream,
    pub off: Stream,
}

/// Builds the appearance for `field`, dispatching on its `FormFieldKind`.
///
/// Fails only for `Text`/`Dropdown` content containing a character outside
/// printable ASCII (32-126) — the only range `pdf_edit`'s AFM tables cover
/// (decision 3 does not embed a font, so there is no fallback glyph source).
/// This is the form-field analogue of `pdf-edit`'s `EncodingGap`: reject
/// before writing anything, never silently drop or mis-render a character.
pub fn build_field_appearance(field: &FormField) -> Result<FieldAppearance, FormError> {
    match &field.kind {
        FormFieldKind::Text { multiline, .. } => Ok(FieldAppearance::Single(build_text_stream(
            field, *multiline,
        )?)),
        FormFieldKind::Dropdown { .. } => {
            Ok(FieldAppearance::Single(build_text_stream(field, false)?))
        }
        FormFieldKind::Checkbox => {
            let (width, height) = (field.rect.width, field.rect.height);
            Ok(FieldAppearance::Checkbox {
                on_state: CHECKBOX_ON_STATE,
                on: glyph_stream(width, height, field.style.color, CHECKMARK_GLYPH),
                off: empty_stream(width, height),
            })
        }
        FormFieldKind::RadioGroup { options } => Ok(FieldAppearance::Radio(
            options
                .iter()
                .map(|option| build_radio_button(option, field.style.color))
                .collect(),
        )),
        _ => Err(FormError::UnsupportedOperation(
            "build_field_appearance: unknown field kind",
        )),
    }
}

fn build_radio_button(option: &RadioOption, color: Color) -> RadioButtonAppearance {
    let (width, height) = (option.rect.width, option.rect.height);
    RadioButtonAppearance {
        export_value: option.export_value.clone(),
        on: glyph_stream(width, height, color, CIRCLE_GLYPH),
        off: empty_stream(width, height),
    }
}

fn stream_dict(width: f64, height: f64) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", "XObject");
    dict.set("Subtype", "Form");
    dict.set("FormType", 1);
    dict.set(
        "BBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(width as f32),
            Object::Real(height as f32),
        ]),
    );
    dict
}

/// The "Off" appearance for a checkbox or radio button: no `/MK` border or
/// background is modeled (out of scope, see `docs/batch-forms.md`), so an
/// unset control simply paints nothing.
fn empty_stream(width: f64, height: f64) -> Stream {
    Stream::new(stream_dict(width, height), Vec::new())
}

/// A single ZapfDingbats glyph, sized to fit the smaller box dimension and
/// centered by a fixed fraction of the box — not a font-metrics-exact
/// centering (this crate does not model ZapfDingbats' own glyph metrics),
/// close enough that the mark reads as inside the control.
fn glyph_stream(width: f64, height: f64, color: Color, glyph: u8) -> Stream {
    let size = 0.8 * width.min(height);
    let x = 0.1 * width;
    let y = 0.15 * height;
    let content = format!(
        "q {r} {g} {b} rg BT /{font} {size} Tf {x} {y} Td {glyph} Tj ET Q",
        r = format_number(color.r as f64 / 255.0),
        g = format_number(color.g as f64 / 255.0),
        b = format_number(color.b as f64 / 255.0),
        font = ZAPF_DINGBATS_RESOURCE,
        size = format_number(size),
        x = format_number(x),
        y = format_number(y),
        glyph = literal_string_byte(glyph),
    );
    Stream::new(stream_dict(width, height), content.into_bytes())
}

/// Wraps one byte as a PDF literal string operand, escaping it if it is a
/// literal-string metacharacter — the ZapfDingbats codes this module uses
/// never are, but the escape is cheap and correct for any byte.
fn literal_string_byte(byte: u8) -> String {
    match byte {
        b'(' | b')' | b'\\' => format!("(\\{})", byte as char),
        0x20..=0x7E => format!("({})", byte as char),
        other => format!("(\\{other:03o})"),
    }
}

/// Escapes ASCII text as a PDF literal string operand for `Tj`. Rejects any
/// character outside 32-126 — see [`build_field_appearance`]'s doc for why.
fn literal_string_text(text: &str) -> Result<String, FormError> {
    let mut out = String::from("(");
    for ch in text.chars() {
        match ch {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            ' '..='~' => out.push(ch),
            other => return Err(FormError::InvalidValue(format!(
                "'{other}' cannot be encoded in a Standard-14 font (only printable ASCII is supported)"
            ))),
        }
    }
    out.push(')');
    Ok(out)
}

fn text_width_pt(text: &str, font: FontFamily, size_pt: f64) -> Result<f64, FormError> {
    let widths = pdf_edit::encoding::tables::standard_14_ascii_widths(base_font_name(font))
        .expect("every FontFamily maps to a name pdf-edit's AFM table recognizes");
    let mut total_thousandths = 0u32;
    for ch in text.chars() {
        let code = ch as u32;
        if !(0x20..=0x7E).contains(&code) {
            return Err(FormError::InvalidValue(format!(
                "'{ch}' cannot be encoded in a Standard-14 font (only printable ASCII is supported)"
            )));
        }
        total_thousandths += widths[(code - 0x20) as usize] as u32;
    }
    Ok(total_thousandths as f64 / 1000.0 * size_pt)
}

/// Greedily wraps `text` to lines no wider than `max_width_pt`, one
/// paragraph per `\n` in the input (so an explicit line break the user
/// typed is always honored, not just wrapped-around overflow). A single
/// word wider than `max_width_pt` on its own gets its own line rather than
/// being split mid-word (no hyphenation in v1).
fn wrap_lines(
    text: &str,
    font: FontFamily,
    size_pt: f64,
    max_width_pt: f64,
) -> Result<Vec<String>, FormError> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split(' ').filter(|w| !w.is_empty()) {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty() || text_width_pt(&candidate, font, size_pt)? <= max_width_pt {
                current = candidate;
            } else {
                lines.push(std::mem::take(&mut current));
                current = word.to_string();
            }
        }
        lines.push(current);
    }
    Ok(lines)
}

fn text_value(field: &FormField) -> String {
    match &field.value {
        FieldValue::Text(text) => text.clone(),
        FieldValue::Choice(Some(chosen)) => chosen.clone(),
        FieldValue::Choice(None) => String::new(),
        FieldValue::Checked(_) => String::new(),
    }
}

fn build_text_stream(field: &FormField, multiline: bool) -> Result<Stream, FormError> {
    let (width, height) = (field.rect.width, field.rect.height);
    let value = text_value(field);
    let max_width = (width - 2.0 * TEXT_PADDING_PT).max(0.0);
    let style = field.style;

    let lines = if multiline {
        wrap_lines(&value, style.font, style.size_pt, max_width)?
    } else {
        vec![value.replace(['\n', '\r'], " ")]
    };

    let mut body = format!(
        "/Tx BMC\nq\n0 0 {w} {h} re W n\nBT\n/{font} {size} Tf\n{r} {g} {b} rg\n",
        w = format_number(width),
        h = format_number(height),
        font = base_font_name_resource(style.font),
        size = format_number(style.size_pt),
        r = format_number(style.color.r as f64 / 255.0),
        g = format_number(style.color.g as f64 / 255.0),
        b = format_number(style.color.b as f64 / 255.0),
    );

    let leading = style.size_pt * 1.15;
    let first_baseline = (height - TEXT_PADDING_PT - style.size_pt).max(TEXT_PADDING_PT);
    body.push_str(&format!(
        "{x} {y} Td\n",
        x = format_number(TEXT_PADDING_PT),
        y = format_number(first_baseline),
    ));
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            body.push_str(&format!("0 {} Td\n", format_number(-leading)));
        }
        body.push_str(&literal_string_text(line)?);
        body.push_str(" Tj\n");
    }
    body.push_str("ET\nQ\nEMC");

    Ok(Stream::new(stream_dict(width, height), body.into_bytes()))
}

fn base_font_name_resource(font: FontFamily) -> &'static str {
    crate::da::resource_name(font)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{FieldOrigin, FormFieldId, PageId, Rect, TextStyle};

    fn style() -> TextStyle {
        TextStyle {
            font: FontFamily::Helvetica,
            size_pt: 10.0,
            color: Color { r: 0, g: 0, b: 0 },
        }
    }

    fn text_field(multiline: bool, value: &str, rect: Rect) -> FormField {
        FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Text_1".to_string(),
            rect,
            style: style(),
            value: FieldValue::Text(value.to_string()),
            kind: FormFieldKind::Text {
                multiline,
                max_len: None,
            },
            origin: FieldOrigin::New,
        }
    }

    fn checkbox_field(rect: Rect) -> FormField {
        FormField {
            id: FormFieldId(2),
            page: PageId(0),
            name: "Checkbox_1".to_string(),
            rect,
            style: style(),
            value: FieldValue::Checked(false),
            kind: FormFieldKind::Checkbox,
            origin: FieldOrigin::New,
        }
    }

    fn wide_rect() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 20.0,
        }
    }

    #[test]
    fn text_stream_contains_the_escaped_value() {
        let field = text_field(false, "Hello", wide_rect());
        let appearance = build_field_appearance(&field).expect("plain ASCII should build");
        let FieldAppearance::Single(stream) = appearance else {
            panic!("expected Single");
        };
        let content = String::from_utf8(stream.content).unwrap();
        assert!(content.contains("(Hello) Tj"));
        assert!(content.contains("/Tx BMC"));
        assert!(content.contains("EMC"));
    }

    #[test]
    fn text_stream_clips_to_the_field_rect() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 123.0,
            height: 45.0,
        };
        let field = text_field(false, "x", rect);
        let FieldAppearance::Single(stream) = build_field_appearance(&field).expect("valid") else {
            panic!("expected Single");
        };
        let content = String::from_utf8(stream.content).unwrap();
        assert!(content.contains("0 0 123 45 re W n"));
        assert_eq!(
            stream.dict.get(b"BBox").unwrap(),
            &Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(123.0),
                Object::Real(45.0),
            ])
        );
    }

    #[test]
    fn multiline_text_wraps_across_the_available_width() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 100.0,
        };
        let field = FormField {
            kind: FormFieldKind::Text {
                multiline: true,
                max_len: None,
            },
            ..text_field(true, "one two three four five", rect)
        };
        let FieldAppearance::Single(stream) = build_field_appearance(&field).expect("valid") else {
            panic!("expected Single");
        };
        let content = String::from_utf8(stream.content).unwrap();
        // A narrow box at 10pt Helvetica cannot fit all five words on one
        // line, so wrapping must have inserted more than one Tj operand.
        assert!(content.matches(" Tj").count() > 1, "content: {content}");
    }

    #[test]
    fn multiline_text_honors_explicit_newlines() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };
        let field = FormField {
            kind: FormFieldKind::Text {
                multiline: true,
                max_len: None,
            },
            ..text_field(true, "first\nsecond", rect)
        };
        let FieldAppearance::Single(stream) = build_field_appearance(&field).expect("valid") else {
            panic!("expected Single");
        };
        let content = String::from_utf8(stream.content).unwrap();
        assert!(content.contains("(first) Tj"));
        assert!(content.contains("(second) Tj"));
    }

    #[test]
    fn single_line_field_ignores_embedded_newlines_as_line_breaks() {
        let field = text_field(false, "one\ntwo", wide_rect());
        let FieldAppearance::Single(stream) = build_field_appearance(&field).expect("valid") else {
            panic!("expected Single");
        };
        let content = String::from_utf8(stream.content).unwrap();
        assert_eq!(content.matches(" Tj").count(), 1);
        assert!(content.contains("(one two) Tj"));
    }

    #[test]
    fn rejects_text_outside_printable_ascii() {
        let field = text_field(false, "café", wide_rect());
        let result = build_field_appearance(&field);
        assert!(matches!(result, Err(FormError::InvalidValue(_))));
    }

    #[test]
    fn checkbox_builds_two_named_states() {
        let field = checkbox_field(Rect {
            x: 0.0,
            y: 0.0,
            width: 12.0,
            height: 12.0,
        });
        let appearance = build_field_appearance(&field).expect("valid");
        let FieldAppearance::Checkbox { on_state, on, off } = appearance else {
            panic!("expected Checkbox");
        };
        assert_eq!(on_state, "Yes");
        assert!(!on.content.is_empty());
        assert!(off.content.is_empty());
    }

    #[test]
    fn radio_group_builds_one_pair_per_option() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 12.0,
            height: 12.0,
        };
        let field = FormField {
            id: FormFieldId(3),
            page: PageId(0),
            name: "Radio_1".to_string(),
            rect,
            style: style(),
            value: FieldValue::Choice(None),
            kind: FormFieldKind::RadioGroup {
                options: vec![
                    RadioOption {
                        export_value: "Yes".to_string(),
                        rect,
                    },
                    RadioOption {
                        export_value: "No".to_string(),
                        rect,
                    },
                ],
            },
            origin: FieldOrigin::New,
        };
        let appearance = build_field_appearance(&field).expect("valid");
        let FieldAppearance::Radio(buttons) = appearance else {
            panic!("expected Radio");
        };
        assert_eq!(buttons.len(), 2);
        assert_eq!(buttons[0].export_value, "Yes");
        assert_eq!(buttons[1].export_value, "No");
        assert!(!buttons[0].on.content.is_empty());
        assert!(buttons[0].off.content.is_empty());
    }

    #[test]
    fn dropdown_shows_the_selected_value() {
        let field = FormField {
            id: FormFieldId(4),
            page: PageId(0),
            name: "Dropdown_1".to_string(),
            rect: wide_rect(),
            style: style(),
            value: FieldValue::Choice(Some("Chosen".to_string())),
            kind: FormFieldKind::Dropdown {
                options: vec!["Chosen".to_string(), "Other".to_string()],
                editable: false,
            },
            origin: FieldOrigin::New,
        };
        let FieldAppearance::Single(stream) = build_field_appearance(&field).expect("valid") else {
            panic!("expected Single");
        };
        let content = String::from_utf8(stream.content).unwrap();
        assert!(content.contains("(Chosen) Tj"));
    }
}
