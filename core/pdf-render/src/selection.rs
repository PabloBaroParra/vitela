//! Text-selection geometry: turning a point on a page into a caret position,
//! a caret range into the rects a shell paints, and into the text it copies.
//!
//! Every shell needs the same three answers to drive a drag-select, and none
//! of them depend on a toolkit — they are arithmetic over the character rects
//! `text_runs` already returns. So they live here, beside [`find_matches`],
//! for the same reason that one does: one implementation shared by the GTK
//! client that links this crate directly and by the shells that reach it
//! through `pdf-ffi`, rather than four private copies that drift apart.
//!
//! [`find_matches`]: crate::text

use std::ops::Range;

use crate::text::{TextRect, TextRun};

/// A caret position: an index *between* characters, from 0 (before the first)
/// to the character count (after the last).
///
/// Carets rather than character indices because that is what a selection
/// actually is. With character indices a click that selects nothing is
/// unrepresentable — the smallest range covers one character — so a plain
/// click would always grab a glyph the user never dragged over.
pub type Caret = usize;

/// A page's characters flattened out of its runs: one entry per Unicode
/// scalar, in reading order, each with its PDF-space rect.
///
/// Built once per page and kept by the shell, because a drag-select asks
/// these questions on every pointer-motion event and re-flattening a
/// text-heavy page's runs each time would allocate thousands of rects per
/// mouse move.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageCharacters {
    characters: Vec<char>,
    bounds: Vec<TextRect>,
}

impl PageCharacters {
    /// Flattens a page's runs, keeping the two vectors the same length.
    ///
    /// `collect_text_runs` promises one rect per character, but a run that
    /// broke that promise would otherwise desynchronise text from geometry
    /// for the whole rest of the page — every later character would report
    /// its neighbour's rect. Truncating each run to the shorter of the two
    /// contains the damage to that run.
    pub fn from_runs(runs: &[TextRun]) -> Self {
        let mut characters = Vec::new();
        let mut bounds = Vec::new();
        for run in runs {
            let paired = run.text.chars().zip(run.character_bounds.iter().copied());
            for (character, rect) in paired {
                characters.push(character);
                bounds.push(rect);
            }
        }
        PageCharacters { characters, bounds }
    }

    pub fn len(&self) -> usize {
        self.characters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.characters.is_empty()
    }

    /// The caret nearest to a point in PDF space (bottom-left origin), or
    /// `None` on a page with no positioned text.
    ///
    /// Resolves the line first and the column second — the same order a
    /// reader's eye uses. A single nearest-rect search would let a click in
    /// the wide margin beside a short line land on a character from the line
    /// above, whichever happened to be closer in a straight line.
    pub fn caret_at(&self, x_pt: f32, y_pt: f32) -> Option<Caret> {
        let line = self.line_at(y_pt)?;
        Some(self.caret_in_line(line, x_pt))
    }

    /// The text covered by a caret range, for the clipboard.
    pub fn text_in(&self, range: Range<Caret>) -> String {
        self.clamp(range)
            .map(|index| self.characters[index])
            .collect()
    }

    /// The rects a shell paints for a caret range: one per visual line, not
    /// one per glyph.
    pub fn rects_in(&self, range: Range<Caret>) -> Vec<TextRect> {
        line_rects(&self.bounds[self.clamp(range)])
    }

    /// Clamps a caret range to the page and orders it, so a range built from
    /// a stale selection can only ever come up short, never panic.
    fn clamp(&self, range: Range<Caret>) -> Range<usize> {
        let end = range.end.min(self.characters.len());
        range.start.min(end)..end
    }

    /// The contiguous span of characters on the line nearest `y_pt`.
    ///
    /// Contiguity is sound because runs arrive in reading order, so a line is
    /// always a slice — never a scattered set of indices.
    fn line_at(&self, y_pt: f32) -> Option<Range<usize>> {
        let seed = (0..self.bounds.len())
            .filter(|index| !is_degenerate(&self.bounds[*index]))
            .min_by(|left, right| {
                vertical_gap(&self.bounds[*left], y_pt)
                    .total_cmp(&vertical_gap(&self.bounds[*right], y_pt))
            })?;
        let seed_rect = self.bounds[seed];

        // Degenerate characters are stepped over rather than treated as line
        // breaks: a space pdfium could not bound sits mid-line, and stopping
        // there would cut the line in two.
        let on_line = |index: usize| {
            let rect = &self.bounds[index];
            is_degenerate(rect) || shares_line(&seed_rect, rect)
        };
        let start = (0..seed)
            .rev()
            .take_while(|i| on_line(*i))
            .last()
            .unwrap_or(seed);
        let end = (seed + 1..self.bounds.len())
            .take_while(|i| on_line(*i))
            .last()
            .map_or(seed + 1, |last| last + 1);
        Some(start..end)
    }

