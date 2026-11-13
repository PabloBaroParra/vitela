//! Underline and strikethrough: a thin filled box in the run's fill colour,
//! under or through its advance, painted in a stream of its own after the
//! page's content.
//!
//! A path cannot be painted inside a text object, so the line is not written
//! next to the show operation but appended to the page, in the run's own
//! coordinate system: the page's leftover transform cancelled, then the
//! run's CTM and text matrix, so a rotated or scaled run gets a rotated or
//! scaled line. The colour is written as device RGB because a colour-space
//! name from a form's resources means nothing at page level.
//!
//! **Not detected on the way back.** Nothing in a file ties a line to the
//! run it decorates — a producer's underline is just another path — so a
//! look read from a file never reports one, and only a line this session
//! recorded can be taken away again (by undoing it, or toggling it off before
//! the save, which folds the two into nothing). Telling a run's underline
//! apart from a table rule under it is future work.

use crate::edit::format_number;
use crate::parse::matrix::Matrix;

/// Where an underline sits and how thick it is, in fractions of the font
/// size below / above the baseline.
const UNDERLINE_TOP: f64 = -0.1;
const STRIKE_TOP: f64 = 0.3;
const THICKNESS: f64 = 0.05;

/// Which lines to draw.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Lines {
    pub(crate) underline: bool,
    pub(crate) strike: bool,
}

impl Lines {
    pub(crate) fn any(self) -> bool {
        self.underline || self.strike
    }
}

/// What the lines of one run are drawn from.
pub(crate) struct Placement {
    /// Cancels whatever CTM the page's own streams leave behind.
    pub(crate) correction: Matrix,
    pub(crate) ctm: Matrix,
    pub(crate) text_matrix: Matrix,
    pub(crate) font_size: f64,
    /// The run's advance in text space, after any font change.
    pub(crate) advance: f64,
    pub(crate) fill_rgb: [f64; 3],
}

/// `q … re f Q`: the lines `lines` asks for under / through the run.
pub(crate) fn operators(lines: Lines, at: &Placement) -> Vec<u8> {
    let mut out = b"\nq ".to_vec();
    for matrix in [at.correction, at.ctm, at.text_matrix] {
        out.extend_from_slice(crate::insert::matrix_operator(matrix).as_bytes());
        out.push(b' ');
    }
    let [r, g, b] = at.fill_rgb;
    out.extend_from_slice(
        format!(
            "{} {} {} rg ",
            format_number(r),
            format_number(g),
            format_number(b)
        )
        .as_bytes(),
    );
    let size = at.font_size.abs();
    let thickness = THICKNESS * size;
    for (wanted, top) in [(lines.underline, UNDERLINE_TOP), (lines.strike, STRIKE_TOP)] {
        if wanted {
            out.extend_from_slice(
                format!(
                    "0 {} {} {} re f ",
                    format_number(top * size - thickness),
                    format_number(at.advance),
                    format_number(thickness)
                )
                .as_bytes(),
            );
        }
    }
    out.extend_from_slice(b"Q\n");
    out
}
