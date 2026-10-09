//! A widget's frame: the `/MK` background and border a foreign form draws
//! around a field (ISO 32000-1 12.5.6.19, table 189; border from `/BS`,
//! table 166).
//!
//! The model does not carry it — no command changes it, and a field Vitela
//! authors has none — so it is read off the widget dictionary at the moment
//! `pdf-save` regenerates that widget's `/AP`, and painted underneath the
//! appearance [`crate::build_field_appearance`] builds. Without it, editing a
//! field of someone else's form erased its box.

use crate::da::format_number;
use lopdf::{Dictionary, Object, Stream};

/// What a widget's `/MK` and `/BS` say to paint under its content.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetFrame {
    /// `/MK /BG`, as its colour components: 1 gray, 3 RGB, 4 CMYK.
    pub background: Option<Vec<f64>>,
    /// `/MK /BC`, with a positive width to stroke it at.
    pub border: Option<Vec<f64>>,
    pub border_width: f64,
    pub border_style: BorderStyle,
}

/// The `/BS /S` styles drawn differently. Beveled and inset are drawn solid:
/// their shading is a viewer's 3-D effect, not something a form depends on.
#[derive(Debug, Clone, PartialEq)]
pub enum BorderStyle {
    Solid,
    /// `/D`, the dash array.
    Dashed(Vec<f64>),
    Underline,
}

/// The outline the frame follows: a box, or the circle a radio button
/// conventionally draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameShape {
    Box,
    Circle,
}

impl WidgetFrame {
    /// The frame `widget` declares, or `None` when it paints nothing.
    /// `resolve` follows an indirect reference; a direct object resolves to
    /// itself.
    pub fn from_widget<'a>(
        widget: &'a Dictionary,
        resolve: impl Fn(&'a Object) -> Option<&'a Object>,
    ) -> Option<Self> {
        let dict = |key: &[u8]| {
            widget
                .get(key)
                .ok()
                .and_then(&resolve)
                .and_then(|object| object.as_dict().ok())
        };
        let mk = dict(b"MK")?;
        let colour = |key: &[u8]| {
            let components: Vec<f64> = mk
                .get(key)
                .ok()
                .and_then(&resolve)
                .and_then(|object| object.as_array().ok())?
                .iter()
                .filter_map(|component| resolve(component).and_then(number))
                .collect();
            // Table 189: 0 components is transparent; 2 is not a colour space.
            matches!(components.len(), 1 | 3 | 4).then_some(components)
        };

        let bs = dict(b"BS");
        let border_width = bs
            .and_then(|bs| bs.get(b"W").ok())
            .and_then(&resolve)
            .and_then(number)
            .unwrap_or(1.0);
        let border_style = match bs
            .and_then(|bs| bs.get(b"S").ok())
            .and_then(|style| style.as_name().ok())
        {
            Some(b"D") => BorderStyle::Dashed(
                bs.and_then(|bs| bs.get(b"D").ok())
                    .and_then(&resolve)
                    .and_then(|dash| dash.as_array().ok())
                    .map(|dash| dash.iter().filter_map(number).collect())
                    .unwrap_or_else(|| vec![3.0]),
            ),
            Some(b"U") => BorderStyle::Underline,
            _ => BorderStyle::Solid,
        };

        let frame = WidgetFrame {
            background: colour(b"BG"),
            border: colour(b"BC").filter(|_| border_width > 0.0),
            border_width,
            border_style,
        };
        (frame.background.is_some() || frame.border.is_some()).then_some(frame)
    }

    /// Paints this frame under `stream`'s content, sized from its `/BBox`.
    pub fn paint_under(&self, stream: &mut Stream, shape: FrameShape) {
        let (width, height) = bbox_size(&stream.dict);
        let mut frame = String::from("q\n");
        if let Some(background) = &self.background {
            frame.push_str(&colour_operator(background, false));
            frame.push_str(&outline(shape, width, height, 0.0));
            frame.push_str("f\n");
        }
        if let Some(border) = &self.border {
            let half = self.border_width / 2.0;
            frame.push_str(&colour_operator(border, true));
            frame.push_str(&format!("{} w\n", format_number(self.border_width)));
            match &self.border_style {
                BorderStyle::Underline => frame.push_str(&format!(
                    "0 {y} m\n{w} {y} l\n",
                    y = format_number(half),
                    w = format_number(width),
                )),
                style => {
                    if let BorderStyle::Dashed(dash) = style {
                        let dash: Vec<String> = dash.iter().map(|d| format_number(*d)).collect();
                        frame.push_str(&format!("[{}] 0 d\n", dash.join(" ")));
                    }
                    frame.push_str(&outline(shape, width, height, half));
                }
            }
            frame.push_str("S\n");
        }
        frame.push_str("Q\n");

        let mut content = frame.into_bytes();
        content.extend_from_slice(&stream.content);
        stream.set_content(content);
    }
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(*value as f64),
        _ => None,
    }
}

