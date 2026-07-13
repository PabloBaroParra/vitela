//! Text-run query stub (T-018) — the "Text-Run Data Exposure" future-phase
//! enabler from `spec.md`: MVP performs no text edits, but the render layer
//! must expose per-run text position/font data so a later text-editing
//! phase doesn't require a render-layer rewrite.

use pdfium_render::prelude::{PdfPageText, PdfPoints};

/// A run of consecutive characters sharing the same font and font size,
/// with the position of the run's first character.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub font_name: String,
    pub font_size_pt: f32,
    pub origin_x_pt: f32,
    pub origin_y_pt: f32,
}

/// Groups a page's individual characters (as exposed by pdfium's text-page
/// API) into runs sharing the same font name and font size.
pub(crate) fn collect_text_runs(text: &PdfPageText) -> Vec<TextRun> {
    struct Building {
        text: String,
        font_name: String,
        font_size_pt: f32,
        origin_x_pt: f32,
        origin_y_pt: f32,
    }

    let mut runs = Vec::new();
    let mut current: Option<Building> = None;

    for ch in text.chars().iter() {
        let font_name = ch.font_name();
        let font_size_pt = ch.scaled_font_size().value;
        let (origin_x, origin_y) = ch.origin().unwrap_or((PdfPoints::ZERO, PdfPoints::ZERO));
        let unicode = ch.unicode_char().unwrap_or('\u{FFFD}');

        let same_run = current
            .as_ref()
            .map(|b| b.font_name == font_name && (b.font_size_pt - font_size_pt).abs() < 0.01)
            .unwrap_or(false);

        if same_run {
            current.as_mut().unwrap().text.push(unicode);
        } else {
            if let Some(building) = current.take() {
                runs.push(TextRun {
                    text: building.text,
                    font_name: building.font_name,
                    font_size_pt: building.font_size_pt,
                    origin_x_pt: building.origin_x_pt,
                    origin_y_pt: building.origin_y_pt,
                });
            }
            current = Some(Building {
                text: unicode.to_string(),
                font_name,
                font_size_pt,
                origin_x_pt: origin_x.value,
                origin_y_pt: origin_y.value,
            });
        }
    }

    if let Some(building) = current {
        runs.push(TextRun {
            text: building.text,
            font_name: building.font_name,
            font_size_pt: building.font_size_pt,
            origin_x_pt: building.origin_x_pt,
            origin_y_pt: building.origin_y_pt,
        });
    }

    runs
}
