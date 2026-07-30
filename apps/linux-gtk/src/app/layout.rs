//! Page geometry: placeholder sizing, fit recomputation on resize, and the
//! stacking math that maps scroll offsets to page indices and back.

use gtk::prelude::*;
use gtk::Picture;

use super::render::update_viewport;
use super::state::{FitRequest, PageState, Viewer};
use super::PAGE_GAP;

pub(crate) const MIN_RENDER_DPI: u32 = 24;
const MAX_RENDER_DPI: u32 = 600;
const MAX_BASE_PIXELS: f64 = 8_000_000.0;
const BRIDGE_PIXELS: f64 = 2_000_000.0;
pub(crate) const TILE_EDGE_PX: u32 = 1024;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Zoom {
    FitWidth,
    FitPage,
    Custom(f64),
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct PageBox {
    pub(crate) logical_width: i32,
    pub(crate) logical_height: i32,
    pub(crate) factor: f64,
    pub(crate) base_dpi: u32,
}

/// The only two values the tile pipeline reads out of a resolved page box:
/// the logical scale and the DPI the base render settled for. Callers that
/// never computed a full `PageBox` pass this instead of inventing dimensions
/// the tile code would discard anyway.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct TileBudget {
    pub(crate) factor: f64,
    pub(crate) base_dpi: u32,
}

