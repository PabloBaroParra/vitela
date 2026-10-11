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
//! `pdf_edit::encoding::winansi` — every font this crate ever draws with has
//! a full WinAnsi AFM table already in that crate.

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

/// One ZapfDingbats glyph: its character code and its ink box, in
/// thousandths of the font size, from Adobe's `ZapfDingbats.afm` `B` entry.
/// The ink box — not the advance width — is what has to sit centered in the
/// control, so the placement works from these and not from fixed fractions.
#[derive(Debug, Clone, Copy)]
struct GlyphMetrics {
    code: u8,
    llx: f64,
    lly: f64,
    urx: f64,
    ury: f64,
}

/// `a20`, the checkmark (Adobe/reportlab's own long-standing convention for
/// a default-style checkbox "on" appearance).
const CHECKMARK: GlyphMetrics = GlyphMetrics {
    code: 0x34,
    llx: 36.0,
    lly: -14.0,
    urx: 811.0,
    ury: 705.0,
};
/// The share of the control's limiting dimension the glyph's ink spans —
/// clear of the edges, close to what Acrobat paints.
const GLYPH_FILL: f64 = 0.7;
/// The radio dot's radius as a fraction of the button's smaller side — what
/// reportlab draws (3.6 in an 18pt button).
const RADIO_DOT_RADIUS: f64 = 0.2;

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
/// Fails only for `Text`/`Dropdown` content containing a character
/// WinAnsiEncoding cannot show (CJK, emoji, ...) — decision 3 does not embed
/// a font, so there is no fallback glyph source. Accented Latin-1 text
/// (`José`, `año`) is fine. This is the form-field analogue of `pdf-edit`'s
/// `EncodingGap`: reject before writing anything, never silently drop or
/// mis-render a character.
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
                on: glyph_stream(width, height, field.style.color, CHECKMARK),
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
        on: dot_stream(width, height, color),
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

/// Where a glyph's ink lands inside a control's box, in the box's own
/// bottom-up space (origin at the box's lower-left corner).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkBox {
    pub left: f64,
    pub bottom: f64,
    pub width: f64,
    pub height: f64,
}

/// The font size a glyph is drawn at, and the ink box that size produces:
/// the ink spans [`GLYPH_FILL`] of the box's limiting dimension and sits
/// centered on both axes. Sizing by the ink box keeps the glyph's aspect
/// ratio in a non-square control.
fn place_glyph(width: f64, height: f64, glyph: GlyphMetrics) -> (f64, InkBox) {
    let ink_width = (glyph.urx - glyph.llx) / 1000.0;
    let ink_height = (glyph.ury - glyph.lly) / 1000.0;
    let size = GLYPH_FILL * (width / ink_width).min(height / ink_height);
    let ink = InkBox {
        left: (width - ink_width * size) / 2.0,
        bottom: (height - ink_height * size) / 2.0,
        width: ink_width * size,
        height: ink_height * size,
    };
    (size, ink)
}

/// Where a checked checkbox's mark sits in a `width` × `height` box — the
/// same box the saved `/AP` paints its glyph in, for a shell that draws its
/// own live stand-in and must match it.
pub fn checkmark_ink_box(width: f64, height: f64) -> InkBox {
    place_glyph(width, height, CHECKMARK).1
}

/// A single ZapfDingbats glyph placed by [`place_glyph`]. The text origin is
/// offset by the glyph's `llx`/`lly` so the ink, not the origin, is centered.
fn glyph_stream(width: f64, height: f64, color: Color, glyph: GlyphMetrics) -> Stream {
    let (size, ink) = place_glyph(width, height, glyph);
    let x = ink.left - glyph.llx / 1000.0 * size;
    let y = ink.bottom - glyph.lly / 1000.0 * size;
    let content = format!(
        "q {r} {g} {b} rg BT /{font} {size} Tf {x} {y} Td {glyph} Tj ET Q",
        r = format_number(color.r as f64 / 255.0),
        g = format_number(color.g as f64 / 255.0),
        b = format_number(color.b as f64 / 255.0),
        font = ZAPF_DINGBATS_RESOURCE,
        size = format_number(size),
        x = format_number(x),
        y = format_number(y),
        glyph = literal_string_byte(glyph.code),
    );
    let mut dict = stream_dict(width, height);
    dict.set("Resources", zapf_dingbats_resources());
    Stream::new(dict, content.into_bytes())
}

