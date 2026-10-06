//! The geometry of a signature drawn on the pad: what counts as ink, which part
//! of the pad the PNG covers, and how far it is scaled down. Pure functions and
//! plain data — no GTK, no Cairo — so every rule is testable without a display.
//!
//! The rules are the Android shell's (`DrawnSignature.kt`), deliberately: the
//! same strokes must give the same picture on every platform, and the core's
//! stamp placement sizes the stamp from this PNG's own proportions, so a crop
//! that kept empty pad around the ink would shrink the signature itself.

/// A point on the pad, in pad pixels.
pub(crate) type Point = (f64, f64);

/// One continuous line the pen drew, in order.
pub(crate) type Stroke = Vec<Point>;

/// Pen width on the pad, in pad pixels. The PNG keeps it, so the signature
/// looks the way it was drawn.
pub(crate) const STROKE_WIDTH: f64 = 3.0;

/// Longest side of the PNG, in pixels. The core places a stamp at most 144 pt
/// long, so this is already about 600 DPI — more would only grow the file.
pub(crate) const MAX_SIDE_PX: f64 = 1200.0;

/// A stroke needs two points to leave a line; a bare click draws nothing.
const MIN_POINTS_FOR_A_LINE: usize = 2;

/// Whether the pad holds anything worth turning into a signature.
pub(crate) fn has_ink(strokes: &[Stroke]) -> bool {
    strokes.iter().any(|stroke| is_a_line(stroke))
}

fn is_a_line(stroke: &[Point]) -> bool {
    stroke.len() >= MIN_POINTS_FOR_A_LINE
}

/// The part of the pad the PNG covers, in pad pixels: the ink's bounds grown by
/// half a stroke so the round caps are not clipped. `scale` shrinks it so the
/// long side fits [`MAX_SIDE_PX`]; a small signature is never enlarged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) scale: f64,
}

impl Frame {
    /// Width of the PNG, at least one pixel so a hairline still encodes.
    pub(crate) fn pixel_width(&self) -> i32 {
        pixels(self.width * self.scale)
    }

    /// Height of the PNG, at least one pixel — a perfectly straight stroke
    /// still has the pen's thickness, but a zero pen width must not break it.
    pub(crate) fn pixel_height(&self) -> i32 {
        pixels(self.height * self.scale)
    }
}

fn pixels(length: f64) -> i32 {
    (length.ceil() as i32).max(1)
}

/// The crop around the ink, or `None` while the pad holds no line. A tap (a
/// one-point stroke) does not widen the frame: it is not in the picture.
pub(crate) fn frame(strokes: &[Stroke], stroke_width: f64, max_side: f64) -> Option<Frame> {
    let mut points = strokes
        .iter()
        .filter(|stroke| is_a_line(stroke))
        .flatten()
        .copied();
    let (first_x, first_y) = points.next()?;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first_x, first_y, first_x, first_y);
    for (x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let half = stroke_width / 2.0;
    let (left, top) = (min_x - half, min_y - half);
    let (width, height) = (max_x + half - left, max_y + half - top);
    let scale = (max_side / width.max(height).max(1.0)).min(1.0);
    Some(Frame {
        left,
        top,
        width,
        height,
        scale,
    })
}

/// What the user has drawn on the pad so far: the finished strokes and the one
/// the pen is still drawing. Owns the pad's rules so the widget only forwards
/// pointer positions.
#[derive(Debug, Default)]
pub(crate) struct Pad {
    strokes: Vec<Stroke>,
    current: Stroke,
}

impl Pad {
    /// The pen came down at `point`. Anything left over from a stroke that
    /// never ended is dropped — a gesture that was cancelled leaves no ink.
    pub(crate) fn begin(&mut self, point: Point) {
        self.current.clear();
        self.current.push(point);
    }

    /// The pen moved to `point`. Ignored when the pen is not down.
    pub(crate) fn extend(&mut self, point: Point) {
        if !self.current.is_empty() {
            self.current.push(point);
        }
    }

    /// The pen came up: the stroke is kept if it left a line, dropped if it
    /// was a bare click.
    pub(crate) fn end(&mut self) {
        let finished = std::mem::take(&mut self.current);
        if is_a_line(&finished) {
            self.strokes.push(finished);
        }
    }

    /// Wipes the pad, including a stroke still being drawn.
    pub(crate) fn clear(&mut self) {
        self.strokes.clear();
        self.current.clear();
    }

    /// Whether **Use** has anything to turn into a PNG.
    pub(crate) fn has_ink(&self) -> bool {
        has_ink(&self.strokes)
    }

    /// Whether **Clear** has anything to wipe.
    pub(crate) fn is_blank(&self) -> bool {
        self.strokes.is_empty() && self.current.is_empty()
    }

