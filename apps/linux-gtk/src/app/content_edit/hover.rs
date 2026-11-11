//! What the pointer rests on in content-edit mode: a thin outline round the
//! item a press would grab, the way Acrobat's "Edit PDF" shows it.
//!
//! The item is found by the same hit-test a SHIFT+click asks
//! ([`multi::member_at`]): an image, then a whole text block, then a vector
//! path. There is no second hit-test here, so what is outlined is always
//! what a press at that point would take.
//!
//! The hover is read from the page's *already parsed* content and never
//! parses it: a pointer crossing a page must not cost a content-stream parse
//! per motion event. `load_visible_page_content` has parsed every page in
//! view by the time the mode is armed, so in practice nothing is missed.
//!
//! Everything is worked out in PDF page space and placed through the page's
//! own `PagePlacement`, so the outline sits right on a rotated page and at
//! any zoom. The padding is a screen distance, divided by the zoom on the way
//! in.

use gtk::prelude::*;
use gtk::{cairo, DrawingArea, EventControllerMotion};
use pdf_document::Rect;
use pdf_render::{place_rect, TextRect};

use crate::app::selection::{handle_reach, pointer_to_pdf};
use crate::app::state::{DocumentSession, PageSlot, Viewer};

use super::multi::{self, Member};

/// How far the outline sits outside the item's box, in device pixels — so
/// it does not run through the glyphs or the picture's own edge.
pub(crate) const OUTLINE_PAD_PX: f64 = 2.0;
/// The outline itself: one device pixel of the handle blue.
const OUTLINE_WIDTH_PX: f64 = 1.0;
const OUTLINE_RGB: (f64, f64, f64) = (0.10, 0.35, 0.85);

/// The item under the pointer, on the page it was found on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hover {
    pub(crate) page_index: usize,
    pub(crate) member: Member,
}

/// The box an item occupies, in PDF page space: a text block's is the union
/// of its runs, as its resting outline is.
pub(crate) fn member_bbox(member: &Member) -> Option<Rect> {
    match member {
        Member::Runs(runs) => multi::union(&pdf_edit::Selection {
            runs: runs.clone(),
            ..pdf_edit::Selection::default()
        }),
        Member::Image(image) => Some(image.bbox),
        Member::Path(path) => Some(path.bbox),
    }
}

/// `rect` grown by `by` on every side.
pub(crate) fn inflate(rect: Rect, by: f64) -> Rect {
    Rect {
        x: rect.x - by,
        y: rect.y - by,
        width: rect.width + 2.0 * by,
        height: rect.height + 2.0 * by,
    }
}

/// The outline's box in PDF page space: the item's, padded by
/// [`OUTLINE_PAD_PX`] at `scale` device pixels per point.
pub(crate) fn outline_rect(bbox: Rect, scale: f64) -> Rect {
    inflate(bbox, OUTLINE_PAD_PX / usable_scale(scale))
}

/// A zoom that can be divided by. A page that has not been laid out yet
/// reports nothing sensible, and one point per pixel is a harmless stand-in.
fn usable_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// What the pointer at `point` (PDF page space) rests on in `content`.
pub(crate) fn resolve(
    content: &pdf_document::PageContent,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> Option<Hover> {
    multi::member_at(content, point, reach).map(|member| Hover { page_index, member })
}

/// Whether a press is being dragged right now. The outline stands down for
/// the length of it: the drag paints its own live box, and an outline left
/// where the item was would read as a second item.
pub(crate) fn dragging(session: &DocumentSession) -> bool {
    session.text_drag.is_some()
        || session.image_drag.is_some()
        || session.path_drag.is_some()
        || session.selection_gesture.is_some()
}

/// Wires the hover to one page's highlight layer.
pub(crate) fn connect(area: &DrawingArea, viewer: &Viewer, page_index: usize) {
    let motion = EventControllerMotion::new();
    motion.connect_motion({
        let viewer = viewer.clone();
        move |_, x, y| track(&viewer, page_index, x, y)
    });
    motion.connect_leave({
        let viewer = viewer.clone();
        move |_| leave(&viewer, page_index)
    });
    area.add_controller(motion);
}

/// Follows the pointer to `(x, y)` on `page_index`'s layer.
pub(crate) fn track(viewer: &Viewer, page_index: usize, x: f64, y: f64) {
    let armed = {
        let state = viewer.state.borrow();
        state.content_edit_mode && state.content_insert_mode.is_none()
    };
    if !armed {
        clear(viewer);
        return;
    }
    let reach = handle_reach(viewer, page_index);
    let Some(point) = pointer_to_pdf(viewer, page_index, x, y) else {
        return;
    };
    let changed = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        if dragging(session) {
            return;
        }
        let hover = session
            .pages
            .get(page_index)
            .and_then(|page| page.content.as_ref())
            .and_then(|content| resolve(content, page_index, point, reach));
        if session.content_hover == hover {
            return;
        }
        let previous = std::mem::replace(&mut session.content_hover, hover);
        repaint_list(
            session,
            previous.as_ref().map(|hover| hover.page_index),
            page_index,
        )
    };
    for area in changed {
        area.queue_draw();
    }
}