/// The glyph stream's own `/Resources`: `/ZaDb` bound to ZapfDingbats. Not
/// left to `/AcroForm /DR` — a foreign form's need not define it (reportlab's
/// defines only `/Cour`), and pdfium then shows the raw glyph code (`4`, `l`)
/// in a fallback font. Same reasoning as [`text_font_resources`].
fn zapf_dingbats_resources() -> Dictionary {
    let mut font = Dictionary::new();
    font.set("Type", "Font");
    font.set("Subtype", "Type1");
    font.set("BaseFont", "ZapfDingbats");
    let mut fonts = Dictionary::new();
    fonts.set(ZAPF_DINGBATS_RESOURCE, Object::Dictionary(font));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    resources
}

/// The radio button's "on" mark: a filled circle centred in the button. A
/// path, not the ZapfDingbats `l` — the glyph's ink sits where that font's
/// metrics put it, which this crate does not model, so it came out more than
/// twice the area and off centre.
fn dot_stream(width: f64, height: f64, color: Color) -> Stream {
    let content = format!(
        "q
{r} {g} {b} rg
{path}f
Q",
        r = format_number(color.r as f64 / 255.0),
        g = format_number(color.g as f64 / 255.0),
        b = format_number(color.b as f64 / 255.0),
        path = crate::frame::circle_path(
            width / 2.0,
            height / 2.0,
            RADIO_DOT_RADIUS * width.min(height),
        ),
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

/// Encodes text as WinAnsi bytes and wraps them as a PDF literal string
/// operand for `Tj`. Bytes above 0x7E are octal-escaped so the stream stays
/// printable ASCII. Rejects any character WinAnsi cannot show — see
/// [`build_field_appearance`]'s doc for why.
fn literal_string_text(text: &str) -> Result<String, FormError> {
    let bytes = pdf_edit::encoding::winansi::encode_winansi(text).map_err(unencodable)?;
    let mut out = String::from("(");
    for byte in bytes {
        match byte {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7E => out.push(byte as char),
            other => out.push_str(&format!("\\{other:03o}")),
        }
    }
    out.push(')');
    Ok(out)
}

fn text_width_pt(text: &str, font: FontFamily, size_pt: f64) -> Result<f64, FormError> {
    let mut total_thousandths = 0u32;
    for ch in text.chars() {
        let width = pdf_edit::encoding::winansi::standard_14_char_width(base_font_name(font), ch)
            .ok_or_else(|| unencodable(ch))?;
        total_thousandths += width as u32;
    }
    Ok(total_thousandths as f64 / 1000.0 * size_pt)
}

fn unencodable(character: char) -> FormError {
    FormError::InvalidValue(format!(
        "'{character}' cannot be encoded in a Standard-14 font (WinAnsi only)"
    ))
}

/// The text stream's own `/Resources`: the field's font under the same name
/// its `/DA` uses, declared `/WinAnsiEncoding`. Without it the name resolves
/// against the form's `/DR`, and an existing form's `/DR` may define that
/// font with no `/Encoding` — which draws the 0xE9 byte as something other
/// than 'é'.
fn text_font_resources(font: FontFamily) -> Dictionary {
    let mut entry = Dictionary::new();
    entry.set("Type", "Font");
    entry.set("Subtype", "Type1");
    entry.set("BaseFont", base_font_name(font));
    entry.set("Encoding", "WinAnsiEncoding");
    let mut fonts = Dictionary::new();
    fonts.set(base_font_name_resource(font), Object::Dictionary(entry));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    resources
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
    pdf_edit::encoding::wrap::wrap_greedy(text, max_width_pt, |line| {
        text_width_pt(line, font, size_pt)
    })
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

    let mut dict = stream_dict(width, height);
    dict.set(
        "Resources",
        Object::Dictionary(text_font_resources(style.font)),
    );
    Ok(Stream::new(dict, body.into_bytes()))
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
    fn accented_text_is_written_as_octal_escaped_winansi() {
        let field = text_field(false, "José Ñandú", wide_rect());
        let FieldAppearance::Single(stream) = build_field_appearance(&field).expect("Latin-1")
        else {
            panic!("expected Single");
        };
        let content = String::from_utf8(stream.content).expect("stream stays ASCII");
        // é = 0xE9, Ñ = 0xD1, ú = 0xFA — single WinAnsi bytes, not UTF-8 pairs.
        assert!(
            content.contains("(Jos\\351 \\321and\\372) Tj"),
            "content: {content}"
        );
    }

    #[test]
    fn text_stream_carries_its_own_winansi_font() {
        // An existing form's `/DR` may define the font without an
        // `/Encoding`, which would draw 0xE9 as something other than 'é'.
        for (font, resource, base) in [
            (FontFamily::Helvetica, "Helv", "Helvetica"),
            (FontFamily::TimesRoman, "TiRo", "Times-Roman"),
            (FontFamily::Courier, "Cour", "Courier"),
        ] {
            let mut field = text_field(false, "año", wide_rect());
            field.style.font = font;
            let FieldAppearance::Single(stream) = build_field_appearance(&field).expect("valid")
            else {
                panic!("expected Single");
            };
            let fonts = stream
                .dict
                .get(b"Resources")
                .and_then(Object::as_dict)
                .and_then(|resources| resources.get(b"Font"))
                .and_then(Object::as_dict)
                .expect("own /Resources /Font");
            let entry = fonts
                .get(resource.as_bytes())
                .and_then(Object::as_dict)
                .unwrap_or_else(|_| panic!("/{resource} missing"));
            assert_eq!(entry.get(b"BaseFont").unwrap(), &Object::Name(base.into()));
            assert_eq!(
                entry.get(b"Encoding").unwrap(),
                &Object::Name(b"WinAnsiEncoding".to_vec())
            );
        }
    }

    #[test]
    fn multiline_accented_text_wraps_in_times() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 100.0,
        };
        let mut field = FormField {
            kind: FormFieldKind::Text {
                multiline: true,
                max_len: None,
            },
            ..text_field(true, "camión árbol canción éxito", rect)
        };
        field.style.font = FontFamily::TimesRoman;
        let FieldAppearance::Single(stream) = build_field_appearance(&field).expect("valid") else {
            panic!("expected Single");
        };
        let content = String::from_utf8(stream.content).unwrap();
        assert!(content.matches(" Tj").count() > 1, "content: {content}");
    }

    #[test]
    fn rejects_text_winansi_cannot_show() {
        let field = text_field(false, "東京", wide_rect());
        let result = build_field_appearance(&field);
        assert!(matches!(result, Err(FormError::InvalidValue(_))));
    }

    /// A foreign form's `/DR` need not define `/ZaDb` — reportlab's defines
    /// only `/Cour` — and pdfium then draws the glyph code (`4`) in a
    /// fallback font. The mark has to name its font itself, exactly as the
    /// text stream above does.
    #[test]
    fn the_checkmark_stream_carries_its_own_zapf_dingbats() {
        let FieldAppearance::Checkbox { on: checkmark, .. } =
            build_field_appearance(&checkbox_field(square(12.0))).expect("valid")
        else {
            panic!("expected Checkbox");
        };
        let entry = checkmark
            .dict
            .get(b"Resources")
            .and_then(Object::as_dict)
            .and_then(|resources| resources.get(b"Font"))
            .and_then(Object::as_dict)
            .and_then(|fonts| fonts.get(ZAPF_DINGBATS_RESOURCE.as_bytes()))
            .and_then(Object::as_dict)
            .expect("own /Resources /Font /ZaDb");
        assert_eq!(
            entry.get(b"BaseFont").unwrap(),
            &Object::Name(b"ZapfDingbats".to_vec())
        );
    }

    fn square(side: f64) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: side,
            height: side,
        }
    }

    fn radio_on_stream(rect: Rect) -> String {
        let radio = FormField {
            kind: FormFieldKind::RadioGroup {
                options: vec![RadioOption {
                    export_value: "a".into(),
                    rect,
                }],
            },
            style: TextStyle {
                color: Color { r: 255, g: 0, b: 0 },
                ..style()
            },
            ..checkbox_field(rect)
        };
        let FieldAppearance::Radio(buttons) = build_field_appearance(&radio).expect("valid") else {
            panic!("expected Radio");
        };
        String::from_utf8(buttons[0].on.content.clone()).unwrap()
    }

    /// The `x y` anchor points of every `m` and `c` in a path: for the
    /// four-arc circle these are its east, north, west and south extremes.
    fn anchor_extent(content: &str) -> (f64, f64, f64, f64) {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for line in content.lines() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            let anchor = match tokens.last() {
                Some(&"m") => tokens.get(0..2),
                Some(&"c") => tokens.get(4..6),
                _ => None,
            };
            if let Some([x, y]) = anchor {
                xs.push(x.parse::<f64>().unwrap());
                ys.push(y.parse::<f64>().unwrap());
            }
        }
        let min = |v: &[f64]| v.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = |v: &[f64]| v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (min(&xs), max(&xs), min(&ys), max(&ys))
    }

    /// A vector circle, not the ZapfDingbats `l`: a glyph's ink is placed by
    /// font metrics this crate does not model, so it landed big and off
    /// centre. Radius 0.2 of the smaller side — what reportlab draws (3.6 in
    /// an 18pt button) — centred in the button.
    #[test]
    fn the_radio_dot_is_a_circle_centred_in_its_button() {
        for (rect, centre, radius) in [
            (square(18.0), (9.0, 9.0), 3.6),
            (
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 30.0,
                    height: 10.0,
                },
                (15.0, 5.0),
                2.0,
            ),
        ] {
            let content = radio_on_stream(rect);
            assert!(!content.contains("Tj"), "no glyph: {content}");
            assert!(
                content.contains("1 0 0 rg"),
                "painted in the field colour: {content}"
            );
            assert!(content.trim_end().ends_with("f\nQ"), "filled: {content}");
            let (x0, x1, y0, y1) = anchor_extent(&content);
            let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
            assert!(
                near(x0, centre.0 - radius) && near(x1, centre.0 + radius),
                "{content}"
            );
            assert!(
                near(y0, centre.1 - radius) && near(y1, centre.1 + radius),
                "{content}"
            );
        }
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

    /// The glyph's ink box in the stream's own space, read back from its
    /// `<size> Tf <x> <y> Td` operands and the glyph's AFM box.
    fn ink_box(stream: &Stream, metrics: GlyphMetrics) -> (f64, f64, f64, f64) {
        let content = String::from_utf8(stream.content.clone()).unwrap();
        let tokens: Vec<&str> = content.split_whitespace().collect();
        let number = |at: usize| tokens[at].parse::<f64>().unwrap();
        let tf = tokens.iter().position(|token| *token == "Tf").unwrap();
        let (size, x, y) = (number(tf - 1), number(tf + 1), number(tf + 2));
        let scale = size / 1000.0;
        (
            x + metrics.llx * scale,
            y + metrics.lly * scale,
            x + metrics.urx * scale,
            y + metrics.ury * scale,
        )
    }

    fn assert_centered_inside(stream: &Stream, metrics: GlyphMetrics, width: f64, height: f64) {
        let (left, bottom, right, top) = ink_box(stream, metrics);
        assert!(
            left >= 0.0 && bottom >= 0.0,
            "ink spills out: {left} {bottom}"
        );
        assert!(
            right <= width && top <= height,
            "ink spills out: {right} {top}"
        );
        assert!(
            ((left + right) / 2.0 - width / 2.0).abs() < 0.01,
            "x off-center"
        );
        assert!(
            ((bottom + top) / 2.0 - height / 2.0).abs() < 0.01,
            "y off-center"
        );
        let fill = ((right - left) / width).max((top - bottom) / height);
        assert!((fill - GLYPH_FILL).abs() < 0.01, "fill {fill}");
    }

    #[test]
    fn checkmark_is_centered_and_sized_in_any_box() {
        for (width, height) in [(12.0, 12.0), (40.0, 12.0), (12.0, 40.0), (8.5, 30.0)] {
            let field = checkbox_field(Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            });
            let FieldAppearance::Checkbox { on, .. } = build_field_appearance(&field).unwrap()
            else {
                panic!("expected Checkbox");
            };
            assert_centered_inside(&on, CHECKMARK, width, height);
        }
    }

    /// The box a shell's live stand-in draws in is the box the saved glyph
    /// actually inks, or the mark jumps the moment the field is saved.
    #[test]
    fn checkmark_ink_box_is_where_the_saved_glyph_inks() {
        let (width, height) = (40.0, 12.0);
        let field = checkbox_field(Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        });
        let FieldAppearance::Checkbox { on, .. } = build_field_appearance(&field).unwrap() else {
            panic!("expected Checkbox");
        };
        let (left, bottom, right, top) = ink_box(&on, CHECKMARK);
        let ink = checkmark_ink_box(width, height);
        assert!((ink.left - left).abs() < 0.01 && (ink.bottom - bottom).abs() < 0.01);
        assert!((ink.width - (right - left)).abs() < 0.01);
        assert!((ink.height - (top - bottom)).abs() < 0.01);
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
