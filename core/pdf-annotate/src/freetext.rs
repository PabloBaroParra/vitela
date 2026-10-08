//! Free-text layout: the one place that decides where each line of a text
//! box goes.
//!
//! The saved `/AP` stream, every shell overlay and the FFI snapshot all
//! consume [`layout`], so a line break can never differ between what the
//! user sees while editing and what other viewers show after saving.
//!
//! The frame is the rect's own: origin at its top-left corner, y growing
//! downwards, in points — the "upright frame" the shells already use for
//! stamps. The AP converts to PDF's bottom-left space itself.

use crate::error::AnnotateError;
use pdf_document::{Color, FontFamily, Rect, TextStyle};
use pdf_edit::encoding::winansi::char_width_thousandths;
use pdf_edit::encoding::wrap::wrap_greedy;

/// Space between the rect's edge and the text, on every side.
pub const FREE_TEXT_PADDING_PT: f64 = 2.0;
/// Line pitch as a multiple of the font size.
pub const FREE_TEXT_LEADING: f64 = 1.15;
/// Helvetica's ascender (AFM `Ascender 718`), as a multiple of the font
/// size: how far the first baseline sits below the padded top.
pub const FREE_TEXT_ASCENT: f64 = 0.718;
/// Helvetica's descender (AFM `Descender -207`), as a multiple of the size.
const DESCENT: f64 = 0.207;

/// The style every new text box gets: Helvetica 12pt black. The model
/// carries a [`TextStyle`] so a styling UI can land later without touching
/// every constructor, but v1 offers no way to change it.
pub fn default_free_text_style() -> TextStyle {
    TextStyle {
        font: FontFamily::Helvetica,
        size_pt: 12.0,
        color: Color { r: 0, g: 0, b: 0 },
    }
}

/// Validates text for a free-text box and returns it with line breaks
/// normalized to `\n`.
///
/// Blank text is refused (deleting the annotation is how you remove it),
/// and so is any character WinAnsi cannot show — the same rule the layout
/// and the saved appearance apply, enforced here so the model can never
/// hold text that would fail at save time.
pub(crate) fn checked_contents(contents: &str) -> Result<String, AnnotateError> {
    if contents.trim().is_empty() {
        return Err(AnnotateError::UnsupportedOperation("empty free text"));
    }
    let normalized = contents.replace("\r\n", "\n").replace('\r', "\n");
    for character in normalized.chars().filter(|c| *c != '\n') {
        if char_width_thousandths(character).is_none() {
            return Err(AnnotateError::EncodingGap { character });
        }
    }
    Ok(normalized)
}

/// The smallest box a resize may produce: one glyph wide and one line high,
/// plus the padding on both sides of each. Returns `(width, height)`.
pub fn min_free_text_size(style: &TextStyle) -> (f64, f64) {
    (
        style.size_pt + 2.0 * FREE_TEXT_PADDING_PT,
        FREE_TEXT_LEADING * style.size_pt + 2.0 * FREE_TEXT_PADDING_PT,
    )
}

/// A rect a box can be drawn into: finite and strictly positive in both
/// dimensions.
pub(crate) fn is_drawable(rect: &Rect) -> bool {
    [rect.x, rect.y, rect.width, rect.height]
        .iter()
        .all(|v| v.is_finite())
        && rect.width > 0.0
        && rect.height > 0.0
}

/// One laid-out line, in the rect's top-left, y-down frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FreeTextLine {
    pub text: String,
    /// Left edge of the line.
    pub x_pt: f64,
    /// Distance from the rect's top edge down to this line's baseline.
    pub baseline_from_top_pt: f64,
}

/// The result of laying a text box out.
#[derive(Debug, Clone, PartialEq)]
pub struct FreeTextLayout {
    pub font_size_pt: f64,
    pub lines: Vec<FreeTextLine>,
    /// True when the lines do not all fit inside the height. They are still
    /// all returned; the painter clips to the rect instead of truncating.
    pub overflow: bool,
}

