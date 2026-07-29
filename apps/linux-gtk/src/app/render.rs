//! Viewport-driven rendering: deciding which pages to rasterize on each
//! scroll tick, dispatching those renders to the core, and applying results.

use gtk::prelude::*;
use gtk::{gdk_pixbuf, gio, glib};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError, RenderOptions, Tile};

use super::layout::{
    page_top, render_range, tile_placement, tile_plan, visible_range, ViewportRect,
};
use super::state::{ActiveRender, PageState, RenderedPage, Viewer};

const POINTS_PER_INCH: f64 = 72.0;
const PREFETCH_PAGES: usize = 1;
const CACHE_PAGES: usize = 3;
const MAX_RASTER_DIMENSION: u32 = 16_384;
const MAX_RASTER_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_RASTER_BYTES: u64 = 128 * 1024 * 1024;
pub(crate) fn update_viewport(viewer: &Viewer) {
    let adjustment = viewer.scroll.vadjustment();
    let mut jobs = Vec::new();
    let mut tile_jobs = Vec::new();
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
        let visible_unsettled =
            (first..=last).any(|index| session.pages[index].state != PageState::Rendered);
        let render_range = render_range(first, last, page_count, visible_unsettled, PREFETCH_PAGES);
        let cache_range = super::layout::nearby_range(first, last, page_count, CACHE_PAGES);

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
        session.active_tiles.retain(|page_index, active| {
            if (first..=last).contains(page_index) {
                true
            } else {
                active.cancellation.cancel();
                false
            }
        });
        for (page_index, page) in session.pages.iter_mut().enumerate() {
            if cache_range.contains(&page_index) {
                continue;
            }
            if page.state == PageState::Rendered {
                page.picture.set_pixbuf(None);
                page.state = PageState::Idle;
            }
            // Tiles are the deep-zoom surfaces: each full one pins
            // `TILE_EDGE_PX` squared pixels of RGBA. They leave with the page
            // for the same reason the base pixbuf does — otherwise every page
            // ever zoomed into stays resident until the next zoom change.
            for tile in page.tiles.drain().map(|(_, picture)| picture) {
                page.overlay.remove_overlay(&tile);
            }
            page.tile_dpi = 0;
            page.tile_failed_dpi = 0;
        }
        let document = session.document;
        let scale_factor = f64::from(session.scale_factor);
        let zoom_generation = session.zoom_generation;
        let horizontal = viewer.scroll.hadjustment().value();
        // `pages` is centred (`Align::Center`), so a page narrower than the
        // column carries slack to its left. The scroll offset is in column
        // coordinates; the tile grid is page-local. Without subtracting that
        // slack the sharp tiles land beside the region actually on screen.
        let column_width = f64::from(viewer.pages.width().max(0));
        for page_index in first..=last {
            let page = &mut session.pages[page_index];
            let page_width = f64::from(page.width_pt) * page.budget.factor;
            let page_left = ((column_width - page_width) / 2.0).max(0.0);
            let plan = tile_plan(
                page.budget,
                page.width_pt,
                page.height_pt,
                ViewportRect::new(
                    (horizontal - page_left).max(0.0),
                    (adjustment.value() - page_top(&session.page_heights, page_index)).max(0.0),
                    viewer.scroll.width().max(1) as f64,
                    adjustment.page_size(),
                ),
                scale_factor,
            );
            if !plan.uses_tiles
                || session.active_tiles.contains_key(&page_index)
                || page.tile_failed_dpi == plan.dpi
            {
                continue;
            }
            if page.tile_dpi != plan.dpi {
                page.tile_dpi = plan.dpi;
                page.tile_generation += 1;
                page.tile_failed_dpi = 0;
                for tile in page.tiles.drain().map(|(_, picture)| picture) {
                    page.overlay.remove_overlay(&tile);
                }
            }
            let missing: Vec<_> = plan
                .tiles
                .into_iter()
                .filter(|tile| !page.tiles.contains_key(tile))
                .collect();
            if !missing.is_empty() {
                tile_jobs.push((
                    document,
                    page_index,
                    plan.dpi,
                    missing,
                    page.tile_generation,
                    zoom_generation,
                ));
            }
        }
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
            let dpi = session.pages[page_index].target_dpi;
            if raster_dimensions(width_pt, height_pt, dpi).is_some() {
                let priority = if (first..=last).contains(&page_index) {
                    Priority::Visible
                } else {
                    Priority::Prefetch
                };
                jobs.push((document, page_index, dpi, priority, zoom_generation));
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
    for (document, page_index, dpi, tiles, tile_generation, zoom_generation) in tile_jobs {
        schedule_tiles(
            viewer,
            document,
            page_index,
            dpi,
            tiles,
            tile_generation,
            zoom_generation,
        );
    }
    for (document, page_index, dpi, priority, zoom_generation) in jobs {
        schedule_render(viewer, document, page_index, dpi, priority, zoom_generation);
    }
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
    generation: u64,
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
                generation,
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
            apply_render_result(
                &viewer, document, page_index, render_id, generation, dpi, result,
            );
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
    generation: u64,
    dpi: u32,
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
    if session
        .active
        .get(&page_index)
        .map(|active| (active.id, active.generation))
        != Some((render_id, generation))
        || session.zoom_generation != generation
        || session.pages.get(page_index).map(|page| page.target_dpi) != Some(dpi)
    {
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

fn schedule_tiles(
    viewer: &Viewer,
    document: DocumentHandle,
    page_index: usize,
    dpi: u32,
    tiles: Vec<super::layout::TileRect>,
    tile_generation: u64,
    generation: u64,
) {
    let core_tiles = tiles
        .iter()
        .map(|tile| Tile {
            left: tile.left,
            top: tile.top,
            width: tile.width,
            height: tile.height,
        })
        .collect();
    let handle = PdfiumRenderer::new().render_page_tiles(
        document,
        page_index as u32,
        dpi,
        core_tiles,
        RenderOptions::default(),
        Priority::Visible,
    );
    let cancellation = handle.cancellation_handle();
    let render_id = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            cancellation.cancel();
            return;
        };
        if session.document != document || session.zoom_generation != generation {
            cancellation.cancel();
            return;
        }
        let id = session.next_render_id;
        session.next_render_id += 1;
        session.active_tiles.insert(
            page_index,
            ActiveRender {
                id,
                generation,
                cancellation,
            },
        );
        id
    };
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = gio::spawn_blocking(move || {
                handle
                    .wait()?
                    .into_iter()
                    .map(|bitmap| {
                        Ok(RenderedPage {
                            width: bitmap.width()?,
                            height: bitmap.height()?,
                            stride: bitmap.stride()?,
                            pixels: bitmap.get_pixels()?,
                        })
                    })
                    .collect()
            })
            .await
            .expect("tile-render task panicked");
            apply_tile_result(
                &viewer,
                TileBatch {
                    document,
                    page_index,
                    render_id,
                    generation,
                    dpi,
                    tile_generation,
                    tiles,
                },
                result,
            );
        }
    });
}