impl PageBox {
    pub(crate) fn budget(self) -> TileBudget {
        TileBudget {
            factor: self.factor,
            base_dpi: self.base_dpi,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ViewportRect {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl ViewportRect {
    pub(crate) fn new(left: f64, top: f64, width: f64, height: f64) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TileRect {
    pub(crate) left: u32,
    pub(crate) top: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) struct TilePlan {
    pub(crate) uses_tiles: bool,
    pub(crate) dpi: u32,
    pub(crate) tiles: Vec<TileRect>,
}

pub(crate) fn resolve_page_box(
    zoom: Zoom,
    width_pt: f32,
    height_pt: f32,
    available_logical_width: f64,
    available_logical_height: f64,
    scale_factor: f64,
) -> PageBox {
    if !usable(width_pt) || !usable(height_pt) {
        return PageBox {
            logical_width: 1,
            logical_height: 1,
            factor: 1.0,
            base_dpi: MIN_RENDER_DPI,
        };
    }
    let factor = match zoom {
        Zoom::FitWidth if available_logical_width.is_finite() && available_logical_width > 0.0 => {
            available_logical_width / f64::from(width_pt)
        }
        Zoom::FitWidth => 1.0,
        Zoom::FitPage
            if available_logical_width.is_finite()
                && available_logical_width > 0.0
                && available_logical_height.is_finite()
                && available_logical_height > 0.0 =>
        {
            (available_logical_width / f64::from(width_pt))
                .min(available_logical_height / f64::from(height_pt))
        }
        Zoom::FitPage => 1.0,
        Zoom::Custom(factor) => factor,
    }
    .clamp(0.10, 8.0);
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let desired_dpi = (72.0 * factor * scale)
        .floor()
        .clamp(f64::from(MIN_RENDER_DPI), f64::from(MAX_RENDER_DPI));
    let pixels =
        f64::from(width_pt) * desired_dpi / 72.0 * (f64::from(height_pt) * desired_dpi / 72.0);
    let base_dpi = if pixels > MAX_BASE_PIXELS {
        (desired_dpi * (MAX_BASE_PIXELS / pixels).sqrt())
            .floor()
            .max(f64::from(MIN_RENDER_DPI)) as u32
    } else {
        desired_dpi as u32
    };
    PageBox {
        logical_width: (f64::from(width_pt) * factor).round().max(1.0) as i32,
        logical_height: (f64::from(height_pt) * factor).round().max(1.0) as i32,
        factor,
        base_dpi,
    }
}

pub(crate) fn intended_dpi(budget: TileBudget, scale_factor: f64) -> u32 {
    (72.0 * budget.factor * scale_factor.max(1.0))
        .floor()
        .clamp(f64::from(MIN_RENDER_DPI), f64::from(MAX_RENDER_DPI)) as u32
}

pub(crate) fn would_use_tiles(budget: TileBudget, scale_factor: f64) -> bool {
    intended_dpi(budget, scale_factor) > budget.base_dpi
}

pub(crate) fn bridge_dpi(width_pt: f32, height_pt: f32, base_dpi: u32) -> u32 {
    let pixels = f64::from(width_pt) * f64::from(base_dpi) / 72.0
        * (f64::from(height_pt) * f64::from(base_dpi) / 72.0);
    if !pixels.is_finite() || pixels <= BRIDGE_PIXELS {
        return base_dpi;
    }
    (f64::from(base_dpi) * (BRIDGE_PIXELS / pixels).sqrt())
        .floor()
        .clamp(f64::from(MIN_RENDER_DPI), f64::from(base_dpi)) as u32
}

pub(crate) fn tile_plan(
    budget: TileBudget,
    width_pt: f32,
    height_pt: f32,
    viewport: ViewportRect,
    scale_factor: f64,
) -> TilePlan {
    let dpi = intended_dpi(budget, scale_factor);
    if !would_use_tiles(budget, scale_factor) || !usable(width_pt) || !usable(height_pt) {
        return TilePlan {
            uses_tiles: false,
            dpi: budget.base_dpi,
            tiles: Vec::new(),
        };
    }
    let page_width = (f64::from(width_pt) * f64::from(dpi) / 72.0).ceil() as u32;
    let page_height = (f64::from(height_pt) * f64::from(dpi) / 72.0).ceil() as u32;
    let pixels_per_logical = f64::from(dpi) / (72.0 * budget.factor);
    let left = (viewport.left * pixels_per_logical).floor().max(0.0) as u32;
    let top = (viewport.top * pixels_per_logical).floor().max(0.0) as u32;
    let right = ((viewport.left + viewport.width) * pixels_per_logical)
        .ceil()
        .max(f64::from(left)) as u32;
    let bottom = ((viewport.top + viewport.height) * pixels_per_logical)
        .ceil()
        .max(f64::from(top)) as u32;
    let left = left.min(page_width);
    let top = top.min(page_height);
    let right = right.min(page_width).max(left);
    let bottom = bottom.min(page_height).max(top);
    let mut tiles = Vec::new();
    for y in (top / TILE_EDGE_PX * TILE_EDGE_PX..bottom).step_by(TILE_EDGE_PX as usize) {
        for x in (left / TILE_EDGE_PX * TILE_EDGE_PX..right).step_by(TILE_EDGE_PX as usize) {
            tiles.push(TileRect {
                left: x,
                top: y,
                width: TILE_EDGE_PX.min(page_width - x),
                height: TILE_EDGE_PX.min(page_height - y),
            });
        }
    }
    TilePlan {
        uses_tiles: true,
        dpi,
        tiles,
    }
}

/// Places one tile in logical coordinates, as `(left, top, width, height)`.
///
/// Both edges go through the same rounding and the size is their difference,
/// so two tiles sharing an integer grid edge in page pixels also share an
/// exact edge on screen. Rounding the offset but ceiling the size — the
/// obvious way to write this — lets neighbours overlap by a pixel or leave a
/// hairline gap depending on which side of `.5` each edge lands.
pub(crate) fn tile_placement(tile: TileRect, logical_per_pixel: f64) -> (i32, i32, i32, i32) {
    let edge = |pixels: f64| (pixels * logical_per_pixel).round() as i32;
    let left = edge(f64::from(tile.left));
    let top = edge(f64::from(tile.top));
    let right = edge(f64::from(tile.left) + f64::from(tile.width));
    let bottom = edge(f64::from(tile.top) + f64::from(tile.height));
    (left, top, (right - left).max(1), (bottom - top).max(1))
}

pub(crate) fn render_range(
    first: usize,
    last: usize,
    page_count: usize,
    visible_unsettled: bool,
    prefetch: usize,
) -> std::ops::Range<usize> {
    if visible_unsettled {
        first..(last + 1).min(page_count)
    } else {
        nearby_range(first, last, page_count, prefetch)
    }
}

fn usable(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

/// Sizes a page's placeholder to the logical fit width and returns the
/// logical height it set, so callers can cache it in `page_heights`.
pub(crate) fn set_placeholder_size(
    picture: &Picture,
    width_pt: f32,
    height_pt: f32,
    fit: FitRequest,
) -> i32 {
    let box_ = resolve_page_box(
        Zoom::FitWidth,
        width_pt,
        height_pt,
        f64::from(fit.available_width / fit.scale_factor as u32),
        f64::from(fit.available_height / fit.scale_factor as u32),
        f64::from(fit.scale_factor),
    );
    picture.set_width_request(box_.logical_width);
    picture.set_height_request(box_.logical_height);
    box_.logical_height
}

pub(crate) fn refresh_layout(viewer: &Viewer) {
    let fit = FitRequest::measure(viewer);
    {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        if session.physical_width == fit.available_width
            && session.physical_height == fit.available_height
            && session.scale_factor == fit.scale_factor
        {
            return;
        }
        session.physical_width = fit.available_width;
        session.physical_height = fit.available_height;
        session.scale_factor = fit.scale_factor;
        for active in session.active.values() {
            active.cancellation.cancel();
        }
        session.active.clear();
        for active in session.active_tiles.values() {
            active.cancellation.cancel();
        }
        session.active_tiles.clear();
        session.zoom_generation += 1;
        session.last_visible = None;
        for index in 0..session.pages.len() {
            let page = &mut session.pages[index];
            page.state = PageState::Idle;
            let box_ = resolve_page_box(
                session.zoom,
                page.width_pt,
                page.height_pt,
                f64::from(fit.available_width / fit.scale_factor as u32),
                f64::from(fit.available_height / fit.scale_factor as u32),
                f64::from(fit.scale_factor),
            );
            page.picture.set_width_request(box_.logical_width);
            page.picture.set_height_request(box_.logical_height);
            page.target_dpi = if would_use_tiles(box_.budget(), f64::from(fit.scale_factor)) {
                bridge_dpi(page.width_pt, page.height_pt, box_.base_dpi)
            } else {
                box_.base_dpi
            };
            page.budget = box_.budget();
            page.tile_generation += 1;
            page.tile_dpi = 0;
            page.tile_failed_dpi = 0;
            for tile in page.tiles.drain().map(|(_, picture)| picture) {
                page.overlay.remove_overlay(&tile);
            }
            session.page_heights[index] = box_.logical_height;
        }
    }
    update_viewport(viewer);
}

pub(crate) fn set_zoom(viewer: &Viewer, zoom: Zoom) {
    let changed = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        if session.zoom == zoom {
            false
        } else {
            session.zoom = zoom;
            true
        }
    };
    if changed {
        // Reset cached viewport dimensions so the shared layout path retargets pages.
        if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
            session.physical_width = 0;
            session.physical_height = 0;
        }
        refresh_layout(viewer);
    }
}

pub(crate) fn visible_range(
    page_heights: &[i32],
    viewport_top: f64,
    viewport_height: f64,
) -> Option<(usize, usize)> {
    let viewport_bottom = viewport_top + viewport_height.max(1.0);
    let mut page_top = 0.0;
    let mut first = None;
    let mut last = 0;
    for (index, height) in page_heights.iter().enumerate() {
        let page_bottom = page_top + f64::from(*height);
        if page_bottom >= viewport_top && page_top <= viewport_bottom {
            first.get_or_insert(index);
            last = index;
        }
        page_top = page_bottom + f64::from(PAGE_GAP);
    }
    first.map(|first| (first, last))
}

pub(crate) fn nearby_range(
    first: usize,
    last: usize,
    page_count: usize,
    radius: usize,
) -> std::ops::Range<usize> {
    first.saturating_sub(radius)..(last + radius + 1).min(page_count)
}

/// Distance from the top of the page box to the top of `page_index`,
/// mirroring the stacking `visible_range` walks: each page contributes its
/// height plus the box's inter-page gap.
pub(crate) fn page_top(page_heights: &[i32], page_index: usize) -> f64 {
    page_heights
        .iter()
        .take(page_index)
        .map(|height| f64::from(*height) + f64::from(PAGE_GAP))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grid's own ceiling: a full tile is `TILE_EDGE_PX` squared, and no
    /// emitted tile may exceed it. Lives here because it is an assertion about
    /// `tile_plan`, not a budget `tile_plan` consults.
    const MAX_TILE_PIXELS: u64 = TILE_EDGE_PX as u64 * TILE_EDGE_PX as u64;

    #[test]
    fn tile_plan_activates_only_when_physical_dpi_exceeds_the_base_budget() {
        let page = resolve_page_box(Zoom::Custom(6.0), 612.0, 792.0, 1_600.0, 1_000.0, 1.0);

        assert!(would_use_tiles(page.budget(), 1.0));
    }

    #[test]
    fn fit_page_uses_viewport_height_when_it_is_the_limiting_dimension() {
        let page = resolve_page_box(Zoom::FitPage, 612.0, 792.0, 1_000.0, 500.0, 1.0);

        assert_eq!(page.logical_width, 386);
        assert_eq!(page.logical_height, 500);
        assert!((page.factor - 500.0 / 792.0).abs() < 1e-9);
    }

    #[test]
    fn fit_page_uses_viewport_width_when_it_is_the_limiting_dimension() {
        let page = resolve_page_box(Zoom::FitPage, 612.0, 792.0, 400.0, 1_000.0, 1.0);

        assert_eq!(page.logical_width, 400);
        assert_eq!(page.logical_height, 518);
        assert!((page.factor - 400.0 / 612.0).abs() < 1e-9);
    }

    #[test]
    fn fit_page_recomputes_when_only_viewport_height_changes() {
        let before = resolve_page_box(Zoom::FitPage, 612.0, 792.0, 1_000.0, 700.0, 1.0);
        let after = resolve_page_box(Zoom::FitPage, 612.0, 792.0, 1_000.0, 350.0, 1.0);

        assert_eq!(before.logical_height, 700);
        assert_eq!(after.logical_height, 350);
        assert!(after.factor < before.factor);
    }

    #[test]
    fn tile_plan_uses_a_fixed_page_local_grid_with_shared_integer_edges() {
        let page = resolve_page_box(Zoom::Custom(6.0), 612.0, 792.0, 1_600.0, 1_000.0, 1.0);
        let plan = tile_plan(
            page.budget(),
            612.0,
            792.0,
            ViewportRect::new(700.0, 1_300.0, 1_200.0, 900.0),
            1.0,
        );

        assert!(plan.tiles.iter().all(|tile| {
            tile.left % TILE_EDGE_PX == 0
                && tile.top % TILE_EDGE_PX == 0
                && tile.width > 0
                && tile.height > 0
                && u64::from(tile.width) * u64::from(tile.height) <= MAX_TILE_PIXELS
        }));
    }

    #[test]
    fn tile_plan_is_stable_for_scrolls_inside_the_same_grid_row() {
        let page = resolve_page_box(Zoom::Custom(6.0), 612.0, 792.0, 1_600.0, 1_000.0, 1.0);
        let at_rest = tile_plan(
            page.budget(),
            612.0,
            792.0,
            ViewportRect::new(0.0, 1_100.0, 1_200.0, 900.0),
            1.0,
        );
        let nudged = tile_plan(
            page.budget(),
            612.0,
            792.0,
            ViewportRect::new(0.0, 1_112.0, 1_200.0, 900.0),
            1.0,
        );

        assert_eq!(at_rest.tiles, nudged.tiles);
    }

    #[test]
    fn tiled_page_uses_a_bounded_base_bridge() {
        let page = resolve_page_box(Zoom::Custom(6.0), 612.0, 792.0, 1_600.0, 1_000.0, 1.0);
        let bridge = bridge_dpi(612.0, 792.0, page.base_dpi);

        assert!(bridge < page.base_dpi && bridge >= MIN_RENDER_DPI);
    }

    #[test]
    fn adjacent_tiles_share_an_exact_logical_edge() {
        // 1024 * 0.1235 = 126.464: rounding the offset while ceiling the size
        // put this tile at 0..127 and its neighbour at 126, overlapping by a
        // pixel row. Deriving the size from two rounded edges cannot.
        let per_pixel = 0.1235;
        let origin = TileRect {
            left: 0,
            top: 0,
            width: TILE_EDGE_PX,
            height: TILE_EDGE_PX,
        };
        let right = TileRect {
            left: TILE_EDGE_PX,
            ..origin
        };
        let below = TileRect {
            top: TILE_EDGE_PX,
            ..origin
        };

        let (x, y, width, height) = tile_placement(origin, per_pixel);
        let (right_x, _, _, _) = tile_placement(right, per_pixel);
        let (_, below_y, _, _) = tile_placement(below, per_pixel);

        assert_eq!(x + width, right_x);
        assert_eq!(y + height, below_y);
    }

    #[test]
    fn render_window_defers_prefetch_while_visible_pages_are_unsettled() {
        assert_eq!(render_range(2, 3, 6, true, 1), 2..4);
    }

    #[test]
    fn visible_range_includes_pages_intersecting_the_viewport_edges() {
        // Viewport [95, 115): page 0 spans [0, 100), page 1 [112, 212)
        // after the 12pt gap — both touch the viewport, page 2 does not.
        assert_eq!(visible_range(&[100, 100, 100], 95.0, 20.0), Some((0, 1)));
    }

    #[test]
    fn visible_range_is_empty_without_pages() {
        assert_eq!(visible_range(&[], 0.0, 20.0), None);
    }

    #[test]
    fn nearby_range_stays_within_document_bounds() {
        assert_eq!(nearby_range(0, 0, 3, 3), 0..3);
    }

    #[test]
    fn page_top_stacks_heights_and_gaps_like_visible_range() {
        // Page 0 starts at 0; page 2 sits below two pages and two gaps.
        assert_eq!(page_top(&[100, 100, 100], 0), 0.0);
        assert_eq!(
            page_top(&[100, 100, 100], 2),
            2.0 * (100.0 + f64::from(PAGE_GAP))
        );
    }
}
