//! Viewport-driven rendering: deciding which pages to rasterize on each
//! scroll tick, dispatching those renders to the core, and applying results.

use gtk::prelude::*;
use gtk::{gdk_pixbuf, gio, glib};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError, RenderOptions};

use super::layout::{nearby_range, visible_range};
use super::state::{ActiveRender, PageState, RenderedPage, Viewer};

const POINTS_PER_INCH: f64 = 72.0;
const PREFETCH_PAGES: usize = 1;
const CACHE_PAGES: usize = 3;
const MAX_RASTER_DIMENSION: u32 = 16_384;
const MAX_RASTER_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_RASTER_BYTES: u64 = 128 * 1024 * 1024;
/// Fit-to-width DPI ceiling: a degenerate MediaBox (e.g. a page 1pt wide
/// but thousands of points tall) would otherwise request an unbounded
/// raster size from the render actor.
const MAX_RENDER_DPI: f64 = 1440.0;

pub(crate) fn update_viewport(viewer: &Viewer) {
    let adjustment = viewer.scroll.vadjustment();
    let mut jobs = Vec::new();
    let mut rejected_visible_page = None;
    let visible = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some((first, last)) = visible_range(
            &session.page_heights,
            adjustment.value(),
            adjustment.page_size(),
        ) else {
            return;
        };
        let page_count = session.pages.len();
        let render_range = nearby_range(first, last, page_count, PREFETCH_PAGES);
        let cache_range = nearby_range(first, last, page_count, CACHE_PAGES);

        // Cancel and drop renders that scrolled out of the prefetch window;
        // `Range::contains` is an allocation-free membership test.
        session.active.retain(|page_index, active| {
            if render_range.contains(page_index) {
                true
            } else {
                active.cancellation.cancel();
                false
            }
        });
        for (page_index, page) in session.pages.iter_mut().enumerate() {
            if !cache_range.contains(&page_index) && page.state == PageState::Rendered {
                page.picture.set_pixbuf(None);
                page.state = PageState::Idle;
            }
        }
        let document = session.document;
        let physical_width = session.physical_width;
        for page_index in render_range {
            if session.pages[page_index].state != PageState::Idle
                || session.active.contains_key(&page_index)
            {
                continue;
            }
            let (width_pt, height_pt) = {
                let page = &session.pages[page_index];
                (page.width_pt, page.height_pt)
            };
            let dpi = dpi_for_width(width_pt, physical_width);
            if raster_dimensions(width_pt, height_pt, dpi).is_some() {
                let priority = if (first..=last).contains(&page_index) {
                    Priority::Visible
                } else {
                    Priority::Prefetch
                };
                jobs.push((document, page_index, dpi, priority));
            } else {
                // Terminal for this fit: don't re-detect it every tick, and
                // only report it if it is actually on screen (not merely a
                // prefetch neighbour hijacking the status line).
                session.pages[page_index].state = PageState::Skipped;
                if (first..=last).contains(&page_index) {
                    rejected_visible_page = Some(page_index);
                }
            }
        }

        let range_changed = session.last_visible != Some((first, last));
        session.last_visible = Some((first, last));
        range_changed.then_some((first, last, page_count))
    };

    if let Some((first, last, page_count)) = visible {
        viewer.status.set_text(&format!(
            "Showing pages {}-{} of {page_count}.",
            first + 1,
            last + 1
        ));
    }
    if let Some(page_index) = rejected_visible_page {
        viewer.status.set_text(&format!(
            "Page {} cannot be rendered safely at this size.",
            page_index + 1
        ));
    }
    for (document, page_index, dpi, priority) in jobs {
        schedule_render(viewer, document, page_index, dpi, priority);
    }
}

fn dpi_for_width(width_pt: f32, available_width: u32) -> u32 {
    ((f64::from(available_width) / f64::from(width_pt)) * POINTS_PER_INCH)
        .floor()
        .clamp(1.0, MAX_RENDER_DPI) as u32
}