/// Identity of one dispatched tile batch. It rides across the async hop so
/// the result can be matched against the state the batch was scheduled from:
/// anything that moved on in the meantime makes the batch stale.
struct TileBatch {
    document: DocumentHandle,
    page_index: usize,
    render_id: u64,
    generation: u64,
    dpi: u32,
    tile_generation: u64,
    tiles: Vec<super::layout::TileRect>,
}

fn apply_tile_result(
    viewer: &Viewer,
    batch: TileBatch,
    result: Result<Vec<RenderedPage>, RenderError>,
) {
    let TileBatch {
        document,
        page_index,
        render_id,
        generation,
        dpi,
        tile_generation,
        tiles,
    } = batch;
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    if session.document != document
        || session.zoom_generation != generation
        || session
            .active_tiles
            .get(&page_index)
            .map(|active| (active.id, active.generation))
            != Some((render_id, generation))
    {
        return;
    }
    session.active_tiles.remove(&page_index);
    let page_count = session.pages.len();
    let Some(page) = session.pages.get_mut(page_index) else {
        return;
    };
    if page.tile_generation != tile_generation || page.tile_dpi != dpi {
        return;
    }
    let rendered = match result {
        Ok(rendered) => rendered,
        Err(RenderError::Cancelled) => return,
        Err(error) => {
            // Mirror the base-page path: make this DPI terminal so the next
            // scroll tick can't re-queue the same doomed batch, and tell the
            // user instead of leaving the page silently soft.
            page.tile_failed_dpi = dpi;
            viewer.status.set_text(&format!(
                "Could not sharpen page {} of {page_count}: {error}",
                page_index + 1,
            ));
            return;
        }
    };
    // The batch contract is one bitmap per requested tile, in order. A
    // mismatch is a core-side defect, not a transient failure to retry.
    if rendered.len() != tiles.len() {
        page.tile_failed_dpi = dpi;
        viewer.status.set_text(&format!(
            "Page {} returned {} tiles for {} requested.",
            page_index + 1,
            rendered.len(),
            tiles.len(),
        ));
        return;
    }
    let logical_per_pixel = page.budget.factor * POINTS_PER_INCH / f64::from(dpi);
    for (tile, bitmap) in tiles.into_iter().zip(rendered) {
        let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
            &glib::Bytes::from_owned(bitmap.pixels),
            gdk_pixbuf::Colorspace::Rgb,
            true,
            8,
            bitmap.width as i32,
            bitmap.height as i32,
            bitmap.stride as i32,
        );
        let picture = gtk::Picture::new();
        picture.set_pixbuf(Some(&pixbuf));
        picture.set_halign(gtk::Align::Start);
        picture.set_valign(gtk::Align::Start);
        let (left, top, width, height) = tile_placement(tile, logical_per_pixel);
        picture.set_margin_start(left);
        picture.set_margin_top(top);
        picture.set_width_request(width);
        picture.set_height_request(height);
        page.overlay.add_overlay(&picture);
        page.tiles.insert(tile, picture);
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
