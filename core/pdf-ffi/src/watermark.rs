//! The "Add watermark" tool's starting point across the boundary.
//!
//! Recording a stamp is `apply_edit(FfiEditCommand::AddWatermark)`; this
//! module only hands shells the look the core starts from, so the first
//! stamp on Windows, Android and Linux is the same large grey diagonal
//! rather than three local copies of the same constants drifting apart.

use pdf_document::Watermark;

use crate::types::FfiWatermark;

/// A watermark showing `text` on the page positions `pages`, with the
/// core's default look (`pdf_document::Watermark::diagonal`).
///
/// Nothing is checked here — a dialog builds this before the user has typed
/// anything. `apply_edit` refuses what cannot be stamped.
#[uniffi::export]
pub fn default_watermark(pages: Vec<u32>, text: String) -> FfiWatermark {
    let look = Watermark::diagonal(Vec::new(), text);
    FfiWatermark {
        pages,
        text: look.text,
        font_size: look.font_size,
        color: look.color.into(),
        opacity: look.opacity,
        rotation_degrees: look.rotation_degrees,
    }
}
