//! Bold and italic faked on the run's own font, for a page that has no real
//! sibling to switch to.
//!
//! - **Bold** is the glyph outline stroked as well as filled (`2 Tr`), in the
//!   fill colour, about 3% of the font size thick. Producers fake bold the
//!   same way, which is why a stroked run already in a file reads as bold.
//! - **Italic** is a shear of about 12° in the text matrix, around the run's
//!   own origin, so the baseline and the start of the run stay put.
//!
//! Both happen inside the text object, where `q`/`Q` are not allowed, so
//! every parameter they change is set back explicitly right after the run:
//! the stroke colour, line width and render mode from the run's own paint
//! state, and the text matrix from its line matrix plus the advance.

use crate::edit::format_number;
use crate::parse::matrix::Matrix;
use crate::parse::tokenize;

/// How thick a stroked bold is, as a fraction of the font size.
pub(crate) const BOLD_STROKE: f64 = 0.03;

/// `tan 12°`: the slant of a synthesized italic.
pub(crate) const ITALIC_SHEAR: f64 = 0.212_556_56;

/// The smallest shear read as a slant (about 3°); anything less is noise in
/// a producer's matrix, not italic.
pub(crate) const SHEAR_THRESHOLD: f64 = 0.05;

/// The render mode that fills and strokes.
pub(crate) const FILL_AND_STROKE: u8 = 2;

/// How far the text matrix leans: the `x` it gains per unit of `y`, read off
/// the angle between its two axes.
pub(crate) fn shear_of(matrix: Matrix) -> f64 {
    let across = matrix.a * matrix.a + matrix.b * matrix.b;
    if across < 1e-12 {
        return 0.0;
    }
    (matrix.a * matrix.c + matrix.b * matrix.d) / across
}

/// `matrix` leaning by `shear` instead of whatever it leant by, around its
/// own origin: the `x` axis and the translation are untouched.
pub(crate) fn with_shear(matrix: Matrix, shear: f64) -> Matrix {
    let delta = shear - shear_of(matrix);
    Matrix {
        c: matrix.c + delta * matrix.a,
        d: matrix.d + delta * matrix.b,
        ..matrix
    }
}

/// The operators that make the glyphs fill-and-stroke in the fill colour:
/// the run's fill colour operators replayed as stroke operators, the line
/// width, and `2 Tr`.
pub(crate) fn bold_on(paint_state: &[u8], line_width: f64) -> Vec<u8> {
    let mut out = Vec::new();
    for (operator, bytes) in colour_operations(paint_state) {
        if let Some(stroke) = stroke_twin(&operator) {
            out.extend_from_slice(&bytes[..bytes.len() - operator.len()]);
            out.extend_from_slice(stroke.as_bytes());
            out.push(b' ');
        }
    }
    out.extend_from_slice(
        format!("{} w {FILL_AND_STROKE} Tr ", format_number(line_width)).as_bytes(),
    );
    out
}

/// What [`bold_on`] changed, put back as the run had it: its stroke colour,
/// its line width (`1`, the initial value, if it never set one) and its
/// render mode.
pub(crate) fn bold_restore(paint_state: &[u8], render_mode: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut width = None;
    for (operator, bytes) in colour_operations(paint_state) {
        if is_stroke_colour(&operator) {
            out.extend_from_slice(&bytes);
            out.push(b' ');
        } else if operator == "w" {
            width = Some(bytes);
        }
    }
    out.extend_from_slice(width.as_deref().unwrap_or(b"1 w"));
    out.extend_from_slice(format!(" {render_mode} Tr").as_bytes());
    out
}

/// Every operation of `paint_state` with its own bytes. The paint state is
/// this crate's own output, so a tokenizer failure can only mean an empty
/// one.
fn colour_operations(paint_state: &[u8]) -> Vec<(String, Vec<u8>)> {
    tokenize(paint_state)
        .unwrap_or_default()
        .into_iter()
        .map(|operation| {
            let bytes = paint_state[operation.span.clone()].to_vec();
            (operation.operator, bytes)
        })
        .collect()
}

fn stroke_twin(fill: &str) -> Option<&'static str> {
    Some(match fill {
        "cs" => "CS",
        "sc" => "SC",
        "scn" => "SCN",
        "g" => "G",
        "rg" => "RG",
        "k" => "K",
        _ => return None,
    })
}

fn is_stroke_colour(operator: &str) -> bool {
    matches!(operator, "CS" | "SC" | "SCN" | "G" | "RG" | "K")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shear_is_read_back_as_it_was_written() {
        let matrix = Matrix::new(2.0, 0.0, 0.0, 2.0, 100.0, 700.0);
        let leaning = with_shear(matrix, ITALIC_SHEAR);
        assert!((shear_of(leaning) - ITALIC_SHEAR).abs() < 1e-9);
        assert_eq!((leaning.e, leaning.f), (100.0, 700.0), "around the origin");
        assert!(shear_of(with_shear(leaning, 0.0)).abs() < 1e-9);
    }

    #[test]
    fn a_rotated_matrix_leans_along_its_own_axes() {
        // 90° rotation: x runs up the page.
        let rotated = Matrix::new(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
        let leaning = with_shear(rotated, ITALIC_SHEAR);
        assert!((shear_of(leaning) - ITALIC_SHEAR).abs() < 1e-9);
        assert_eq!(
            (leaning.a, leaning.b),
            (0.0, 1.0),
            "the baseline keeps its direction"
        );
    }

    #[test]
    fn bold_strokes_in_the_fill_colour_and_restores_the_stroke() {
        let paint = b"/CS0 cs 0.2 sc 1 0 0 RG 0.5 w 0 Tc 0 Tw 100 Tz 0 Ts 0 Tr ";
        let on = String::from_utf8(bold_on(paint, 0.36)).unwrap();
        assert_eq!(on, "/CS0 CS 0.2 SC 0.36 w 2 Tr ");
        let off = String::from_utf8(bold_restore(paint, 0)).unwrap();
        assert_eq!(off, "1 0 0 RG 0.5 w 0 Tr");
    }

    #[test]
    fn a_run_that_never_set_a_width_gets_the_initial_one_back() {
        let off = String::from_utf8(bold_restore(b"0 g 0 G ", 0)).unwrap();
        assert_eq!(off, "0 G 1 w 0 Tr");
    }
}
