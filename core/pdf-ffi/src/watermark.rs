//! The "Add watermark" tool's starting points across the boundary.
//!
//! Recording a stamp is `apply_edit(FfiEditCommand::AddWatermark)` for text
//! and `AddImageWatermark` for a picture; this module only hands shells the
//! looks the core starts from, so the first stamp on Windows, Android and
//! Linux is the same rather than three local copies of the same constants
//! drifting apart.

use pdf_document::{Watermark, WatermarkMark};

use crate::types::{FfiImageWatermark, FfiWatermark};

/// A watermark showing `text` on the page positions `pages`, with the
/// core's default look (`pdf_document::Watermark::diagonal`).
///
/// Nothing is checked here — a dialog builds this before the user has typed
/// anything. `apply_edit` refuses what cannot be stamped.
#[uniffi::export]
pub fn default_watermark(pages: Vec<u32>, text: String) -> FfiWatermark {
    let look = Watermark::diagonal(Vec::new(), text);
    let WatermarkMark::Text {
        text,
        font_size,
        color,
    } = look.mark
    else {
        unreachable!("Watermark::diagonal always builds a text mark");
    };
    FfiWatermark {
        pages,
        text,
        font_size,
        color: color.into(),
        opacity: look.opacity,
        rotation_degrees: look.rotation_degrees,
        layer: look.layer.into(),
    }
}

/// A watermark showing the picture `image` on the page positions `pages`,
/// with the core's default look for a picture
/// (`pdf_document::Watermark::picture`): upright, half the page, faded,
/// behind the content.
///
/// Nothing is checked here — not even that `image` decodes. `apply_edit`
/// refuses what cannot be stamped.
#[uniffi::export]
pub fn default_image_watermark(pages: Vec<u32>, image: Vec<u8>) -> FfiImageWatermark {
    let look = Watermark::picture(Vec::new(), image);
    let WatermarkMark::Image { bytes, scale } = look.mark else {
        unreachable!("Watermark::picture always builds an image mark");
    };
    FfiImageWatermark {
        pages,
        image: bytes,
        scale,
        opacity: look.opacity,
        rotation_degrees: look.rotation_degrees,
        layer: look.layer.into(),
    }
}
