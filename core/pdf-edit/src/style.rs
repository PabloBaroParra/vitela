//! How a text run looks on the page — its font, size, fill colour and
//! spacing — for a shell that draws its own editor over the run and wants
//! the typed text to look like the text it replaces.
//!
//! Read-only, and built from what the parse already records rather than a
//! second pass over the stream: the size and matrices come from
//! [`crate::parse::TextPlacement`], the colour and spacing from the
//! operators in [`crate::parse::LocatedTextRun::paint_state`], and the font
//! from the resources the run's scope resolved its name against.
//!
//! Every length is in **page space** (points, as the run's box is): a run
//! drawn at `9 Tf` under a `2 0 0 2 cm` reports an 18 pt size, because that
//! is the size a reader sees and the size an overlay has to draw at.

use lopdf::{Dictionary, Document, Object, ObjectId};
use pdf_document::TextRun;

use crate::edit::{locate_run_for_style, scope_resources};
use crate::error::EditError;
use crate::parse::{decode_stream, tokenize, LocatedTextRun, Operand};

/// The largest font program [`text_run_font_program`] decodes. A real
/// embedded font, even a full CJK one, is far below it; a stream that
/// inflates past it is not a font anybody draws with.
const MAX_FONT_PROGRAM_BYTES: usize = 32 * 1024 * 1024;

/// What a run looks like, in page space.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRunStyle {
    /// The font's `/BaseFont`, raw — subset tag and style suffix included
    /// (`ABCDEF+Vera-Bold`). Empty when the font names none.
    pub base_font: String,
    /// The size a reader sees: `Tf` scaled by the text and current
    /// transformation matrices' vertical stretch.
    pub font_size: f64,
    /// The fill colour as RGB in `0.0..=1.0`. Gray and CMYK are converted
    /// naively; a colour space this cannot read (a pattern, a separation)
    /// reads as black, the initial fill colour.
    pub fill_rgb: [f64; 3],
    /// `Tc`, in page-space points: the gap added after every glyph.
    pub char_spacing: f64,
    /// `Tz` as a factor: `1.0` is unscaled.
    pub horizontal_scale: f64,
}

/// The kind of program a font embeds, which says what a font loader has to
/// be told it is reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontProgramFormat {
    /// `/FontFile2`: a TrueType program.
    TrueType,
    /// `/FontFile3` with `/Subtype /Type1C` or `/CIDFontType0C`: a bare
    /// CFF table.
    Cff,
    /// `/FontFile3` with `/Subtype /OpenType`: a whole OpenType file.
    OpenType,
    /// `/FontFile`: a Type 1 program, cleartext and encrypted portions
    /// as the PDF stores them.
    Type1,
}

/// A font program embedded in the file, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontProgram {
    pub format: FontProgramFormat,
    pub bytes: Vec<u8>,
}

/// How `run` looks where it is painted on the page `page_object`.
///
/// `run` is matched the way an edit matches it, and — because a run with a
/// retype still pending shows text the file does not hold yet — failing
/// that, by its id and font, which a retype leaves alone. A run that only
/// exists because of a pending insertion is not in the file at all, and is
/// refused with the error the edits use.
pub fn text_run_style(
    document: &Document,
    page_object: ObjectId,
    run: &TextRun,
) -> Result<TextRunStyle, EditError> {
    let located = locate_run_for_style(document, page_object, run)?;
    let resources = scope_resources(document, page_object, &located.form_path);
    let base_font = font_dictionary(document, &resources, &run.resource_font_name)
        .and_then(|font| name_of(document, &font, b"BaseFont"))
        .unwrap_or_default();
    Ok(style_of(&located, base_font))
}

