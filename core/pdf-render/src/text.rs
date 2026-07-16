//! Text-run query stub (T-018) — the "Text-Run Data Exposure" future-phase
//! enabler from `spec.md`: MVP performs no text edits, but the render layer
//! must expose per-run text position/font data so a later text-editing
//! phase doesn't require a render-layer rewrite.

use pdfium_render::prelude::{PdfPageText, PdfPoints, PdfRect, PdfiumError};

/// A run of consecutive characters sharing the same font and font size,
/// with PDF-space bounds for every character.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub font_name: String,
    pub font_size_pt: f32,
    pub character_bounds: Vec<TextRect>,
}

/// One character's bounding rectangle in PDF points with a bottom-left
/// origin. It is kept renderer-owned so shell boundaries can translate it to
/// their own stable DTOs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextRect {
    pub x_pt: f32,
    pub y_pt: f32,
    pub width_pt: f32,
    pub height_pt: f32,
}

/// Computes one character's [`TextRect`] from pdfium's per-glyph geometry.
///
/// pdfium can't always compute loose bounds — certain whitespace or
/// missing-glyph entries return an error. Rather than fail the whole page's
/// extraction (and with it whole-document search), a glyph without loose
/// bounds falls back to a zero-size rect at its origin, and one whose origin
/// also fails collapses to the page's bottom-left corner. Either way exactly
/// one rect is produced per character — the invariant callers rely on.
fn character_rect(
    loose_bounds: Result<PdfRect, PdfiumError>,
    origin: Result<(PdfPoints, PdfPoints), PdfiumError>,
) -> TextRect {
    match loose_bounds {
        Ok(bounds) => TextRect {
            x_pt: bounds.left().value,
            y_pt: bounds.bottom().value,
            width_pt: bounds.width().value,
            height_pt: bounds.height().value,
        },
        Err(_) => {
            let (x, y) = origin.unwrap_or((PdfPoints::ZERO, PdfPoints::ZERO));
            TextRect {
                x_pt: x.value,
                y_pt: y.value,
                width_pt: 0.0,
                height_pt: 0.0,
            }
        }
    }
}

/// Groups a page's individual characters (as exposed by pdfium's text-page
/// API) into runs sharing the same font name and font size.
pub(crate) fn collect_text_runs(text: &PdfPageText) -> Vec<TextRun> {
    struct Building {
        text: String,
        font_name: String,
        font_size_pt: f32,
        character_bounds: Vec<TextRect>,
    }

    let mut runs = Vec::new();
    let mut current: Option<Building> = None;

    for ch in text.chars().iter() {
        let font_name = ch.font_name();
        let font_size_pt = ch.scaled_font_size().value;
        let unicode = ch.unicode_char().unwrap_or('\u{FFFD}');
        let character_bounds = character_rect(ch.loose_bounds(), ch.origin());

        let same_run = current
            .as_ref()
            .map(|b| b.font_name == font_name && (b.font_size_pt - font_size_pt).abs() < 0.01)
            .unwrap_or(false);

        if same_run {
            if let Some(current) = current.as_mut() {
                current.text.push(unicode);
                current.character_bounds.push(character_bounds);
            }
        } else {
            if let Some(building) = current.take() {
                runs.push(TextRun {
                    text: building.text,
                    font_name: building.font_name,
                    font_size_pt: building.font_size_pt,
                    character_bounds: building.character_bounds,
                });
            }
            current = Some(Building {
                text: unicode.to_string(),
                font_name,
                font_size_pt,
                character_bounds: vec![character_bounds],
            });
        }
    }

    if let Some(building) = current {
        runs.push(TextRun {
            text: building.text,
            font_name: building.font_name,
            font_size_pt: building.font_size_pt,
            character_bounds: building.character_bounds,
        });
    }

    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    // pdfium exposes `loose_bounds`/`origin` as fallible FFI calls; the error
    // variant is opaque, so any unit variant stands in for "pdfium said no".
    const PDFIUM_ERR: PdfiumError = PdfiumError::PageIndexOutOfBounds;

    #[test]
    fn character_rect_uses_loose_bounds_when_available() {
        // new_from_values(bottom, left, top, right): left=20, bottom=10,
        // width=right-left=30, height=top-bottom=30.
        let rect = character_rect(
            Ok(PdfRect::new_from_values(10.0, 20.0, 40.0, 50.0)),
            Err(PDFIUM_ERR),
        );

        assert_eq!(
            rect,
            TextRect {
                x_pt: 20.0,
                y_pt: 10.0,
                width_pt: 30.0,
                height_pt: 30.0,
            }
        );
    }

    #[test]
    fn character_rect_falls_back_to_a_zero_size_rect_at_the_glyph_origin() {
        let rect = character_rect(
            Err(PDFIUM_ERR),
            Ok((PdfPoints::new(72.0), PdfPoints::new(144.0))),
        );

        // A glyph pdfium can't bound still yields exactly one rect — anchored at
        // its real position so a highlight lands on the glyph, not in a corner.
        assert_eq!(
            rect,
            TextRect {
                x_pt: 72.0,
                y_pt: 144.0,
                width_pt: 0.0,
                height_pt: 0.0,
            }
        );
    }

    #[test]
    fn character_rect_collapses_to_the_origin_when_neither_bounds_nor_origin_resolve() {
        let rect = character_rect(Err(PDFIUM_ERR), Err(PDFIUM_ERR));

        assert_eq!(
            rect,
            TextRect {
                x_pt: 0.0,
                y_pt: 0.0,
                width_pt: 0.0,
                height_pt: 0.0,
            }
        );
    }
}