    /// The caret within one line nearest `x_pt`: before the nearest character
    /// if the point falls in its left half, after it otherwise.
    fn caret_in_line(&self, line: Range<usize>, x_pt: f32) -> Caret {
        let nearest = line
            .clone()
            .min_by(|left, right| {
                horizontal_gap(&self.bounds[*left], x_pt)
                    .total_cmp(&horizontal_gap(&self.bounds[*right], x_pt))
            })
            .unwrap_or(line.start);
        let rect = &self.bounds[nearest];
        if x_pt >= rect.x_pt + rect.width_pt / 2.0 {
            nearest + 1
        } else {
            nearest
        }
    }
}

/// Collapses a reading-order run of character rects into one rect per visual
/// line.
///
/// A per-glyph highlight looks striped, because adjacent loose bounds leave
/// hairline gaps between them, and costs one draw call per character.
/// Unioning consecutive same-line characters yields the handful of bars a
/// user expects to see.
///
/// Shared by both things a shell highlights — a drag-selection and a search
/// match — because a [`TextMatch`](crate::text::TextMatch)'s bounds are the
/// same per-character rects in the same reading order.
pub fn line_rects(bounds: &[TextRect]) -> Vec<TextRect> {
    let mut lines = Vec::new();
    let mut current: Option<TextRect> = None;
    for rect in bounds.iter().copied() {
        // Degenerate rects are pdfium's fallback for glyphs it cannot bound —
        // typically whitespace. They carry no area to highlight, and letting
        // one start a line would anchor the union at a point rather than
        // around the text.
        if is_degenerate(&rect) {
            continue;
        }
        current = match current {
            Some(line) if shares_line(&line, &rect) => Some(union(line, rect)),
            Some(line) => {
                lines.push(line);
                Some(rect)
            }
            None => Some(rect),
        };
    }
    lines.extend(current);
    lines
}

/// Orders the two ends of a drag into a range.
///
/// The anchor is where the press landed and the focus where the pointer is
/// now, so dragging up or leftwards puts them in the wrong order — a case
/// every caller would otherwise have to remember on its own.
pub fn caret_range(anchor: Caret, focus: Caret) -> Range<Caret> {
    anchor.min(focus)..anchor.max(focus)
}

/// A rect placed in a shell's drawing space: top-left origin, y growing
/// downwards, scaled to the size the page is displayed at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// Places a PDF-space rect on a page drawn at `scale` display units per point.
///
/// PDF space has a bottom-left origin and every toolkit draws from the top
/// left, so the flip has to happen somewhere. It happens here, once, rather
/// than in each shell's paint code where an off-by-a-height error just looks
/// like a highlight sitting slightly low.
pub fn place_rect(rect: TextRect, page_height_pt: f32, scale: f64) -> PlacedRect {
    let top_pt = f64::from(page_height_pt) - f64::from(rect.y_pt + rect.height_pt);
    PlacedRect {
        left: f64::from(rect.x_pt) * scale,
        top: top_pt * scale,
        width: f64::from(rect.width_pt) * scale,
        height: f64::from(rect.height_pt) * scale,
    }
}

/// The inverse of [`place_rect`] for a single point: a pointer position on a
/// drawn page, back into PDF space, ready for [`PageCharacters::caret_at`].
///
/// A `scale` of zero or worse would otherwise divide the pointer into
/// infinity and hand `caret_at` a NaN that compares false against everything,
/// so it falls back to 1:1 rather than propagating the poison.
pub fn point_to_pdf(x: f64, y: f64, page_height_pt: f32, scale: f64) -> (f32, f32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    ((x / scale) as f32, page_height_pt - (y / scale) as f32)
}

fn is_degenerate(rect: &TextRect) -> bool {
    rect.width_pt <= 0.0 && rect.height_pt <= 0.0
}

/// Whether two rects overlap vertically, i.e. sit on the same line of text.
fn shares_line(a: &TextRect, b: &TextRect) -> bool {
    a.y_pt < b.y_pt + b.height_pt && b.y_pt < a.y_pt + a.height_pt
}

fn vertical_gap(rect: &TextRect, y_pt: f32) -> f32 {
    gap(rect.y_pt, rect.y_pt + rect.height_pt, y_pt)
}

fn horizontal_gap(rect: &TextRect, x_pt: f32) -> f32 {
    gap(rect.x_pt, rect.x_pt + rect.width_pt, x_pt)
}

/// Distance from a value to a span: zero inside it, else the shortfall.
fn gap(low: f32, high: f32, value: f32) -> f32 {
    if value < low {
        low - value
    } else if value > high {
        value - high
    } else {
        0.0
    }
}

