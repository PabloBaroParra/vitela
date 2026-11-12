//! The text editor drawn to look like the text it retypes ("camouflaged"):
//! the run's own font, size, colour, letter spacing and leading, its first
//! baseline on the original baseline, over a box painted in the page's own
//! colour so the original runs are hidden while the box is open.
//!
//! Three questions, answered apart:
//!
//! - **what the text looks like** — [`look_for`], from the core's read of the
//!   run (`pdf_save::text_run_style_of`) and its embedded font
//!   ([`fonts`]); and the page's colour behind it ([`background`]);
//! - **how GTK is told** — [`Camouflage`], a CSS provider scoped to one
//!   class on one editor, removed again when the editor goes;
//! - **where the box goes** — [`box_geometry`], pure arithmetic over the
//!   look and the font's metrics, so the first baseline lands where the
//!   page has it whatever the font's ascent.
//!
//! The box's size is computed here from Pango, never read back from the
//! widget's style: the editor is measured before it is on screen, when GTK
//! may not have applied the class yet.

mod background;
mod fonts;

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Weak};

use gtk::{gdk, pango, prelude::*, CssProvider};
use pdf_document::{PageId, Rect, TextRun};
use pdf_edit::Alignment;
use pdf_manip::LopdfDocument;

use crate::app::state::DocumentSession;

#[cfg(test)]
pub(crate) use fonts::can_register;
pub(crate) use fonts::{remove_font_files, FontChoice};

/// The fraction of a run's box height above its baseline — the parser's
/// convention for a run box (`pdf_edit`'s fallback ascent of 0.75, against
/// a descent of 0.25), which is how a baseline is read back off a box.
const BOX_ASCENT: f64 = 0.75;

/// Pixels added to the box's width beyond the block's, so a line exactly
/// as wide as the block is not wrapped by a font a hair wider than the PDF's
/// metrics, and the caret after its last letter is not clipped.
const WIDTH_SLACK_PX: f64 = 3.0;

/// The selection behind typed text: the theme's blue, faint enough that the
/// text keeps its own colour on top of it.
const SELECTION: &str = "rgba(53, 132, 228, 0.30)";

/// What the text the editor shows has to look like, in page space.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditorLook {
    pub(crate) font: FontChoice,
    /// The size the page shows the text at, in points.
    pub(crate) size_pt: f64,
    /// Fill colour, RGB in `0.0..=1.0`.
    pub(crate) colour: [f64; 3],
    /// `Tc`, in page-space points.
    pub(crate) letter_spacing_pt: f64,
    /// Baseline to baseline, in points: a paragraph's leading, `None` for a
    /// single line.
    pub(crate) leading_pt: Option<f64>,
    pub(crate) alignment: Alignment,
    /// The page's colour behind the block, RGB in `0.0..=1.0`.
    pub(crate) background: [f64; 3],
    /// The block's box, in page space.
    pub(crate) block: Rect,
    /// The block's first baseline, in page space.
    pub(crate) first_baseline: f64,
}

impl EditorLook {
    /// The font the look asks Pango for at `scale` pixels a point.
    pub(crate) fn font_description(&self, scale: f64) -> pango::FontDescription {
        let mut description = pango::FontDescription::new();
        description.set_family(&self.font.families.join(","));
        description.set_absolute_size(self.size_pt * scale * f64::from(pango::SCALE));
        if self.font.bold {
            description.set_weight(pango::Weight::Bold);
        }
        if self.font.italic {
            description.set_style(pango::Style::Italic);
        }
        description
    }

    /// The Pango attributes the CSS sets on the view, for laying the text
    /// out the same way when measuring it.
    pub(crate) fn attributes(&self, scale: f64) -> pango::AttrList {
        let attributes = pango::AttrList::new();
        let letter = (self.letter_spacing_pt * scale * f64::from(pango::SCALE)).round() as i32;
        if letter != 0 {
            attributes.insert(pango::AttrInt::new_letter_spacing(letter));
        }
        if let Some(line) = self.line_height_px(scale) {
            attributes.insert(pango::AttrInt::new_line_height_absolute(
                (line * f64::from(pango::SCALE)).round() as i32,
            ));
        }
        attributes
    }

    pub(crate) fn line_height_px(&self, scale: f64) -> Option<f64> {
        self.leading_pt.map(|leading| leading * scale)
    }