pub(crate) fn raster_dimensions(width_pt: f32, height_pt: f32, dpi: u32) -> Option<(u32, u32)> {
    let scale = f64::from(dpi) / POINTS_PER_INCH;
    let width = (f64::from(width_pt) * scale).round();
    let height = (f64::from(height_pt) * scale).round();
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let (width, height) = (width as u32, height as u32);
    let pixels = u64::from(width).checked_mul(u64::from(height))?;
    let bytes = pixels.checked_mul(4)?;
    (width <= MAX_RASTER_DIMENSION
        && height <= MAX_RASTER_DIMENSION
        && pixels <= MAX_RASTER_PIXELS
        && bytes <= MAX_RASTER_BYTES)
        .then_some((width, height))
}

fn schedule_render(
    viewer: &Viewer,
    document: DocumentHandle,
    page_index: usize,
    dpi: u32,
    priority: Priority,
) {
    let handle = PdfiumRenderer::new().render_page(
        document,
        page_index as u32,
        dpi,
        None,
        RenderOptions::default(),
        priority,
    );
    let cancellation = handle.cancellation_handle();
    // The session is identified by its document handle (pdfium hands out
    // monotonically increasing ids, so a replaced session never collides
    // with the one this render targets). That check alone tells a stale
    // render from a live one — no separate generation is threaded through.
    let render_id = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            cancellation.cancel();
            return;
        };
        if session.document != document {
            cancellation.cancel();
            return;
        }
        let render_id = session.next_render_id;
        session.next_render_id += 1;
        session.active.insert(
            page_index,
            ActiveRender {
                id: render_id,
                cancellation,
            },
        );
        render_id
    };
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = gio::spawn_blocking(move || render_result(handle))
                .await
                .expect("page-render task panicked");
            apply_render_result(&viewer, document, page_index, render_id, result);
        }
    });
}

pub(crate) fn render_result(handle: pdf_render::RenderHandle) -> Result<RenderedPage, RenderError> {
    let bitmap = handle.wait()?;
    Ok(RenderedPage {
        width: bitmap.width()?,
        height: bitmap.height()?,
        stride: bitmap.stride()?,
        pixels: bitmap.get_pixels()?,
    })
}

fn apply_render_result(
    viewer: &Viewer,
    document: DocumentHandle,
    page_index: usize,
    render_id: u64,
    result: Result<RenderedPage, RenderError>,
) {
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    if session.document != document {
        return;
    }
    // Only the render currently recorded for this page may apply its
    // result; a superseded one (its slot re-scheduled after a refit) is
    // dropped. This is the same window `update_viewport` kept in the
    // `active` map, so the finished render is always still wanted — no
    // recompute of the visible range is needed to re-check that.
    if session.active.get(&page_index).map(|active| active.id) != Some(render_id) {
        return;
    }
    session.active.remove(&page_index);
    let page_count = session.pages.len();
    let Some(slot) = session.pages.get_mut(page_index) else {
        return;
    };
    match result {
        Ok(page) => {
            let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
                &glib::Bytes::from_owned(page.pixels),
                gdk_pixbuf::Colorspace::Rgb,
                true,
                8,
                page.width as i32,
                page.height as i32,
                page.stride as i32,
            );
            slot.picture.set_pixbuf(Some(&pixbuf));
            slot.state = PageState::Rendered;
        }
        Err(RenderError::Cancelled) => {}
        Err(error) => {
            // Mark the slot failed so `update_viewport` doesn't re-queue the
            // same doomed render on every scroll tick. A refit resets it.
            slot.state = PageState::Failed;
            viewer.status.set_text(&format!(
                "Could not render page {} of {page_count}: {error}",
                page_index + 1,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_dimensions_rejects_an_oversized_page() {
        assert_eq!(raster_dimensions(72.0, 72_000.0, 72), None);
    }
}