fn union(a: TextRect, b: TextRect) -> TextRect {
    let left = a.x_pt.min(b.x_pt);
    let bottom = a.y_pt.min(b.y_pt);
    let right = (a.x_pt + a.width_pt).max(b.x_pt + b.width_pt);
    let top = (a.y_pt + a.height_pt).max(b.y_pt + b.height_pt);
    TextRect {
        x_pt: left,
        y_pt: bottom,
        width_pt: right - left,
        height_pt: top - bottom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a run of single-width characters on one baseline, laid out left
    /// to right from `x_pt` — the shape of a line of text, in miniature.
    fn line(text: &str, x_pt: f32, y_pt: f32) -> TextRun {
        TextRun {
            text: text.to_string(),
            font_name: "Test".to_string(),
            font_size_pt: 10.0,
            character_bounds: (0..text.chars().count())
                .map(|offset| TextRect {
                    x_pt: x_pt + offset as f32 * 10.0,
                    y_pt,
                    width_pt: 10.0,
                    height_pt: 10.0,
                })
                .collect(),
        }
    }

    /// Two stacked lines: "ab" at y=100 (upper) and "cd" at y=50 (lower).
    fn two_lines() -> PageCharacters {
        PageCharacters::from_runs(&[line("ab", 0.0, 100.0), line("cd", 0.0, 50.0)])
    }

    #[test]
    fn from_runs_pairs_every_character_with_its_rect() {
        let page = PageCharacters::from_runs(&[line("ab", 0.0, 100.0)]);

        assert_eq!(page.len(), 2);
        assert_eq!(page.text_in(0..2), "ab");
    }

    #[test]
    fn from_runs_truncates_a_run_whose_bounds_are_short_instead_of_desyncing() {
        let mut short = line("abc", 0.0, 100.0);
        short.character_bounds.pop();
        let page = PageCharacters::from_runs(&[short, line("de", 0.0, 50.0)]);

        // The third character is dropped with its missing rect, so "d" and
        // "e" still report their own geometry rather than their neighbour's.
        assert_eq!(page.text_in(0..page.len()), "abde");
        assert_eq!(page.rects_in(2..4)[0].y_pt, 50.0);
    }

    #[test]
    fn caret_at_returns_none_on_a_page_without_positioned_text() {
        assert_eq!(PageCharacters::default().caret_at(10.0, 10.0), None);
    }

    #[test]
    fn caret_at_lands_before_a_character_when_the_point_is_in_its_left_half() {
        // "a" spans x 0..10 on the upper line; x=3 is left of its midpoint.
        assert_eq!(two_lines().caret_at(3.0, 105.0), Some(0));
    }

    #[test]
    fn caret_at_lands_after_a_character_when_the_point_is_in_its_right_half() {
        assert_eq!(two_lines().caret_at(7.0, 105.0), Some(1));
    }

    #[test]
    fn caret_at_clamps_to_the_end_of_the_line_when_the_point_is_past_it() {
        // Far right of the upper line: caret 2 is after "b", before "c" on
        // the next line — never inside the lower line.
        assert_eq!(two_lines().caret_at(500.0, 105.0), Some(2));
    }

    #[test]
    fn caret_at_clamps_to_the_start_of_the_line_when_the_point_is_before_it() {
        assert_eq!(two_lines().caret_at(-500.0, 55.0), Some(2));
    }

    #[test]
    fn caret_at_resolves_the_line_before_the_column() {
        // x=500 is far right of every character, so a plain nearest-rect
        // search would pick "b" (the upper line) for a click at y=55, which
        // is 45pt below it and only level with the lower line. Line first
        // puts the caret at the end of the lower line instead.
        assert_eq!(two_lines().caret_at(500.0, 55.0), Some(4));
    }

    #[test]
    fn caret_at_snaps_to_the_nearest_line_when_the_point_is_between_them() {
        // y=97 sits in the gap: 3pt below the upper line, 37pt above the lower.
        assert_eq!(two_lines().caret_at(3.0, 97.0), Some(0));
    }

    #[test]
    fn caret_range_orders_a_backwards_drag() {
        assert_eq!(caret_range(4, 1), 1..4);
        assert_eq!(caret_range(1, 4), 1..4);
    }

    #[test]
    fn caret_range_of_a_click_without_a_drag_is_empty() {
        assert!(caret_range(2, 2).is_empty());
        assert_eq!(two_lines().text_in(caret_range(2, 2)), "");
    }

    #[test]
    fn rects_in_unions_a_run_of_same_line_characters_into_one_bar() {
        let rects = two_lines().rects_in(0..2);

        assert_eq!(rects.len(), 1);
        assert_eq!(
            rects[0],
            TextRect {
                x_pt: 0.0,
                y_pt: 100.0,
                width_pt: 20.0,
                height_pt: 10.0,
            }
        );
    }

    #[test]
    fn rects_in_emits_one_bar_per_line_for_a_selection_that_spans_lines() {
        let rects = two_lines().rects_in(1..3);

        // "b" on the upper line and "c" on the lower: two bars, never one
        // box swallowing the whitespace between them.
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].y_pt, 100.0);
        assert_eq!(rects[1].y_pt, 50.0);
    }

    #[test]
    fn rects_in_skips_characters_pdfium_could_not_bound() {
        let mut with_space = line("ab", 0.0, 100.0);
        with_space.character_bounds[0] = TextRect {
            x_pt: 0.0,
            y_pt: 0.0,
            width_pt: 0.0,
            height_pt: 0.0,
        };
        let page = PageCharacters::from_runs(&[with_space]);

        // One bar around "b" alone — the unbounded glyph must not drag the
        // union down to the page's bottom-left corner.
        assert_eq!(page.rects_in(0..2).len(), 1);
        assert_eq!(page.rects_in(0..2)[0].y_pt, 100.0);
    }

    #[test]
    fn rects_in_is_empty_for_an_empty_selection() {
        assert!(two_lines().rects_in(2..2).is_empty());
    }

    #[test]
    fn a_line_survives_a_character_pdfium_could_not_bound_in_its_middle() {
        let mut with_space = line("a b", 0.0, 100.0);
        with_space.character_bounds[1] = TextRect {
            x_pt: 10.0,
            y_pt: 100.0,
            width_pt: 0.0,
            height_pt: 0.0,
        };
        let page = PageCharacters::from_runs(&[with_space]);

        // The unbounded space must not cut the line: a click past "b" still
        // resolves to the caret after it, not to the caret before the space.
        assert_eq!(page.caret_at(500.0, 105.0), Some(3));
    }

    #[test]
    fn a_range_reaching_past_the_page_is_clamped_rather_than_panicking() {
        let page = two_lines();

        assert_eq!(page.text_in(2..99), "cd");
        assert_eq!(page.rects_in(2..99).len(), 1);
    }

    #[test]
    fn line_rects_bars_a_search_matchs_bounds_the_same_way_a_selection_is_barred() {
        // A `TextMatch` carries the same per-character rects in the same
        // order, so the highlight a shell paints for it comes from here too.
        let page = two_lines();
        let matched = line("ab", 0.0, 100.0).character_bounds;

        assert_eq!(line_rects(&matched), page.rects_in(0..2));
    }

    #[test]
    fn place_rect_flips_the_y_axis_and_scales() {
        // A 10pt-tall rect sitting on the baseline at y=100 on a 792pt page
        // is 792 - 110 = 682pt from the top, doubled by the 2x scale.
        let placed = place_rect(
            TextRect {
                x_pt: 5.0,
                y_pt: 100.0,
                width_pt: 20.0,
                height_pt: 10.0,
            },
            792.0,
            2.0,
        );

        assert_eq!(
            placed,
            PlacedRect {
                left: 10.0,
                top: 1_364.0,
                width: 40.0,
                height: 20.0,
            }
        );
    }

    #[test]
    fn point_to_pdf_inverts_place_rect() {
        let rect = TextRect {
            x_pt: 5.0,
            y_pt: 100.0,
            width_pt: 20.0,
            height_pt: 10.0,
        };
        let placed = place_rect(rect, 792.0, 1.5);

        // Round-tripping the placed top-left lands back on the rect's PDF-space
        // top-left, which is its `y_pt + height_pt` edge.
        let (x_pt, y_pt) = point_to_pdf(placed.left, placed.top, 792.0, 1.5);
        assert!((x_pt - rect.x_pt).abs() < 1e-3);
        assert!((y_pt - (rect.y_pt + rect.height_pt)).abs() < 1e-3);
    }

    #[test]
    fn point_to_pdf_falls_back_to_one_to_one_for_an_unusable_scale() {
        assert_eq!(point_to_pdf(30.0, 92.0, 792.0, 0.0), (30.0, 700.0));
        assert_eq!(point_to_pdf(30.0, 92.0, 792.0, f64::NAN), (30.0, 700.0));
    }

    #[test]
    fn an_inverted_range_yields_nothing_rather_than_panicking() {
        // Only reachable from a caller that built a range without
        // `caret_range`; the bounds come from variables because a literal
        // `3..1` reads as a typo to both a reviewer and clippy.
        let (start, end) = (3, 1);

        assert_eq!(two_lines().text_in(start..end), "");
    }
}