    /// The justification a `TextView` needs to align lines as the block does.
    pub(crate) fn justification(&self) -> gtk::Justification {
        match self.alignment {
            Alignment::Left => gtk::Justification::Left,
            Alignment::Center => gtk::Justification::Center,
            Alignment::Right => gtk::Justification::Right,
        }
    }

    /// The CSS that draws `class`'s editor in this look at `scale`.
    pub(crate) fn css(&self, class: &str, scale: f64) -> String {
        let families = self
            .font
            .families
            .iter()
            .map(|family| css_family(family))
            .collect::<Vec<_>>()
            .join(", ");
        let colour = css_rgb(self.colour);
        let background = css_rgb(self.background);
        let line_height = self
            .line_height_px(scale)
            .map(|line| format!("line-height: {line:.3}px;"))
            .unwrap_or_default();
        format!(
            ".{class}, .{class} text, .{class} textview, .{class} textview > text {{
  font-family: {families};
  font-size: {size:.3}px;
  font-weight: {weight};
  font-style: {style};
  letter-spacing: {letter:.3}px;
  {line_height}
  color: {colour};
  caret-color: {colour};
  background-color: {background};
  background-image: none;
  border: none;
  border-radius: 0;
  box-shadow: none;
  outline: none;
  min-height: 0;
  min-width: 0;
  padding: 0;
  margin: 0;
}}
.{class} selection, .{class} text selection, .{class} textview > text selection {{
  background-color: {SELECTION};
  color: {colour};
}}
.{class} scrollbar {{ opacity: 0; }}
.{class} undershoot, .{class} overshoot {{ background: none; }}
",
            size = self.size_pt * scale,
            weight = if self.font.bold { 700 } else { 400 },
            style = if self.font.italic { "italic" } else { "normal" },
            letter = self.letter_spacing_pt * scale,
        )
    }
}

/// A family name as a CSS string, stripped of anything that could end it.
fn css_family(family: &str) -> String {
    match family {
        "sans-serif" | "serif" | "monospace" => family.to_string(),
        _ => {
            let safe: String = family
                .chars()
                .filter(|c| c.is_alphanumeric() || " -+_.".contains(*c))
                .collect();
            format!("\"{safe}\"")
        }
    }
}

fn css_rgb([r, g, b]: [f64; 3]) -> String {
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("rgb({}, {}, {})", channel(r), channel(g), channel(b))
}

/// A font's vertical metrics at the size the look draws it, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineMetrics {
    pub(crate) ascent_px: f64,
    pub(crate) descent_px: f64,
}

impl LineMetrics {
    /// What `context` resolves `look`'s font to at `scale`.
    pub(crate) fn of(context: &pango::Context, look: &EditorLook, scale: f64) -> Self {
        let metrics = context.metrics(Some(&look.font_description(scale)), None);
        Self {
            ascent_px: f64::from(metrics.ascent()) / f64::from(pango::SCALE),
            descent_px: f64::from(metrics.descent()) / f64::from(pango::SCALE),
        }
    }
}

/// How far below the top of a laid-out line its baseline sits.
///
/// With no line height that is the font's ascent. With one, Pango shares
/// the difference from the font's own height evenly above and below the
/// line, so half of it moves the baseline — down for a loose leading, up
/// for a tight one.
pub(crate) fn first_baseline_offset(metrics: LineMetrics, line_height_px: Option<f64>) -> f64 {
    let natural = metrics.ascent_px + metrics.descent_px;
    match line_height_px {
        Some(line) => metrics.ascent_px + (line - natural) / 2.0,
        None => metrics.ascent_px,
    }
}

/// Where the editor box goes, in page space, and its size in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BoxGeometry {
    /// The box in page space: its top-left is where the frame is anchored.
    pub(crate) placed: Rect,
    pub(crate) width_px: f64,
    pub(crate) height_px: f64,
}

impl BoxGeometry {
    /// The same box, grown to at least `width_px` by `height_px` at `scale`:
    /// rightwards and downwards, so its top-left — and with it the first
    /// baseline — stays where it was.
    pub(crate) fn grown_to(self, width_px: f64, height_px: f64, scale: f64) -> Self {
        let scale = scale.max(f64::EPSILON);
        let width_px = self.width_px.max(width_px.ceil());
        let height_px = self.height_px.max(height_px.ceil());
        let top = self.placed.y + self.placed.height;
        Self {
            placed: Rect {
                x: self.placed.x,
                y: top - height_px / scale,
                width: width_px / scale,
                height: height_px / scale,
            },
            width_px,
            height_px,
        }
    }
}

