//! Render-time options and scheduling priority (T-016, T-015).
//!
//! `RenderOptions` mirrors `design.md`'s `RenderOptions { invert_content_colors, .. }`
//! shape. It intentionally lives in this crate rather than being folded into
//! the Batch 0 port stub's signature — see `error.rs` module
//! docs for why B3 does not touch `pdf-document`.

/// Options controlling how a single page render is produced.
///
/// `invert_content_colors` requests dark-mode inversion (T-017): rendering is
/// display-only and never mutates the underlying document bytes. See
/// `spec.md` "Dark-Mode Render Option".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderOptions {
    pub invert_content_colors: bool,
}

impl RenderOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_invert_content_colors(mut self, invert: bool) -> Self {
        self.invert_content_colors = invert;
        self
    }
}

/// A rectangular sub-region of a page, expressed in PDF points (1/72 inch),
/// used to request a partial-page render rather than the full page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// A bounded rectangle in the page's rendered top-left pixel coordinate
/// space. Unlike [`Rect`], this is already quantized to output pixels so
/// neighboring tiles share exact edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }
}

/// Scheduling priority for a job submitted to the pdfium actor's queue.
///
/// Ordered so that `Visible` jobs (the page currently on screen) always
/// dequeue ahead of `Prefetch` (adjacent pages, likely to be scrolled to
/// soon) and `Thumbnail` (sidebar strip, lowest urgency) — see `spec.md`
/// "Serialized pdfium Access" and `design.md`'s priority-queue rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Visible = 0,
    Prefetch = 1,
    Thumbnail = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_orders_visible_first() {
        let mut priorities = vec![Priority::Thumbnail, Priority::Visible, Priority::Prefetch];
        priorities.sort();
        assert_eq!(
            priorities,
            vec![Priority::Visible, Priority::Prefetch, Priority::Thumbnail]
        );
    }

    #[test]
    fn render_options_default_does_not_invert() {
        assert!(!RenderOptions::default().invert_content_colors);
    }

    #[test]
    fn rect_width_and_height() {
        let rect = Rect {
            left: 10.0,
            top: 20.0,
            right: 110.0,
            bottom: 220.0,
        };
        assert_eq!(rect.width(), 100.0);
        assert_eq!(rect.height(), 200.0);
    }

    #[test]
    fn tile_keeps_integer_output_edges() {
        let tile = Tile {
            left: 1024,
            top: 0,
            width: 1024,
            height: 768,
        };
        assert_eq!(tile.left + tile.width, 2048);
    }
}