/// The program embedded for `run`'s font, or `None` for a font that embeds
/// none — a standard-14 font, or one left to the reader's system — or whose
/// program is in a form this does not read (a Type 3 font).
pub fn text_run_font_program(
    document: &Document,
    page_object: ObjectId,
    run: &TextRun,
) -> Result<Option<FontProgram>, EditError> {
    let located = locate_run_for_style(document, page_object, run)?;
    let resources = scope_resources(document, page_object, &located.form_path);
    let Some(font) = font_dictionary(document, &resources, &run.resource_font_name) else {
        return Ok(None);
    };
    let Some(descriptor) = font_descriptor(document, &font) else {
        return Ok(None);
    };
    let entries: [(&[u8], Option<FontProgramFormat>); 3] = [
        (b"FontFile2", Some(FontProgramFormat::TrueType)),
        (b"FontFile", Some(FontProgramFormat::Type1)),
        // Its format is in the stream's own `/Subtype`.
        (b"FontFile3", None),
    ];
    for (key, format) in entries {
        let Ok(Object::Reference(id)) = descriptor.get(key) else {
            continue;
        };
        let Ok(stream) = document.get_object(*id).and_then(Object::as_stream) else {
            continue;
        };
        let format = match format {
            Some(format) => format,
            None => match name_of(document, &stream.dict, b"Subtype").as_deref() {
                Some("Type1C" | "CIDFontType0C") => FontProgramFormat::Cff,
                Some("OpenType") => FontProgramFormat::OpenType,
                _ => continue,
            },
        };
        let decoded = decode_stream(document, stream, *id, MAX_FONT_PROGRAM_BYTES)?;
        return Ok(Some(FontProgram {
            format,
            bytes: decoded.bytes,
        }));
    }
    Ok(None)
}

/// The style a located run was painted with.
fn style_of(located: &LocatedTextRun, base_font: String) -> TextRunStyle {
    let placement = &located.placement;
    let transform = placement.text_matrix.then(placement.ctm);
    // How far one text-space unit up, and one across, reach on the page.
    let vertical = transform.c.hypot(transform.d);
    let horizontal = transform.a.hypot(transform.b);
    let paint = PaintState::read(&located.paint_state);
    TextRunStyle {
        base_font,
        font_size: placement.font_size.abs() * vertical,
        fill_rgb: paint.fill_rgb,
        char_spacing: paint.char_spacing * paint.horizontal_scale * horizontal,
        horizontal_scale: paint.horizontal_scale,
    }
}

/// What a run's `paint_state` operators set.
struct PaintState {
    fill_rgb: [f64; 3],
    char_spacing: f64,
    horizontal_scale: f64,
}

impl PaintState {
    /// Reads the operators back. They are this crate's own output — one
    /// optional `cs`, one colour operator, then the text state — so a
    /// tokenizer failure can only mean an empty or unusual state, which
    /// reads as the initial one.
    fn read(paint_state: &[u8]) -> Self {
        let mut state = PaintState {
            fill_rgb: [0.0; 3],
            char_spacing: 0.0,
            horizontal_scale: 1.0,
        };
        let Ok(operations) = tokenize(paint_state) else {
            return state;
        };
        // The components a `cs` space takes, which is how `sc`/`scn` are
        // read: by the device space they were set in.
        let mut space_components = None;
        for operation in operations {
            let numbers: Vec<f64> = operation
                .operands
                .iter()
                .filter_map(Operand::as_f64)
                .collect();
            match operation.operator.as_str() {
                "g" | "rg" | "k" => {
                    if let Some(rgb) = device_rgb(&numbers) {
                        state.fill_rgb = rgb;
                    }
                }
                "cs" => {
                    space_components = match operation.operands.first() {
                        Some(Operand::Name(name)) => match name.as_str() {
                            "DeviceGray" | "CalGray" | "G" => Some(1),
                            "DeviceRGB" | "CalRGB" | "RGB" => Some(3),
                            "DeviceCMYK" | "CMYK" => Some(4),
                            _ => None,
                        },
                        _ => None,
                    };
                    // A new space starts at its initial colour: black.
                    state.fill_rgb = [0.0; 3];
                }
                "sc" | "scn" => {
                    if space_components == Some(numbers.len()) {
                        if let Some(rgb) = device_rgb(&numbers) {
                            state.fill_rgb = rgb;
                        }
                    }
                }
                "Tc" => state.char_spacing = numbers.first().copied().unwrap_or(0.0),
                "Tz" => state.horizontal_scale = numbers.first().copied().unwrap_or(100.0) / 100.0,
                _ => {}
            }
        }
        state
    }
}