    /// The finished strokes, for the PNG.
    pub(crate) fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    /// The stroke being drawn right now, for painting it live.
    pub(crate) fn current(&self) -> &[Point] {
        &self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(points: &[Point]) -> Stroke {
        points.to_vec()
    }

    #[test]
    fn a_pad_without_a_line_has_no_ink() {
        assert!(!has_ink(&[]));
        assert!(!has_ink(&[line(&[(5.0, 5.0)])]));
    }

    #[test]
    fn a_two_point_stroke_is_ink() {
        assert!(has_ink(&[line(&[(0.0, 0.0), (1.0, 1.0)])]));
    }

    #[test]
    fn no_frame_while_the_pad_holds_no_line() {
        assert_eq!(frame(&[line(&[(5.0, 5.0)])], 4.0, MAX_SIDE_PX), None);
    }

    #[test]
    fn the_frame_hugs_the_ink_plus_half_a_stroke() {
        let strokes = [
            line(&[(100.0, 50.0), (300.0, 80.0)]),
            line(&[(5.0, 5.0)]), // a tap leaves no line, so it does not widen the frame
            line(&[(150.0, 120.0), (200.0, 60.0)]),
        ];

        let frame = frame(&strokes, 8.0, MAX_SIDE_PX).expect("there is a line");

        assert_eq!(
            frame,
            Frame {
                left: 96.0,
                top: 46.0,
                width: 208.0,
                height: 78.0,
                scale: 1.0,
            }
        );
        assert_eq!((frame.pixel_width(), frame.pixel_height()), (208, 78));
    }

    #[test]
    fn a_large_signature_is_scaled_down_to_the_long_side_cap() {
        let strokes = [line(&[(0.0, 0.0), (2396.0, 596.0)])];

        let frame = frame(&strokes, 4.0, 1200.0).expect("there is a line");

        assert_eq!(frame.scale, 0.5);
        assert_eq!((frame.pixel_width(), frame.pixel_height()), (1200, 300));
    }

    #[test]
    fn a_small_signature_is_never_enlarged() {
        let strokes = [line(&[(0.0, 0.0), (10.0, 10.0)])];

        let frame = frame(&strokes, 2.0, 1200.0).expect("there is a line");

        assert_eq!(frame.scale, 1.0);
    }

    #[test]
    fn a_straight_line_still_has_a_pixel_of_height() {
        let strokes = [line(&[(0.0, 10.0), (100.0, 10.0)])];

        let frame = frame(&strokes, 0.0, 1200.0).expect("there is a line");

        assert!(frame.pixel_height() >= 1);
    }

    #[test]
    fn a_pad_keeps_a_stroke_that_left_a_line() {
        let mut pad = Pad::default();

        pad.begin((1.0, 1.0));
        pad.extend((4.0, 5.0));
        pad.end();

        assert_eq!(pad.strokes(), [line(&[(1.0, 1.0), (4.0, 5.0)])]);
        assert!(pad.has_ink());
    }

    #[test]
    fn a_bare_click_leaves_the_pad_blank() {
        let mut pad = Pad::default();

        pad.begin((1.0, 1.0));
        pad.end();

        assert!(!pad.has_ink());
        assert!(pad.is_blank());
    }

    #[test]
    fn the_stroke_in_progress_is_visible_but_is_not_ink_yet() {
        let mut pad = Pad::default();

        pad.begin((1.0, 1.0));
        pad.extend((2.0, 2.0));

        assert_eq!(pad.current(), [(1.0, 1.0), (2.0, 2.0)]);
        assert!(!pad.has_ink());
        assert!(!pad.is_blank());
    }

    #[test]
    fn moving_with_the_pen_up_draws_nothing() {
        let mut pad = Pad::default();

        pad.extend((9.0, 9.0));

        assert!(pad.is_blank());
    }

    #[test]
    fn clear_wipes_finished_strokes_and_the_one_in_progress() {
        let mut pad = Pad::default();
        pad.begin((1.0, 1.0));
        pad.extend((2.0, 2.0));
        pad.end();
        pad.begin((3.0, 3.0));

        pad.clear();

        assert!(pad.is_blank());
        assert!(!pad.has_ink());
    }

    #[test]
    fn a_new_press_drops_a_stroke_that_never_ended() {
        let mut pad = Pad::default();
        pad.begin((1.0, 1.0));
        pad.extend((2.0, 2.0));

        pad.begin((7.0, 7.0));
        pad.extend((8.0, 8.0));
        pad.end();

        assert_eq!(pad.strokes(), [line(&[(7.0, 7.0), (8.0, 8.0)])]);
    }
}
