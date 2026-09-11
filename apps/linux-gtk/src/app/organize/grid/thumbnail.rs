//! Turning a page into a card's picture: the off-thread pdfium render and
//! the DPI that fits its result into the card that asked for it.
//!
//! Split from [`super`], which owns the cards themselves — both views of the
//! Organize screen render through this, at their own two different sizes.

use gtk::prelude::*;
use gtk::{gdk_pixbuf, gio, glib, Picture};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderOptions};

use crate::app::render::render_result;
use crate::app::state::{RenderedPage, Viewer};

const POINTS_PER_INCH: f64 = 72.0;

/// A card stretched past its logical size needs pixels to stretch into.
/// `grid` is homogeneous (see `super::super::build_organize_panel`), so a row
/// with fewer than `super::super::CARDS_PER_ROW` cards stretches each card
/// well past the size it asked for to fill the line, and GTK4 CSS has no
/// `max-width`/`max-height` to cap that. Rendering at this multiple of the
/// logical size instead of 1x is what keeps the stretch from going blocky;
/// the DPI is still clamped in [`thumbnail_dpi`].
const RENDER_HEADROOM: i32 = 3;

/// Renders one card's thumbnail off the main thread and fills its `Picture`
/// in when the render lands.
///
/// The `cfg(test)` early return is the module's one test seam, and it is here
/// rather than in the tests because this is the only line where the pairing
/// under test — *which* pdfium page index a given card asked for — still
/// exists. `Document.pages` is reordered by `Command::MovePage`, so a card
/// that rendered by grid position instead of by page identity would still
/// look right in the model and wrong on screen; capturing the request is what
/// tells the two apart. The tests cannot let the real path run: their session
/// carries no pdfium document.
pub(in crate::app::organize) fn spawn_thumbnail(
    viewer: &Viewer,
    handle: DocumentHandle,
    pdfium_page_index: u32,
    picture: Picture,
    (width_px, height_px): (i32, i32),
) {
    #[cfg(test)]
    if super::super::tests::capture_thumbnail(pdfium_page_index, &picture) {
        return;
    }
    let scale_factor = picture.scale_factor().max(1) * RENDER_HEADROOM;
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let job = move || -> Result<RenderedPage, pdf_render::RenderError> {
                let renderer = PdfiumRenderer::new();
                let (width_pt, height_pt) = renderer
                    .page_size(handle, pdfium_page_index, Priority::Thumbnail)
                    .wait()?;
                let dpi = thumbnail_dpi(width_pt, height_pt, scale_factor, (width_px, height_px));
                render_result(renderer.render_page(
                    handle,
                    pdfium_page_index,
                    dpi,
                    None,
                    RenderOptions::new(),
                    Priority::Thumbnail,
                ))
            };
            let Ok(Ok(page)) = gio::spawn_blocking(job).await else {
                return;
            };
            let still_current = viewer
                .state
                .borrow()
                .session
                .as_ref()
                .is_some_and(|session| session.document == handle);
            if !still_current {
                return;
            }
            let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
                &glib::Bytes::from_owned(page.pixels),
                gdk_pixbuf::Colorspace::Rgb,
                true,
                8,
                page.width as i32,
                page.height as i32,
                page.stride as i32,
            );
            picture.set_pixbuf(Some(&pixbuf));
        }
    });
}

/// The DPI that fits a `width_pt` x `height_pt` page inside `width_px` x
/// `height_px` at `scale_factor` — the organize-grid twin of
/// `home::recents::thumbnail_dpi`. Kept as its own copy rather than shared:
/// two call sites and a ten-line pure function is not worth an abstraction.
///
/// The target size is a parameter rather than [`CARD_WIDTH_PX`]/
/// [`CARD_HEIGHT_PX`] because the Documents view renders the same pages onto
/// a smaller block cover (`documents::COVER_WIDTH_PX`).
fn thumbnail_dpi(
    width_pt: f32,
    height_pt: f32,
    scale_factor: i32,
    (width_px, height_px): (i32, i32),
) -> u32 {
    let scale = f64::from(scale_factor.max(1));
    let fit = |pixels: i32, points: f32| {
        f64::from(pixels) * scale * POINTS_PER_INCH / f64::from(points.max(1.0))
    };
    fit(width_px, width_pt)
        .min(fit(height_px, height_pt))
        .floor()
        .clamp(8.0, 300.0) as u32
}