/// The width a line of `text_width_px` needs, caret and rounding included.
pub(crate) fn line_width_px(text_width_px: f64) -> f64 {
    text_width_px + WIDTH_SLACK_PX
}

/// The box that puts `look`'s first baseline on the block's at `scale`,
/// with its left edge on the block's, as wide as the block, and tall enough
/// for both the block (so the original lines stay covered) and
/// `text_height_px` of laid-out text.
pub(crate) fn box_geometry(
    look: &EditorLook,
    scale: f64,
    metrics: LineMetrics,
    text_height_px: f64,
) -> BoxGeometry {
    let scale = scale.max(f64::EPSILON);
    let offset = first_baseline_offset(metrics, look.line_height_px(scale));
    let top = look.first_baseline + offset / scale;
    let width_px =
        (look.block.width * scale + WIDTH_SLACK_PX + look.letter_spacing_pt.max(0.0) * scale)
            .ceil();
    let covering = (top - look.block.y) * scale;
    let height_px = covering.max(text_height_px).ceil().max(1.0);
    BoxGeometry {
        placed: Rect {
            x: look.block.x,
            y: top - height_px / scale,
            width: width_px / scale,
            height: height_px / scale,
        },
        width_px,
        height_px,
    }
}

/// The editor's look applied: a CSS provider on the display, matching one
/// class no other widget carries, removed again when this drops — which is
/// when the editor leaves the session.
pub(crate) struct Camouflage {
    pub(crate) look: EditorLook,
    provider: CssProvider,
    display: gdk::Display,
    class: String,
    /// The zoom the stylesheet draws at ([`Camouflage::restyle_at`]).
    scale: Cell<f64>,
}

thread_local! {
    static NEXT_CLASS: Cell<u64> = const { Cell::new(0) };
}

impl Camouflage {
    /// Draws `widget` (the editor's outer widget) in `look` at `scale`.
    pub(crate) fn apply(look: EditorLook, widget: &impl IsA<gtk::Widget>, scale: f64) -> Self {
        let class = NEXT_CLASS.with(|next| {
            let id = next.get();
            next.set(id + 1);
            format!("vitela-camouflage-{id}")
        });
        let provider = CssProvider::new();
        provider.load_from_data(&look.css(&class, scale));
        let display = widget.display();
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            // Above the application's own stylesheet, which styles entries.
            gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
        );
        widget.add_css_class(&class);
        Self {
            look,
            provider,
            display,
            class,
            scale: Cell::new(scale),
        }
    }

    /// The provider and the stylesheet to load into it to draw at `scale`,
    /// when the editor is not drawn at it yet — a zoom since it opened, so
    /// the text is drawn at the zoom its box is measured at. Loaded by the
    /// caller once it holds no borrow of the session.
    pub(crate) fn restyle_at(&self, scale: f64) -> Option<(CssProvider, String)> {
        (self.scale.replace(scale) != scale)
            .then(|| (self.provider.clone(), self.look.css(&self.class, scale)))
    }
}

impl Drop for Camouflage {
    fn drop(&mut self) {
        gtk::style_context_remove_provider_for_display(&self.display, &self.provider);
    }
}

/// The font family a document's font resource resolved to, per
/// (document, page, resource), so opening the editor over the same font
/// again neither extracts its program nor registers it a second time. The
/// document is held weakly: one dropped takes its entries' meaning with it.
type FamilyCacheEntry = (Weak<LopdfDocument>, PageId, String, Option<String>);

thread_local! {
    static FAMILIES: RefCell<Vec<FamilyCacheEntry>> = const { RefCell::new(Vec::new()) };
}

