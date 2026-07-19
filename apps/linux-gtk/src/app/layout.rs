//! Page geometry: placeholder sizing, fit recomputation on resize, and the
//! stacking math that maps scroll offsets to page indices and back.

use gtk::prelude::*;
use gtk::Picture;

use super::render::update_viewport;
use super::state::{FitRequest, PageState, Viewer};
use super::PAGE_GAP;

/// Sizes a page's placeholder to the logical fit width and returns the
/// logical height it set, so callers can cache it in `page_heights`.
pub(crate) fn set_placeholder_size(
    picture: &Picture,
    width_pt: f32,
    height_pt: f32,
    fit: FitRequest,
) -> i32 {
    let logical_width = (fit.available_width / fit.scale_factor as u32).max(1) as i32;
    let logical_height = page_height(width_pt, height_pt, logical_width);
    picture.set_width_request(logical_width);
    picture.set_height_request(logical_height);
    logical_height
}

fn page_height(width_pt: f32, height_pt: f32, logical_width: i32) -> i32 {
    if width_pt <= 0.0 || height_pt <= 0.0 {
        return 1;
    }
    ((f64::from(logical_width) * f64::from(height_pt) / f64::from(width_pt)).round() as i32).max(1)
}

pub(crate) fn refresh_layout(viewer: &Viewer) {
    let fit = FitRequest::measure(viewer);
    {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        if session.physical_width == fit.available_width && session.scale_factor == fit.scale_factor
        {
            return;
        }
        session.physical_width = fit.available_width;
        session.scale_factor = fit.scale_factor;
        for active in session.active.values() {
            active.cancellation.cancel();
        }
        session.active.clear();
        session.last_visible = None;
        for index in 0..session.pages.len() {
            let page = &mut session.pages[index];
            page.picture.set_pixbuf(None);
            page.state = PageState::Idle;
            let logical_height =
                set_placeholder_size(&page.picture, page.width_pt, page.height_pt, fit);
            session.page_heights[index] = logical_height;
        }
    }
    update_viewport(viewer);
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