/// A gray, RGB or CMYK colour's components as RGB, by how many there are.
fn device_rgb(components: &[f64]) -> Option<[f64; 3]> {
    let clamp = |value: f64| value.clamp(0.0, 1.0);
    match *components {
        [gray] => Some([clamp(gray); 3]),
        [r, g, b] => Some([clamp(r), clamp(g), clamp(b)]),
        [c, m, y, k] => {
            let k = clamp(k);
            Some([
                (1.0 - clamp(c)) * (1.0 - k),
                (1.0 - clamp(m)) * (1.0 - k),
                (1.0 - clamp(y)) * (1.0 - k),
            ])
        }
        _ => None,
    }
}

/// The font dictionary `resource_name` names in `resources`.
fn font_dictionary(
    document: &Document,
    resources: &Dictionary,
    resource_name: &str,
) -> Option<Dictionary> {
    let fonts = dictionary_at(document, resources.get(b"Font").ok()?)?;
    dictionary_at(document, fonts.get(resource_name.as_bytes()).ok()?)
}

/// A font's descriptor: its own, or its first descendant's for a composite
/// font, which is where a Type0 font keeps its program.
fn font_descriptor(document: &Document, font: &Dictionary) -> Option<Dictionary> {
    if let Some(descriptor) = font
        .get(b"FontDescriptor")
        .ok()
        .and_then(|object| dictionary_at(document, object))
    {
        return Some(descriptor);
    }
    let descendants = match font.get(b"DescendantFonts").ok()? {
        Object::Reference(id) => document.get_object(*id).ok()?.as_array().ok()?.clone(),
        Object::Array(items) => items.clone(),
        _ => return None,
    };
    let descendant = dictionary_at(document, descendants.first()?)?;
    dictionary_at(document, descendant.get(b"FontDescriptor").ok()?)
}

fn dictionary_at(document: &Document, object: &Object) -> Option<Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict.clone()),
        Object::Reference(id) => document.get_dictionary(*id).ok().cloned(),
        _ => None,
    }
}