/// How the text of `run` — the stand-in for `block` when there is one —
/// has to look on page `page_index`, read through the session's document.
/// Whatever the core cannot say (a run only a pending insertion holds)
/// falls back to the parser's own reading of the box: black, as tall as it
/// is, in a family matched from the font's name.
pub(crate) fn look_for(
    session: &DocumentSession,
    page_index: usize,
    run: &TextRun,
    block: &[TextRun],
    font_map: &pango::FontMap,
) -> EditorLook {
    let members: Vec<TextRun> = if block.is_empty() {
        vec![run.clone()]
    } else {
        block.to_vec()
    };
    let grouped = pdf_edit::group_text_runs(&members).into_iter().next();
    let (bbox, leading, alignment, first_baseline, first) = match &grouped {
        Some(grouped) => {
            let first_id = grouped.lines.first().and_then(|line| line.runs.first());
            let first = members
                .iter()
                .find(|member| Some(&member.id) == first_id)
                .unwrap_or(&members[0])
                .clone();
            let baseline = grouped
                .lines
                .first()
                .map(|line| line.baseline)
                .unwrap_or(first.bbox.y + first.bbox.height * (1.0 - BOX_ASCENT));
            (
                grouped.bbox,
                grouped.leading.filter(|_| grouped.lines.len() > 1),
                grouped.alignment,
                baseline,
                first,
            )
        }
        None => (
            run.bbox,
            None,
            Alignment::Left,
            run.bbox.y + run.bbox.height * (1.0 - BOX_ASCENT),
            run.clone(),
        ),
    };

    let read = read_style(session, &first, font_map);
    let background = session
        .pages
        .get(page_index)
        .and_then(|page| {
            let texture = page.picture.paintable()?.downcast::<gdk::Texture>().ok()?;
            background::sample(&texture, bbox, page.placement())
        })
        .unwrap_or(background::PAPER);

    EditorLook {
        font: fonts::choose(&read.base_font, read.family),
        size_pt: read.size_pt.unwrap_or(first.bbox.height),
        colour: read.colour,
        letter_spacing_pt: read.letter_spacing_pt,
        leading_pt: leading,
        alignment,
        background,
        block: bbox,
        first_baseline,
    }
}

/// What the core could say about a run's look.
struct ReadStyle {
    base_font: String,
    family: Option<String>,
    size_pt: Option<f64>,
    colour: [f64; 3],
    letter_spacing_pt: f64,
}

fn read_style(session: &DocumentSession, run: &TextRun, font_map: &pango::FontMap) -> ReadStyle {
    let mut read = ReadStyle {
        base_font: String::new(),
        family: None,
        size_pt: None,
        colour: [0.0; 3],
        letter_spacing_pt: 0.0,
    };
    let (Some(document), Some(backing)) = (
        session.document_model.as_ref(),
        session.save_backing.as_ref(),
    ) else {
        return read;
    };
    let sources: Vec<_> = session
        .imported_sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect();
    let sources = pdf_save::ImportedSources::new(&sources);

    match pdf_save::text_run_style_of(document, &backing.base, sources, run) {
        Ok(Some(style)) => {
            read.base_font = style.base_font;
            read.size_pt = Some(style.font_size);
            read.colour = style.fill_rgb;
            read.letter_spacing_pt = style.char_spacing;
        }
        _ => {
            read.base_font =
                pdf_save::page_font_families_of(document, run.page, &backing.base, sources)
                    .ok()
                    .and_then(|families| families.get(&run.resource_font_name).cloned())
                    .unwrap_or_default();
        }
    }

    read.family = cached_family(&backing.base, run).unwrap_or_else(|| {
        let family = pdf_save::text_run_font_program_of(document, &backing.base, sources, run)
            .ok()
            .flatten()
            .and_then(|program| fonts::register(font_map, &program));
        remember_family(&backing.base, run, family.clone());
        family
    });
    read
}

/// The family cached for `run`'s font in `base`: `Some(None)` when it is
/// known to have none, `None` when it has not been looked up.
fn cached_family(base: &Arc<LopdfDocument>, run: &TextRun) -> Option<Option<String>> {
    FAMILIES.with(|families| {
        let mut families = families.borrow_mut();
        families.retain(|(document, ..)| document.strong_count() > 0);
        families
            .iter()
            .find(|(document, page, resource, _)| {
                std::ptr::eq(document.as_ptr(), Arc::as_ptr(base))
                    && *page == run.page
                    && *resource == run.resource_font_name
            })
            .map(|(.., family)| family.clone())
    })
}