/// Lays `contents` out in a `width_pt` x `height_pt` box.
///
/// Greedy word wrap on spaces, explicit line breaks honored, and a word
/// wider than the box broken by character so no line is wider than the
/// padded width (a box narrower than one glyph still gets one glyph per
/// line). Runs of spaces collapse to one, a documented v1 limit shared with
/// form fields.
///
/// A character WinAnsi cannot show is an
/// [`AnnotateError::EncodingGap`]: measuring it would be a guess.
pub fn layout(
    contents: &str,
    style: &TextStyle,
    width_pt: f64,
    height_pt: f64,
) -> Result<FreeTextLayout, AnnotateError> {
    let size = style.size_pt;
    if size <= 0.0 {
        return Err(AnnotateError::UnsupportedOperation(
            "non-positive font size",
        ));
    }
    let available = (width_pt - 2.0 * FREE_TEXT_PADDING_PT).max(0.0);
    let text = contents.replace("\r\n", "\n").replace('\r', "\n");
    let measure = |line: &str| text_width_pt(line, size);

    let mut wrapped = Vec::new();
    for line in wrap_greedy(&text, available, measure)
        .map_err(|character| AnnotateError::EncodingGap { character })?
    {
        wrapped.extend(break_by_character(&line, available, size)?);
    }

    let leading = FREE_TEXT_LEADING * size;
    let first_baseline = FREE_TEXT_PADDING_PT + FREE_TEXT_ASCENT * size;
    let lines: Vec<FreeTextLine> = wrapped
        .into_iter()
        .enumerate()
        .map(|(index, text)| FreeTextLine {
            text,
            x_pt: FREE_TEXT_PADDING_PT,
            baseline_from_top_pt: first_baseline + leading * index as f64,
        })
        .collect();
    let overflow = lines.last().is_some_and(|last| {
        last.baseline_from_top_pt + DESCENT * size + FREE_TEXT_PADDING_PT > height_pt
    });

    Ok(FreeTextLayout {
        font_size_pt: size,
        lines,
        overflow,
    })
}

fn text_width_pt(text: &str, size_pt: f64) -> Result<f64, char> {
    let mut thousandths = 0u32;
    for character in text.chars() {
        let width = char_width_thousandths(character).ok_or(character)?;
        thousandths += u32::from(width);
    }
    Ok(f64::from(thousandths) / 1000.0 * size_pt)
}