/// The pointer left `page_index`'s layer: whatever it rested on there is no
/// longer under it.
fn leave(viewer: &Viewer, page_index: usize) {
    let area = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        if session
            .content_hover
            .as_ref()
            .is_none_or(|hover| hover.page_index != page_index)
        {
            return;
        }
        session.content_hover = None;
        session
            .pages
            .get(page_index)
            .map(|page| page.highlights.clone())
    };
    if let Some(area) = area {
        area.queue_draw();
    }
}

/// Drops the hover — the mode went off, or an insert kind was armed and
/// every click now places something new rather than grabbing what is there.
pub(crate) fn clear(viewer: &Viewer) {
    let area = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(previous) = session.content_hover.take() else {
            return;
        };
        session
            .pages
            .get(previous.page_index)
            .map(|page| page.highlights.clone())
    };
    if let Some(area) = area {
        area.queue_draw();
    }
}

/// The layers a hover moving from `previous` to `current` must repaint.
fn repaint_list(
    session: &DocumentSession,
    previous: Option<usize>,
    current: usize,
) -> Vec<DrawingArea> {
    let mut pages = vec![current];
    if let Some(previous) = previous.filter(|previous| *previous != current) {
        pages.push(previous);
    }
    pages
        .into_iter()
        .filter_map(|index| session.pages.get(index))
        .map(|page| page.highlights.clone())
        .collect()
}