fn bbox_size(dict: &Dictionary) -> (f64, f64) {
    let corners: Vec<f64> = dict
        .get(b"BBox")
        .and_then(Object::as_array)
        .map(|bbox| bbox.iter().filter_map(number).collect())
        .unwrap_or_default();
    match corners[..] {
        [x0, y0, x1, y1] => ((x1 - x0).abs(), (y1 - y0).abs()),
        _ => (0.0, 0.0),
    }
}

/// `g`/`rg`/`k` for a fill, `G`/`RG`/`K` for a stroke, by component count.
fn colour_operator(components: &[f64], stroke: bool) -> String {
    let operator = match (components.len(), stroke) {
        (1, false) => "g",
        (1, true) => "G",
        (3, false) => "rg",
        (3, true) => "RG",
        (_, false) => "k",
        (_, true) => "K",
    };
    let operands: Vec<String> = components.iter().map(|c| format_number(*c)).collect();
    format!("{} {operator}\n", operands.join(" "))
}

/// The shape's path, `inset` points in from every edge — half the stroke
/// width, so the whole border lands inside the widget.
fn outline(shape: FrameShape, width: f64, height: f64, inset: f64) -> String {
    match shape {
        FrameShape::Box => format!(
            "{i} {i} {w} {h} re\n",
            i = format_number(inset),
            w = format_number(width - 2.0 * inset),
            h = format_number(height - 2.0 * inset),
        ),
        FrameShape::Circle => circle_path(
            width / 2.0,
            height / 2.0,
            (width.min(height) / 2.0 - inset).max(0.0),
        ),
    }
}

