//! Free-text layout across the boundary.
//!
//! The snapshot an annotation list carries already holds its lines, so a
//! shell drawing a stored box never calls this. It exists for the two
//! moments there is no stored box yet: a dialog previewing what the user is
//! typing, and a resize drag showing the text rewrapping live.

use pdf_annotate::{default_free_text_style, layout_free_text};

use crate::error::FfiError;
use crate::types::{FfiFreeTextLayout, FfiFreeTextLine};

impl From<pdf_annotate::FreeTextLine> for FfiFreeTextLine {
    fn from(line: pdf_annotate::FreeTextLine) -> Self {
        Self {
            text: line.text,
            x_pt: line.x_pt,
            baseline_from_top_pt: line.baseline_from_top_pt,
        }
    }
}

impl From<pdf_annotate::FreeTextLayout> for FfiFreeTextLayout {
    fn from(layout: pdf_annotate::FreeTextLayout) -> Self {
        Self {
            font_size_pt: layout.font_size_pt,
            lines: layout.lines.into_iter().map(Into::into).collect(),
            overflow: layout.overflow,
        }
    }
}

/// Lays `contents` out in a `width_pt` x `height_pt` box with the default
/// free-text style — the same function, with the same result, as the lines
/// in a stored box's snapshot and in the saved appearance stream.
///
/// Blank text is not an error here (a dialog is empty before the user types);
/// a character Helvetica cannot show is [`FfiError::EncodingGap`].
#[uniffi::export]
pub fn freetext_layout(
    contents: String,
    width_pt: f64,
    height_pt: f64,
) -> Result<FfiFreeTextLayout, FfiError> {
    Ok(layout_free_text(&contents, &default_free_text_style(), width_pt, height_pt)?.into())
}
