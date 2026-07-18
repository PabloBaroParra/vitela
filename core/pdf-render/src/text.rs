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

/// One exact-text match: its 0-indexed page and one PDF-space rect per
/// Unicode scalar in the matched text — enough for a shell to highlight it.
#[derive(Debug, Clone, PartialEq)]
pub struct TextMatch {
    pub page_index: u32,
    pub text: String,
    pub character_bounds: Vec<TextRect>,
}

/// Finds every exact, case-sensitive occurrence of `query` in one page's text
/// runs. Empty queries match nothing.
///
/// Runs concatenate into the page's text, and `collect_text_runs` guarantees
/// exactly one rect per Unicode scalar, so a match's rects are simply the
/// character-indexed slice of the flattened bounds. `match_indices` yields
/// byte offsets, hence the `chars().count()` conversion to a character index.
///
/// This is the single implementation of the match algorithm: it lives beside
/// `TextRun` (the data it operates on) so every shell — the GTK client that
/// links this crate directly and the `pdf-ffi` boundary that other shells
/// cross — shares one behavior instead of keeping private copies.
pub(crate) fn find_matches(runs: &[TextRun], query: &str, page_index: u32) -> Vec<TextMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let page_text: String = runs.iter().map(|run| run.text.as_str()).collect();
    let character_bounds: Vec<TextRect> = runs
        .iter()
        .flat_map(|run| run.character_bounds.iter().copied())
        .collect();
    let query_character_count = query.chars().count();

    page_text
        .match_indices(query)
        .filter_map(|(byte_index, _)| {
            let character_index = page_text[..byte_index].chars().count();
            // `get`, not indexing: a run whose bounds are shorter than its
            // text would otherwise panic the whole document's search.
            let bounds =
                character_bounds.get(character_index..character_index + query_character_count)?;
            Some(TextMatch {
                page_index,
                text: query.to_string(),
                character_bounds: bounds.to_vec(),
            })
        })
        .collect()
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

    /// A run whose bounds carry one rect per character, each tagged with its
    /// character index via `x_pt` so tests can assert which slice was taken.
    fn run(text: &str, first_index: usize) -> TextRun {
        TextRun {
            text: text.to_string(),
            font_name: "Test".to_string(),
            font_size_pt: 12.0,
            character_bounds: (0..text.chars().count())
                .map(|offset| TextRect {
                    x_pt: (first_index + offset) as f32,
                    y_pt: 0.0,
                    width_pt: 1.0,
                    height_pt: 1.0,
                })
                .collect(),
        }
    }

    #[test]
    fn find_matches_returns_nothing_for_an_empty_query() {
        assert_eq!(find_matches(&[run("hello", 0)], "", 0), Vec::new());
    }

    #[test]
    fn find_matches_is_case_sensitive() {
        assert_eq!(find_matches(&[run("Hello", 0)], "hello", 0), Vec::new());
    }

    #[test]
    fn find_matches_reports_every_occurrence_with_its_page_index() {
        let matches = find_matches(&[run("abcabc", 0)], "bc", 3);

        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|m| m.page_index == 3 && m.text == "bc"));
        // Character bounds are the slice for each occurrence: chars 1..3 and 4..6.
        assert_eq!(matches[0].character_bounds[0].x_pt, 1.0);
        assert_eq!(matches[1].character_bounds[0].x_pt, 4.0);
    }

    #[test]
    fn find_matches_spans_a_run_boundary() {
        // "ab" + "cd" concatenate to "abcd": a match crossing the two runs
        // must still resolve to the flattened bounds 1..3.
        let matches = find_matches(&[run("ab", 0), run("cd", 2)], "bc", 0);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].character_bounds.len(), 2);
        assert_eq!(matches[0].character_bounds[0].x_pt, 1.0);
        assert_eq!(matches[0].character_bounds[1].x_pt, 2.0);
    }

    #[test]
    fn find_matches_uses_character_indices_not_byte_offsets() {
        // "ñ" is two bytes: a byte-indexed slice would take the wrong rects.
        let matches = find_matches(&[run("ñab", 0)], "ab", 0);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].character_bounds[0].x_pt, 1.0);
    }

    #[test]
    fn find_matches_skips_a_match_whose_bounds_are_short_instead_of_panicking() {
        let mut short = run("abc", 0);
        short.character_bounds.pop();

        assert_eq!(find_matches(&[short], "bc", 0), Vec::new());
    }

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