/// Paints the hover outline on `page`, if the pointer rests on something
/// there. Called from `selection::draw_highlights` while the mode is on.
pub(crate) fn draw(
    context: &cairo::Context,
    page: &PageSlot,
    session: &DocumentSession,
    page_index: usize,
) {
    // An open editor sits over its run, and a drag paints its own box.
    if dragging(session) || session.content_editor.is_some() {
        return;
    }
    let Some(bbox) = session
        .content_hover
        .as_ref()
        .filter(|hover| hover.page_index == page_index)
        .and_then(|hover| member_bbox(&hover.member))
    else {
        return;
    };
    let outline = outline_rect(bbox, page.budget.factor);
    let placed = place_rect(
        TextRect {
            x_pt: outline.x as f32,
            y_pt: outline.y as f32,
            width_pt: outline.width as f32,
            height_pt: outline.height as f32,
        },
        page.placement(),
    );
    let _ = context.save();
    let (red, green, blue) = OUTLINE_RGB;
    context.set_source_rgb(red, green, blue);
    context.set_line_width(OUTLINE_WIDTH_PX);
    context.set_dash(&[], 0.0);
    context.rectangle(placed.left, placed.top, placed.width, placed.height);
    let _ = context.stroke();
    let _ = context.restore();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::content_edit::test_support::Fixture;
    use pdf_document::PageId;
    use pdf_render::PageRotation;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_outline_pads_by_two_screen_pixels_at_any_zoom() {
        let bbox = rect(100.0, 200.0, 50.0, 10.0);

        assert_eq!(outline_rect(bbox, 1.0), rect(98.0, 198.0, 54.0, 14.0));
        assert_eq!(outline_rect(bbox, 2.0), rect(99.0, 199.0, 52.0, 12.0));
    }

    #[test]
    fn an_unlaid_page_pads_as_if_at_one_point_per_pixel() {
        let bbox = rect(0.0, 0.0, 10.0, 10.0);

        assert_eq!(outline_rect(bbox, 0.0), outline_rect(bbox, 1.0));
        assert_eq!(outline_rect(bbox, f64::NAN), outline_rect(bbox, 1.0));
    }

    /// The hover is the SHIFT+click hit-test's answer, so a text block is
    /// outlined whole and an empty margin outlines nothing.
    #[test]
    fn a_hover_over_a_run_outlines_its_whole_block() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let content = pdf_edit::read_page_content(&base, PageId(0)).expect("the fixture parses");
        let run = content.text_runs[0].clone();
        let middle = (
            run.bbox.x + run.bbox.width / 2.0,
            run.bbox.y + run.bbox.height / 2.0,
        );

        let hover = resolve(&content, 3, middle, 1.0).expect("the run is under the pointer");

        assert_eq!(hover.page_index, 3);
        let Member::Runs(runs) = &hover.member else {
            panic!("a run resolves to its text block, got {:?}", hover.member);
        };
        assert!(runs.iter().any(|held| held.id == run.id));
        let bbox = member_bbox(&hover.member).expect("a block has a box");
        assert!(bbox.x <= run.bbox.x && bbox.width >= run.bbox.width);
        assert_eq!(resolve(&content, 3, (1.0, 1.0), 1.0), None);
    }

    fn hovered(viewer: &Viewer) -> Option<Hover> {
        viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.content_hover.clone())
    }

    /// The pointer over a run outlines it; leaving the page lets it go.
    #[gtk::test]
    fn gtk_ui_the_pointer_over_text_hovers_it_until_it_leaves() {
        let fixture = Fixture::new(PageRotation::None);
        let (x, y) = fixture.on_screen(Fixture::middle(&fixture.runs[0]));

        track(fixture.viewer(), 0, x, y);
        let hover = hovered(fixture.viewer()).expect("the run is hovered");
        assert!(matches!(&hover.member, Member::Runs(runs) if runs[0].id == fixture.runs[0].id));

        leave(fixture.viewer(), 0);
        assert_eq!(hovered(fixture.viewer()), None);
    }

    /// The pointer is mapped through the page's turn, so on a rotated page
    /// the run drawn under it is still the one hovered.
    #[gtk::test]
    fn gtk_ui_a_rotated_page_hovers_the_run_drawn_under_the_pointer() {
        for rotation in [
            PageRotation::Clockwise90,
            PageRotation::Clockwise180,
            PageRotation::Clockwise270,
        ] {
            let fixture = Fixture::new(rotation);
            let target = &fixture.runs[1];
            let (x, y) = fixture.on_screen(Fixture::middle(target));

            track(fixture.viewer(), 0, x, y);

            let hover = hovered(fixture.viewer()).expect("the run is hovered");
            assert!(
                matches!(&hover.member, Member::Runs(runs) if runs.iter().any(|run| run.id == target.id)),
                "{rotation:?} hovered {:?}",
                hover.member
            );
        }
    }

    /// Leaving the mode, or arming an insertion, drops the outline; while
    /// either holds, the pointer outlines nothing.
    #[gtk::test]
    fn gtk_ui_the_hover_ends_with_the_mode_and_under_an_armed_insert() {
        let fixture = Fixture::new(PageRotation::None);
        let (x, y) = fixture.on_screen(Fixture::middle(&fixture.runs[0]));
        track(fixture.viewer(), 0, x, y);

        super::super::set_insert_mode(
            fixture.viewer(),
            Some(crate::app::state::ContentInsertKind::Text),
        );
        assert_eq!(hovered(fixture.viewer()), None);
        track(fixture.viewer(), 0, x, y);
        assert_eq!(hovered(fixture.viewer()), None);

        super::super::set_insert_mode(fixture.viewer(), None);
        track(fixture.viewer(), 0, x, y);
        assert!(hovered(fixture.viewer()).is_some());
        super::super::set_mode(fixture.viewer(), false);
        assert_eq!(hovered(fixture.viewer()), None);
    }
}