/// Splits a line wider than `available` into chunks that each fit, at least
/// one character per chunk so a hopelessly narrow box still terminates.
fn break_by_character(
    line: &str,
    available: f64,
    size_pt: f64,
) -> Result<Vec<String>, AnnotateError> {
    let gap = |character| AnnotateError::EncodingGap { character };
    if text_width_pt(line, size_pt).map_err(gap)? <= available {
        return Ok(vec![line.to_string()]);
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for character in line.chars() {
        current.push(character);
        if current.chars().count() > 1 && text_width_pt(&current, size_pt).map_err(gap)? > available
        {
            current.pop();
            chunks.push(std::mem::take(&mut current));
            current.push(character);
        }
    }
    chunks.push(current);
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Color, FontFamily, TextStyle};

    fn style() -> TextStyle {
        TextStyle {
            font: FontFamily::Helvetica,
            size_pt: 12.0,
            color: Color { r: 0, g: 0, b: 0 },
        }
    }

    fn texts(layout: &FreeTextLayout) -> Vec<&str> {
        layout.lines.iter().map(|l| l.text.as_str()).collect()
    }

    fn line_width(text: &str) -> f64 {
        text.chars()
            .map(|c| f64::from(pdf_edit::encoding::winansi::char_width_thousandths(c).unwrap()))
            .sum::<f64>()
            / 1000.0
            * 12.0
    }

    #[test]
    fn wraps_greedily_at_the_padded_width() {
        // 60pt box, 2pt padding each side: 56pt of text. "aaa bbb" is
        // 43.4pt, adding " ccc" would reach 70pt.
        let layout = layout("aaa bbb ccc ddd", &style(), 60.0, 100.0).unwrap();
        assert_eq!(texts(&layout), ["aaa bbb", "ccc ddd"]);
    }

    #[test]
    fn a_wide_box_keeps_one_line() {
        let layout = layout("aaa bbb ccc ddd", &style(), 400.0, 100.0).unwrap();
        assert_eq!(texts(&layout), ["aaa bbb ccc ddd"]);
    }

    #[test]
    fn no_wrapped_line_is_wider_than_the_padded_width() {
        let layout = layout("aaa bbb ccc ddd eee fff ggg", &style(), 60.0, 200.0).unwrap();
        assert!(layout.lines.len() > 2);
        for line in &layout.lines {
            assert!(
                line_width(&line.text) <= 56.0,
                "{:?} is too wide",
                line.text
            );
        }
    }

    #[test]
    fn an_explicit_newline_breaks_the_line() {
        let layout = layout("a\nb", &style(), 400.0, 100.0).unwrap();
        assert_eq!(texts(&layout), ["a", "b"]);
    }

    #[test]
    fn crlf_and_lone_cr_count_as_one_break_each() {
        let layout = layout("a\r\nb\rc", &style(), 400.0, 100.0).unwrap();
        assert_eq!(texts(&layout), ["a", "b", "c"]);
    }

    #[test]
    fn a_word_wider_than_the_box_is_split_by_character() {
        // 'W' is 11.328pt at 12pt; 46pt of text fits four of them.
        let layout = layout("WWWWWWWWWW", &style(), 50.0, 200.0).unwrap();
        assert_eq!(texts(&layout), ["WWWW", "WWWW", "WW"]);
    }

    #[test]
    fn a_box_narrower_than_one_glyph_still_makes_progress() {
        let layout = layout("abc", &style(), 3.0, 200.0).unwrap();
        assert_eq!(texts(&layout), ["a", "b", "c"]);
    }

    #[test]
    fn the_first_baseline_sits_one_ascent_below_the_padded_top() {
        let layout = layout("a", &style(), 100.0, 100.0).unwrap();
        let line = &layout.lines[0];
        assert_eq!(line.x_pt, FREE_TEXT_PADDING_PT);
        let expected = FREE_TEXT_PADDING_PT + FREE_TEXT_ASCENT * 12.0;
        assert!((line.baseline_from_top_pt - expected).abs() < 1e-9);
    }

    #[test]
    fn successive_baselines_are_one_leading_apart() {
        let layout = layout("a\nb\nc", &style(), 100.0, 100.0).unwrap();
        let gap = layout.lines[1].baseline_from_top_pt - layout.lines[0].baseline_from_top_pt;
        assert!((gap - FREE_TEXT_LEADING * 12.0).abs() < 1e-9);
        let gap = layout.lines[2].baseline_from_top_pt - layout.lines[1].baseline_from_top_pt;
        assert!((gap - FREE_TEXT_LEADING * 12.0).abs() < 1e-9);
    }

    #[test]
    fn lines_past_the_height_are_kept_and_flagged_as_overflow() {
        let layout = layout("a\nb\nc\nd", &style(), 100.0, 20.0).unwrap();
        assert_eq!(layout.lines.len(), 4);
        assert!(layout.overflow);
    }

    #[test]
    fn text_that_fits_is_not_overflow() {
        let layout = layout("a", &style(), 100.0, 50.0).unwrap();
        assert!(!layout.overflow);
    }

    #[test]
    fn the_layout_carries_the_font_size() {
        let mut big = style();
        big.size_pt = 20.0;
        let layout = layout("a", &big, 100.0, 50.0).unwrap();
        assert_eq!(layout.font_size_pt, 20.0);
    }

    #[test]
    fn spanish_accents_are_measured_not_refused() {
        let layout = layout("Canción, año ¿sí?", &style(), 400.0, 100.0).unwrap();
        assert_eq!(texts(&layout), ["Canción, año ¿sí?"]);
    }

    #[test]
    fn a_character_winansi_lacks_is_an_encoding_gap() {
        let result = layout("Hola 日本", &style(), 400.0, 100.0);
        assert_eq!(result, Err(AnnotateError::EncodingGap { character: '日' }));
    }

    #[test]
    fn a_non_positive_font_size_is_refused() {
        let mut bad = style();
        bad.size_pt = 0.0;
        assert!(matches!(
            layout("a", &bad, 100.0, 50.0),
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }
}