/// A closed circle path of radius `r` around `(cx, cy)`: the usual four
/// Béziers, control points 0.5523·r out. Shared with the radio dot
/// (`appearance::dot_stream`) so the two circles are drawn the same way.
pub(crate) fn circle_path(cx: f64, cy: f64, r: f64) -> String {
    let k = 0.552_284_75 * r;
    let p = |x: f64, y: f64| format!("{} {}", format_number(x), format_number(y));
    format!(
        "{} m
{} {} {} c
{} {} {} c
{} {} {} c
{} {} {} c
",
        p(cx + r, cy),
        p(cx + r, cy + k),
        p(cx + k, cy + r),
        p(cx, cy + r),
        p(cx - k, cy + r),
        p(cx - r, cy + k),
        p(cx - r, cy),
        p(cx - r, cy - k),
        p(cx - k, cy - r),
        p(cx, cy - r),
        p(cx + k, cy - r),
        p(cx + r, cy - k),
        p(cx + r, cy),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(object: &Object) -> Option<&Object> {
        Some(object)
    }

    fn reals(values: &[f32]) -> Object {
        Object::Array(values.iter().map(|value| Object::Real(*value)).collect())
    }

    /// reportlab's widget: white background, dark-gray border, `/BS /W 1`.
    fn reportlab_widget() -> Dictionary {
        let mut mk = Dictionary::new();
        mk.set("BC", reals(&[0.2, 0.2, 0.2]));
        mk.set("BG", Object::Array(vec![1.into(), 1.into(), 1.into()]));
        let mut bs = Dictionary::new();
        bs.set("S", "S");
        bs.set("W", 1);
        let mut widget = Dictionary::new();
        widget.set("MK", mk);
        widget.set("BS", bs);
        widget
    }

    fn stream(width: i64, height: i64, content: &str) -> Stream {
        let mut dict = Dictionary::new();
        dict.set(
            "BBox",
            Object::Array(vec![0.into(), 0.into(), width.into(), height.into()]),
        );
        Stream::new(dict, content.as_bytes().to_vec())
    }

    fn content(stream: &Stream) -> String {
        String::from_utf8(stream.content.clone()).unwrap()
    }

    #[test]
    fn reads_reportlabs_background_and_border() {
        let frame = WidgetFrame::from_widget(&reportlab_widget(), resolve).expect("a frame");
        assert_eq!(frame.background, Some(vec![1.0, 1.0, 1.0]));
        let border = frame.border.expect("a border");
        assert!(border.iter().all(|channel| (channel - 0.2).abs() < 1e-6));
        assert_eq!(frame.border_width, 1.0);
        assert_eq!(frame.border_style, BorderStyle::Solid);
    }

    #[test]
    fn a_widget_without_mk_has_no_frame() {
        assert_eq!(WidgetFrame::from_widget(&Dictionary::new(), resolve), None);
    }

    /// An empty colour array is "transparent" (table 189), and a zero
    /// `/BS /W` strokes nothing — neither is a frame.
    #[test]
    fn transparent_colours_and_a_zero_width_paint_nothing() {
        let mut widget = reportlab_widget();
        let mk = widget.get_mut(b"MK").unwrap().as_dict_mut().unwrap();
        mk.set("BG", Object::Array(vec![]));
        let bs = widget.get_mut(b"BS").unwrap().as_dict_mut().unwrap();
        bs.set("W", 0);
        assert_eq!(WidgetFrame::from_widget(&widget, resolve), None);
    }

    /// No `/BS` means the default: a 1pt solid border.
    #[test]
    fn a_border_colour_without_bs_strokes_one_point_solid() {
        let mut widget = reportlab_widget();
        widget.remove(b"BS");
        let frame = WidgetFrame::from_widget(&widget, resolve).expect("a frame");
        assert_eq!(frame.border_width, 1.0);
        assert_eq!(frame.border_style, BorderStyle::Solid);
    }

    #[test]
    fn follows_an_indirect_mk() {
        let referenced = Object::Dictionary(
            reportlab_widget()
                .get(b"MK")
                .unwrap()
                .as_dict()
                .unwrap()
                .clone(),
        );
        let mut widget = reportlab_widget();
        widget.set("MK", Object::Reference((7, 0)));
        let frame = WidgetFrame::from_widget(&widget, |object| match object {
            Object::Reference((7, 0)) => Some(&referenced),
            other => Some(other),
        });
        assert!(frame.expect("a frame").border.is_some());
    }

    #[test]
    fn dashed_and_underline_styles_are_read() {
        let mut widget = reportlab_widget();
        let bs = widget.get_mut(b"BS").unwrap().as_dict_mut().unwrap();
        bs.set("S", "D");
        bs.set("D", Object::Array(vec![2.into(), 1.into()]));
        let frame = WidgetFrame::from_widget(&widget, resolve).unwrap();
        assert_eq!(frame.border_style, BorderStyle::Dashed(vec![2.0, 1.0]));

        let bs = widget.get_mut(b"BS").unwrap().as_dict_mut().unwrap();
        bs.set("S", "U");
        let frame = WidgetFrame::from_widget(&widget, resolve).unwrap();
        assert_eq!(frame.border_style, BorderStyle::Underline);
    }

    /// The frame goes *under* the content, inside its own `q`/`Q`, and the
    /// border sits half its width in so the whole stroke is inside the box —
    /// the same geometry reportlab's own appearance uses.
    #[test]
    fn paints_a_box_under_the_existing_content() {
        let frame = WidgetFrame::from_widget(&reportlab_widget(), resolve).unwrap();
        let mut box_stream = stream(18, 18, "BT (x) Tj ET");
        frame.paint_under(&mut box_stream, FrameShape::Box);
        let painted = content(&box_stream);

        assert!(painted.starts_with("q\n"), "{painted}");
        assert!(painted.ends_with("Q\nBT (x) Tj ET"), "{painted}");
        assert!(painted.contains("1 1 1 rg\n0 0 18 18 re\nf\n"), "{painted}");
        assert!(
            painted.contains("0.2 0.2 0.2 RG\n1 w\n0.5 0.5 17 17 re\nS\n"),
            "{painted}"
        );
    }

    #[test]
    fn paints_a_circle_for_a_radio_button() {
        let frame = WidgetFrame::from_widget(&reportlab_widget(), resolve).unwrap();
        let mut radio = stream(18, 18, "");
        frame.paint_under(&mut radio, FrameShape::Circle);
        let painted = content(&radio);

        assert!(!painted.contains(" re\n"), "a circle, not a box: {painted}");
        assert_eq!(
            painted.matches(" c\n").count(),
            8,
            "two four-arc circles: {painted}"
        );
        assert!(
            painted.contains("f\n") && painted.contains("S\n"),
            "{painted}"
        );
    }

    #[test]
    fn colour_operators_follow_the_component_count() {
        let mut frame = WidgetFrame::from_widget(&reportlab_widget(), resolve).unwrap();
        frame.background = Some(vec![0.5]);
        frame.border = Some(vec![0.0, 0.0, 0.0, 1.0]);
        let mut box_stream = stream(10, 10, "");
        frame.paint_under(&mut box_stream, FrameShape::Box);
        let painted = content(&box_stream);
        assert!(painted.contains("0.5 g\n"), "{painted}");
        assert!(painted.contains("0 0 0 1 K\n"), "{painted}");
    }

    #[test]
    fn dashed_and_underlined_borders() {
        let mut frame = WidgetFrame::from_widget(&reportlab_widget(), resolve).unwrap();
        frame.background = None;
        frame.border_style = BorderStyle::Dashed(vec![3.0]);
        let mut dashed = stream(10, 10, "");
        frame.paint_under(&mut dashed, FrameShape::Box);
        assert!(
            content(&dashed).contains("[3] 0 d\n"),
            "{}",
            content(&dashed)
        );

        frame.border_style = BorderStyle::Underline;
        let mut underlined = stream(10, 10, "");
        frame.paint_under(&mut underlined, FrameShape::Box);
        let painted = content(&underlined);
        assert!(painted.contains("0 0.5 m\n10 0.5 l\nS\n"), "{painted}");
        assert!(!painted.contains(" re\n"), "{painted}");
    }
}