fn name_of(document: &Document, dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    let object = match dictionary.get(key).ok()? {
        Object::Reference(id) => document.get_object(*id).ok()?,
        direct => direct,
    };
    object
        .as_name()
        .ok()
        .map(|name| String::from_utf8_lossy(name).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{document_with_content, helvetica_resources};
    use crate::parse::read_page_object_content;
    use pdf_document::PageId;

    fn first_run(document: &Document, page: ObjectId) -> TextRun {
        read_page_object_content(document, page, PageId(0))
            .expect("the page parses")
            .text_runs
            .remove(0)
    }

    fn style(content: &[u8]) -> TextRunStyle {
        let (document, page) = document_with_content(content, helvetica_resources());
        let run = first_run(&document, page);
        text_run_style(&document, page, &run).expect("the run has a style")
    }

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-6
    }

    /// The committed reportlab quote: its spec paragraph is Vera 9 in a
    /// dark blue with 0.3 pt of character spacing, from an embedded subset.
    fn quote_spec_run() -> (Document, ObjectId, TextRun) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/content-edit/reportlab_quote_paragraph.pdf");
        let document = Document::load(path).expect("committed fixture loads");
        let page = *document.get_pages().values().next().expect("one page");
        let run = read_page_object_content(&document, page, PageId(0))
            .expect("the page parses")
            .text_runs
            .into_iter()
            .find(|run| run.text.starts_with("COMPOSICI"))
            .expect("the spec paragraph's first line");
        (document, page, run)
    }

    #[test]
    fn the_quote_paragraph_reads_its_size_colour_and_spacing() {
        let (document, page, run) = quote_spec_run();

        let style = text_run_style(&document, page, &run).expect("a style");

        assert!(style.base_font.contains('+'), "{:?}", style.base_font);
        assert!(close(style.font_size, 9.0), "{}", style.font_size);
        let [r, g, b] = style.fill_rgb;
        assert!(
            close(r, 0.1) && close(g, 0.2) && close(b, 0.55),
            "{r} {g} {b}"
        );
        assert!(close(style.char_spacing, 0.3), "{}", style.char_spacing);
        assert!(close(style.horizontal_scale, 1.0));
    }

    #[test]
    fn the_quote_paragraph_embeds_a_truetype_program() {
        let (document, page, run) = quote_spec_run();

        let program = text_run_font_program(&document, page, &run)
            .expect("the read succeeds")
            .expect("the subset is embedded");

        assert_eq!(program.format, FontProgramFormat::TrueType);
        // An sfnt version: 1.0 for TrueType outlines.
        assert_eq!(&program.bytes[..4], &[0, 1, 0, 0]);
    }

    #[test]
    fn a_standard_14_run_has_no_program_and_paints_black() {
        let (document, page) = document_with_content(
            b"BT /F1 24 Tf 100 700 Td (Hello) Tj ET",
            helvetica_resources(),
        );
        let run = first_run(&document, page);

        let style = text_run_style(&document, page, &run).expect("a style");
        let program = text_run_font_program(&document, page, &run).expect("the read succeeds");

        assert_eq!(style.base_font, "Helvetica");
        assert!(close(style.font_size, 24.0));
        assert_eq!(style.fill_rgb, [0.0; 3]);
        assert_eq!(program, None);
    }

    #[test]
    fn gray_and_cmyk_fills_convert_to_rgb() {
        let gray = style(b"0.5 g BT /F1 12 Tf 100 700 Td (Hi) Tj ET");
        assert_eq!(gray.fill_rgb, [0.5; 3]);

        let cmyk = style(b"0 1 1 0 k BT /F1 12 Tf 100 700 Td (Hi) Tj ET");
        assert_eq!(cmyk.fill_rgb, [1.0, 0.0, 0.0]);

        let spaced = style(b"/DeviceRGB cs 0 0.5 1 sc BT /F1 12 Tf 100 700 Td (Hi) Tj ET");
        assert_eq!(spaced.fill_rgb, [0.0, 0.5, 1.0]);
    }

    #[test]
    fn a_colour_in_a_space_this_cannot_read_is_black() {
        let style = style(b"/P0 cs /Pat sc BT /F1 12 Tf 100 700 Td (Hi) Tj ET");
        assert_eq!(style.fill_rgb, [0.0; 3]);
    }

    /// Sizes and spacing are what the page shows: scaled by the matrices.
    #[test]
    fn size_and_spacing_are_in_page_space() {
        let style = style(b"2 0 0 2 0 0 cm BT /F1 12 Tf 1 Tc 50 Tz 10 300 Td (Hi) Tj ET");

        assert!(close(style.font_size, 24.0), "{}", style.font_size);
        assert!(close(style.horizontal_scale, 0.5));
        // 1 unit of Tc, squeezed by Tz to 0.5, doubled by the CTM.
        assert!(close(style.char_spacing, 1.0), "{}", style.char_spacing);
    }

    /// A run whose retype is still pending shows text the file does not
    /// hold; it is found by its id and font instead.
    #[test]
    fn a_run_with_pending_text_is_found_by_id_and_font() {
        let (document, page) = document_with_content(
            b"1 0 0 rg BT /F1 12 Tf 100 700 Td (Hello) Tj ET",
            helvetica_resources(),
        );
        let mut run = first_run(&document, page);
        run.text = "Retyped".to_string();

        let style = text_run_style(&document, page, &run).expect("a style");

        assert_eq!(style.fill_rgb, [1.0, 0.0, 0.0]);
    }
}