fn remember_family(base: &Arc<LopdfDocument>, run: &TextRun, family: Option<String>) {
    FAMILIES.with(|families| {
        families.borrow_mut().push((
            Arc::downgrade(base),
            run.page,
            run.resource_font_name.clone(),
            family,
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> EditorLook {
        EditorLook {
            font: FontChoice {
                families: vec!["Vera".to_string(), "sans-serif".to_string()],
                bold: false,
                italic: false,
            },
            size_pt: 9.0,
            colour: [0.1, 0.2, 0.55],
            letter_spacing_pt: 0.3,
            leading_pt: Some(11.0),
            alignment: Alignment::Left,
            background: [1.0, 1.0, 1.0],
            block: Rect {
                x: 72.0,
                y: 615.0,
                width: 250.0,
                height: 47.0,
            },
            first_baseline: 660.0,
        }
    }

    #[test]
    fn with_no_line_height_the_baseline_sits_one_ascent_down() {
        let metrics = LineMetrics {
            ascent_px: 18.0,
            descent_px: 4.0,
        };
        assert_eq!(first_baseline_offset(metrics, None), 18.0);
    }

    /// A leading looser than the font's own height pushes the first
    /// baseline down by half the difference; a tighter one pulls it up.
    #[test]
    fn a_line_height_moves_the_baseline_by_half_its_difference() {
        let metrics = LineMetrics {
            ascent_px: 18.0,
            descent_px: 4.0,
        };
        assert_eq!(first_baseline_offset(metrics, Some(30.0)), 22.0);
        assert_eq!(first_baseline_offset(metrics, Some(20.0)), 17.0);
    }

    /// The box's top is the first baseline plus the offset, so the first
    /// line's baseline lands exactly on the page's, at any zoom.
    #[test]
    fn the_box_puts_the_first_baseline_on_the_pages() {
        let look = look();
        let metrics = LineMetrics {
            ascent_px: 16.6,
            descent_px: 4.2,
        };
        for scale in [2.0, 6.0] {
            let geometry = box_geometry(&look, scale, metrics, 10.0);
            let top = geometry.placed.y + geometry.placed.height;
            let line = look.leading_pt.unwrap() * scale;
            let baseline = top - first_baseline_offset(metrics, Some(line)) / scale;
            assert!((baseline - look.first_baseline).abs() < 1e-9, "{baseline}");
            assert_eq!(geometry.placed.x, look.block.x);
            assert!(geometry.width_px >= look.block.width * scale);
        }
    }

    /// Tall enough to cover every original line, and taller still when the
    /// typed text needs it.
    #[test]
    fn the_box_covers_the_block_and_grows_with_the_text() {
        let look = look();
        let metrics = LineMetrics {
            ascent_px: 16.6,
            descent_px: 4.2,
        };
        let short = box_geometry(&look, 2.0, metrics, 10.0);
        assert!(short.placed.y <= look.block.y + 1e-9, "{:?}", short.placed);

        let tall = box_geometry(&look, 2.0, metrics, 400.0);
        assert_eq!(tall.height_px, 400.0);
        assert!(tall.placed.y < short.placed.y);
    }

    /// A line typed past the run's width widens the box to the right and a
    /// taller font grows it down; the top-left corner never moves.
    #[test]
    fn a_grown_box_keeps_its_top_left_corner() {
        let look = look();
        let metrics = LineMetrics {
            ascent_px: 16.6,
            descent_px: 4.2,
        };
        let opened = box_geometry(&look, 2.0, metrics, 10.0);
        let grown = opened.grown_to(opened.width_px + 100.0, opened.height_px + 8.0, 2.0);

        assert_eq!(grown.width_px, opened.width_px + 100.0);
        assert_eq!(grown.placed.x, opened.placed.x);
        let top = |geometry: BoxGeometry| geometry.placed.y + geometry.placed.height;
        assert!((top(grown) - top(opened)).abs() < 1e-9);
        assert_eq!(opened.grown_to(1.0, 1.0, 2.0), opened, "never shrinks");
    }

    #[test]
    fn the_css_carries_the_size_colour_spacing_and_leading_at_the_zoom() {
        let css = look().css("probe", 2.0);
        assert!(css.contains("font-family: \"Vera\", sans-serif;"), "{css}");
        assert!(css.contains("font-size: 18.000px;"), "{css}");
        assert!(css.contains("color: rgb(26, 51, 140);"), "{css}");
        assert!(css.contains("letter-spacing: 0.600px;"), "{css}");
        assert!(css.contains("line-height: 22.000px;"), "{css}");
        assert!(
            css.contains("background-color: rgb(255, 255, 255);"),
            "{css}"
        );
    }

    #[test]
    fn a_family_cannot_break_out_of_its_css_string() {
        let quoted = css_family("Evil\"; } * { color: red");
        let inner = &quoted[1..quoted.len() - 1];
        assert!(!inner.contains(['"', '}', ';']), "{quoted}");
        assert_eq!(css_family("serif"), "serif");
    }

    #[test]
    fn alignment_maps_to_justification() {
        let mut look = look();
        look.alignment = Alignment::Right;
        assert_eq!(look.justification(), gtk::Justification::Right);
        look.alignment = Alignment::Center;
        assert_eq!(look.justification(), gtk::Justification::Center);
    }
}
