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
//!
//! ## The border band
//!
//! A band [`BORDER_BAND_PX`] wide either side of the outline is the item's
//! *border*: the pointer turns into the move cursor there, and a press there
//! moves the whole item at once ([`begin_border_move`]) through the same
//! drag every kind already has — never the text retype, and never a marquee,
//! even where the band reaches outside the item's own box. Inside the band
//! the item behaves as it always has. The outer half of the band belongs to
//! the item already hovered: the hit-test cannot see it, so the hover holds
//! on while the pointer is still in it.
//!
//! The corners of a selected image or shape keep their resize handles: a
//! corner there is [`Zone::Handle`], not border.

use gtk::prelude::*;
use gtk::{cairo, gdk, glib, DrawingArea, EventControllerMotion};
use pdf_document::{ContentItemId, Rect, TextRun};
use pdf_render::{place_rect, TextRect};

use crate::app::selection::{handle_reach, pointer_to_pdf};
use crate::app::state::{DocumentSession, PageSlot, Viewer};

use super::multi::{self, Member};
use super::{geometry, image, model, path, text};

/// How far the outline sits outside the item's box, in device pixels — so
/// it does not run through the glyphs or the picture's own edge.
pub(crate) const OUTLINE_PAD_PX: f64 = 2.0;
/// The outline itself: one device pixel of the handle blue.
const OUTLINE_WIDTH_PX: f64 = 1.0;
const OUTLINE_RGB: (f64, f64, f64) = (0.10, 0.35, 0.85);
/// How far either side of the outline the border reaches, in device pixels.
pub(crate) const BORDER_BAND_PX: f64 = 4.0;
/// The move cursor, by the names a cursor theme may know it under: the CSS
/// name first, then the older ones.
const MOVE_CURSOR_NAMES: [&str; 3] = ["move", "all-scroll", "fleur"];

/// Which part of the hovered item the pointer is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Zone {
    /// Inside the item, clear of its border: a press does what it always
    /// has — selects text, grabs a picture.
    Inside,
    /// On the band round the outline: a press moves the whole item.
    Border,
    /// On a resize handle of the selected image or shape, which outranks
    /// the border it overlaps.
    Handle,
}

/// The item under the pointer, on the page it was found on, and which part
/// of it the pointer is on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hover {
    pub(crate) page_index: usize,
    pub(crate) member: Member,
    pub(crate) zone: Zone,
}

/// The items that carry resize handles right now: the selected image and
/// the selected shape, on the page being hovered.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Handles {
    pub(crate) image: Option<ContentItemId>,
    pub(crate) path: Option<ContentItemId>,
}

impl Handles {
    fn of(session: &DocumentSession, page_index: usize) -> Self {
        Self {
            image: session
                .selected_image
                .as_ref()
                .filter(|selected| selected.page_index == page_index)
                .map(|selected| selected.item.id),
            path: session
                .selected_path
                .as_ref()
                .filter(|selected| selected.page_index == page_index)
                .map(|selected| selected.item.id),
        }
    }

    fn carries(self, member: &Member) -> bool {
        match member {
            Member::Image(image) => self.image == Some(image.id),
            Member::Path(path) => self.path == Some(path.id),
            Member::Runs(_) => false,
        }
    }
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

/// Whether `member` belongs to the several-item selection held on
/// `page_index`.
fn in_held_group(session: &DocumentSession, page_index: usize, member: &Member) -> bool {
    let Some(held) = multi::held_on(session, page_index) else {
        return false;
    };
    match member {
        Member::Runs(runs) => runs
            .iter()
            .any(|run| held.runs.iter().any(|item| item.id == run.id)),
        Member::Image(image) => held.images.iter().any(|item| item.id == image.id),
        Member::Path(path) => held.paths.iter().any(|item| item.id == path.id),
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

/// How far `point` is from the nearest edge of `rect` — from inside or
/// outside alike.
pub(crate) fn edge_distance(rect: Rect, (x, y): (f64, f64)) -> f64 {
    let (left, right) = (rect.x, rect.x + rect.width);
    let (bottom, top) = (rect.y, rect.y + rect.height);
    let inside = (left..=right).contains(&x) && (bottom..=top).contains(&y);
    if inside {
        return (x - left).min(right - x).min(y - bottom).min(top - y);
    }
    let dx = (left - x).max(0.0).max(x - right);
    let dy = (bottom - y).max(0.0).max(y - top);
    dx.hypot(dy)
}

/// Where `point` falls against an item boxed by `bbox` at `scale` device
/// pixels per point: on its border band, inside it, or clear of both.
/// Rotation needs no say: a page turns by quarter turns, which keep every
/// distance.
pub(crate) fn classify(bbox: Rect, point: (f64, f64), scale: f64) -> Option<Zone> {
    let outline = outline_rect(bbox, scale);
    if edge_distance(outline, point) <= BORDER_BAND_PX / usable_scale(scale) {
        Some(Zone::Border)
    } else if geometry::contains(outline, point) {
        Some(Zone::Inside)
    } else {
        None
    }
}

/// The zone `point` is in on `member`, the handles outranking the band.
fn zone_of(
    member: &Member,
    point: (f64, f64),
    scale: f64,
    reach: f64,
    handles: Handles,
) -> Option<Zone> {
    let bbox = member_bbox(member)?;
    if handles.carries(member) && geometry::corner_at(bbox, point, reach).is_some() {
        return Some(Zone::Handle);
    }
    classify(bbox, point, scale)
}

/// What the pointer at `point` (PDF page space) rests on in `content`.
///
/// The hit-test is the SHIFT+click one. When it finds nothing, the item
/// already hovered (`current`) holds while the pointer is on its border
/// band — the band's outer half lies beyond the item's own box, where no
/// hit-test reaches. A path the hit-test grabs within `reach` of its edges
/// can lie beyond the band too; that still counts as its border.
pub(crate) fn resolve(
    content: &pdf_document::PageContent,
    current: Option<&Hover>,
    page_index: usize,
    point: (f64, f64),
    scale: f64,
    reach: f64,
    handles: Handles,
) -> Option<Hover> {
    if let Some(member) = multi::member_at(content, point, reach) {
        let zone = zone_of(&member, point, scale, reach, handles).unwrap_or(Zone::Border);
        return Some(Hover {
            page_index,
            member,
            zone,
        });
    }
    held_on_band(current, page_index, point, scale, reach, handles)
}

/// `current`, still hovered while `point` is on its border band.
fn held_on_band(
    current: Option<&Hover>,
    page_index: usize,
    point: (f64, f64),
    scale: f64,
    reach: f64,
    handles: Handles,
) -> Option<Hover> {
    let held = current.filter(|hover| hover.page_index == page_index)?;
    let zone =
        zone_of(&held.member, point, scale, reach, handles).filter(|zone| *zone != Zone::Inside)?;
    Some(Hover {
        page_index,
        member: held.member.clone(),
        zone,
    })
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

/// The item whose outer band holds `point` with nothing hovered: whatever
/// lies a band's width away along either axis, if `point` is on its band.
fn band_owner(
    content: &pdf_document::PageContent,
    page_index: usize,
    point: (f64, f64),
    scale: f64,
    reach: f64,
    handles: Handles,
) -> Option<Hover> {
    let step = (OUTLINE_PAD_PX + BORDER_BAND_PX + 1.0) / usable_scale(scale);
    [(step, 0.0), (-step, 0.0), (0.0, step), (0.0, -step)]
        .into_iter()
        .find_map(|(dx, dy)| {
            let member = multi::member_at(content, (point.0 + dx, point.1 + dy), reach)?;
            let candidate = Hover {
                page_index,
                member,
                zone: Zone::Inside,
            };
            held_on_band(Some(&candidate), page_index, point, scale, reach, handles)
        })
}

/// Wires the hover to one page's highlight layer.
///
/// The cursor goes on the layer itself, not on an ancestor: a press makes
/// the layer the grab widget, and GTK stops looking for a cursor there.
pub(crate) fn connect(area: &DrawingArea, viewer: &Viewer, page_index: usize) {
    let motion = EventControllerMotion::new();
    motion.connect_motion({
        let viewer = viewer.clone();
        move |controller, x, y| {
            if let Some(area) = controller.widget() {
                pointer_moved(&viewer, &area, page_index, x, y);
            }
        }
    });
    motion.connect_leave({
        let viewer = viewer.clone();
        move |controller| {
            leave(&viewer, page_index);
            if let Some(area) = controller.widget() {
                set_cursor(&area, None);
            }
        }
    });
    area.add_controller(motion);
}

/// The pointer moved to `(x, y)` on `area`, `page_index`'s layer: follow it,
/// and show the cursor for whatever it now rests on.
pub(crate) fn pointer_moved(
    viewer: &Viewer,
    area: &impl IsA<gtk::Widget>,
    page_index: usize,
    x: f64,
    y: f64,
) {
    follow_pointer(viewer, area, page_index, x, y, false);
}

/// [`pointer_moved`]; `refind` as for [`track`].
fn follow_pointer(
    viewer: &Viewer,
    area: &impl IsA<gtk::Widget>,
    page_index: usize,
    x: f64,
    y: f64,
    refind: bool,
) {
    if track(viewer, page_index, x, y, refind) {
        let zone = viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.content_hover.as_ref())
            .filter(|hover| hover.page_index == page_index)
            .map(|hover| hover.zone);
        set_cursor(area, zone);
    }
}

/// The cursor names for `zone`, best first; none means the default arrow.
/// Only the border moves an item as a whole, so only it changes the cursor.
pub(crate) fn cursor_names(zone: Option<Zone>) -> &'static [&'static str] {
    match zone {
        Some(Zone::Border) => &MOVE_CURSOR_NAMES,
        Some(Zone::Inside | Zone::Handle) | None => &[],
    }
}

fn set_cursor(area: &impl IsA<gtk::Widget>, zone: Option<Zone>) {
    // `from_name` never fails, so the later names ride as fallbacks of the
    // first rather than being tried in turn.
    let cursor = cursor_names(zone)
        .iter()
        .rev()
        .fold(None, |fallback, name| {
            gdk::Cursor::from_name(name, fallback.as_ref())
        });
    area.set_cursor(cursor.as_ref());
}

/// Follows the pointer to `(x, y)` on `page_index`'s layer; `refind` lets
/// the outer band count with nothing hovered ([`band_owner`]). Returns
/// whether the hover changed.
fn track(viewer: &Viewer, page_index: usize, x: f64, y: f64, refind: bool) -> bool {
    let armed = {
        let mut state = viewer.state.borrow_mut();
        if let Some(session) = state.session.as_mut() {
            session.content_hover_pointer = Some((page_index, x, y));
        }
        state.content_edit_mode && state.content_insert_mode.is_none()
    };
    if !armed {
        return clear(viewer);
    }
    let reach = handle_reach(viewer, page_index);
    let Some(point) = pointer_to_pdf(viewer, page_index, x, y) else {
        return false;
    };
    let changed = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return false;
        };
        if dragging(session) {
            return false;
        }
        // An open editor owns the pointer: a border press commits it, which
        // can re-parse the page under whatever the hover holds.
        if session.content_editor.is_some() {
            drop(state);
            return clear(viewer);
        }
        let handles = Handles::of(session, page_index);
        let hover = session.pages.get(page_index).and_then(|page| {
            let (content, scale) = (page.content.as_ref()?, page.budget.factor);
            let current = session.content_hover.as_ref();
            resolve(content, current, page_index, point, scale, reach, handles).or_else(|| {
                (refind && current.is_none())
                    .then(|| band_owner(content, page_index, point, scale, reach, handles))
                    .flatten()
            })
        });
        if session.content_hover == hover {
            return false;
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
    true
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
            .content_hover_pointer
            .is_some_and(|(page, _, _)| page == page_index)
        {
            session.content_hover_pointer = None;
        }
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

/// Drops the hover and its cursor — the mode went off, or an insert kind
/// was armed and every click now places something new rather than grabbing
/// what is there. Returns whether there was one.
pub(crate) fn clear(viewer: &Viewer) -> bool {
    let area = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return false;
        };
        let Some(previous) = session.content_hover.take() else {
            return false;
        };
        session
            .pages
            .get(previous.page_index)
            .map(|page| page.highlights.clone())
    };
    if let Some(area) = area {
        set_cursor(&area, None);
        area.queue_draw();
    }
    true
}

/// Drops the hover, its outline and its move cursor because the page
/// content it was read from changed: an edit was recorded, or the content
/// was dropped to be parsed again. Content ids are positional, so after a
/// re-parse the hover's ids could name different items. The next pointer
/// motion finds the hover again.
///
/// Takes the session rather than the viewer: every caller already holds it
/// borrowed. A cursor change and a queued draw call nothing back into it.
pub(crate) fn forget(session: &mut DocumentSession) {
    let Some(previous) = session.content_hover.take() else {
        return;
    };
    if let Some(page) = session.pages.get(previous.page_index) {
        set_cursor(&page.highlights, None);
        page.highlights.queue_draw();
    }
}

/// Finds the hover again on the next idle where the pointer last rested,
/// once an edit dropped it ([`forget`]) and the page was parsed again: the
/// outline and move cursor follow a moved item, even one moved by its outer
/// band, without the pointer moving. [`track`]'s guards all still apply.
pub(crate) fn restore_soon(viewer: &Viewer) {
    let viewer = viewer.clone();
    glib::idle_add_local_once(move || restore(&viewer));
}

fn restore(viewer: &Viewer) {
    let (page_index, x, y, area) = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return;
        };
        let Some((page_index, x, y)) = session.content_hover_pointer else {
            return;
        };
        let Some(page) = session.pages.get(page_index) else {
            return;
        };
        (page_index, x, y, page.highlights.clone())
    };
    follow_pointer(viewer, &area, page_index, x, y, true);
}

/// Whether `member` is still on the page as `content` has it now: every
/// item with the same id and in the same place. A hover read from content
/// that has since been parsed again fails this.
fn still_current(content: &pdf_document::PageContent, member: &Member) -> bool {
    match member {
        Member::Runs(runs) => {
            !runs.is_empty()
                && runs.iter().all(|run| {
                    content
                        .text_run(run.id)
                        .is_some_and(|now| now.bbox == run.bbox)
                })
        }
        Member::Image(item) => content
            .images
            .iter()
            .any(|now| now.id == item.id && now.bbox == item.bbox),
        Member::Path(item) => content
            .paths
            .iter()
            .any(|now| now.id == item.id && now.bbox == item.bbox),
    }
}

/// What a press on the hovered item's border grabbed — so the caller can let
/// go of whatever else was selected, as it does for the same press inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grabbed {
    Image,
    Text,
    Path,
}

/// The hovered item on `page_index`, when `point` is on its border band and
/// the item is still where the page's current content has it.
pub(super) fn border_member(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> Option<Member> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    if session.content_editor.is_some() {
        return None;
    }
    let hover = session
        .content_hover
        .as_ref()
        .filter(|hover| hover.page_index == page_index)?;
    let page = session.pages.get(page_index)?;
    if !still_current(page.content.as_ref()?, &hover.member) {
        return None;
    }
    let scale = page.budget.factor;
    let handles = Handles::of(session, page_index);
    (zone_of(&hover.member, point, scale, reach, handles) == Some(Zone::Border))
        .then(|| hover.member.clone())
}

/// A press at `point` on the hovered item's border band moves the whole
/// item, whichever part of the band it landed on — the stretch outside the
/// item's own box included, which no hit-test would hand to it. Each kind
/// goes through its own drag (`text`, `image`, `path`), picked by id from
/// the page as parsed now rather than from the snapshot the hover holds.
///
/// `None` leaves the press to the ordinary dispatch.
pub(crate) fn begin_border_move(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> Option<Grabbed> {
    match border_member(viewer, page_index, point, reach)? {
        Member::Runs(runs) => text::begin_block_move(viewer, page_index, point, runs.first()?.id)
            .then_some(Grabbed::Text),
        Member::Image(item) => {
            image::begin_image_move(viewer, page_index, point, item.id).then_some(Grabbed::Image)
        }
        Member::Path(item) => {
            path::begin_path_move(viewer, page_index, point, item.id).then_some(Grabbed::Path)
        }
    }
}

/// The text block whose border band a click at `point` landed on, as
/// `block::select` takes it: the stand-in and its runs. A click there is
/// aimed at the block, even off its own box, so it selects it rather than
/// clearing the page.
pub(crate) fn border_block(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
) -> Option<(TextRun, Vec<TextRun>)> {
    let reach = handle_reach(viewer, page_index);
    let Member::Runs(runs) = border_member(viewer, page_index, point, reach)? else {
        return None;
    };
    let state = viewer.state.borrow();
    let content = state
        .session
        .as_ref()?
        .pages
        .get(page_index)?
        .content
        .as_ref()?;
    let run = content.text_run(runs.first()?.id)?.clone();
    let block = model::block_runs(content, &run);
    Some((model::block_stand_in(run, &block), block))
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
        // A member of the held group draws no box of its own: the group
        // frame is the only thing drawn round its items.
        .filter(|hover| !in_held_group(session, page_index, &hover.member))
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

        let hover = resolve(&content, None, 3, middle, 1.0, 1.0, Handles::default())
            .expect("the run is under the pointer");

        assert_eq!(hover.page_index, 3);
        assert_eq!(hover.zone, Zone::Inside);
        let Member::Runs(runs) = &hover.member else {
            panic!("a run resolves to its text block, got {:?}", hover.member);
        };
        assert!(runs.iter().any(|held| held.id == run.id));
        let bbox = member_bbox(&hover.member).expect("a block has a box");
        assert!(bbox.x <= run.bbox.x && bbox.width >= run.bbox.width);
        assert_eq!(
            resolve(&content, None, 3, (1.0, 1.0), 1.0, 1.0, Handles::default()),
            None
        );
    }

    #[test]
    fn edge_distance_is_measured_from_inside_and_outside_alike() {
        let area = rect(0.0, 0.0, 100.0, 50.0);

        assert_eq!(edge_distance(area, (50.0, 25.0)), 25.0);
        assert_eq!(edge_distance(area, (3.0, 25.0)), 3.0);
        assert_eq!(edge_distance(area, (100.0, 25.0)), 0.0);
        assert_eq!(edge_distance(area, (104.0, 25.0)), 4.0);
        assert_eq!(edge_distance(area, (103.0, 54.0)), 5.0);
    }

    /// The band is four screen pixels either side of the outline, which
    /// itself sits two outside the box: it reaches two pixels into the box
    /// and six out of it, at any zoom.
    #[test]
    fn the_border_band_straddles_the_outline_in_screen_pixels() {
        let bbox = rect(100.0, 100.0, 100.0, 40.0);
        let middle_y = 120.0;

        assert_eq!(classify(bbox, (150.0, middle_y), 1.0), Some(Zone::Inside));
        assert_eq!(classify(bbox, (101.9, middle_y), 1.0), Some(Zone::Border));
        assert_eq!(classify(bbox, (102.5, middle_y), 1.0), Some(Zone::Inside));
        assert_eq!(classify(bbox, (94.1, middle_y), 1.0), Some(Zone::Border));
        assert_eq!(classify(bbox, (93.5, middle_y), 1.0), None);

        // At 2x a screen pixel is half a point: the band is half as wide.
        assert_eq!(classify(bbox, (97.1, middle_y), 2.0), Some(Zone::Border));
        assert_eq!(classify(bbox, (96.5, middle_y), 2.0), None);
        assert_eq!(classify(bbox, (101.2, middle_y), 2.0), Some(Zone::Inside));
    }

    /// A line too thin to have an inside is border all through, so it can
    /// always be grabbed and moved.
    #[test]
    fn a_hairline_is_border_all_through() {
        let rule = rect(0.0, 100.0, 200.0, 0.0);

        assert_eq!(classify(rule, (100.0, 100.0), 1.0), Some(Zone::Border));
    }

    fn an_image(id: u64, bbox: Rect) -> pdf_document::ImageItem {
        pdf_document::ImageItem {
            id: ContentItemId(id),
            page: PageId(0),
            bbox,
            source: pdf_document::ImageSource::Inline,
        }
    }

    /// The selected image keeps its resize handles: a corner is a handle,
    /// not border. Another image's corner is plain border.
    #[test]
    fn a_selected_images_corner_is_a_handle_not_border() {
        let bbox = rect(100.0, 100.0, 80.0, 60.0);
        let member = Member::Image(an_image(7, bbox));
        let corner = (99.0, 99.0);
        let selected = Handles {
            image: Some(ContentItemId(7)),
            path: None,
        };

        assert_eq!(
            zone_of(&member, corner, 1.0, 8.0, selected),
            Some(Zone::Handle)
        );
        assert_eq!(
            zone_of(&member, corner, 1.0, 8.0, Handles::default()),
            Some(Zone::Border)
        );
        assert_eq!(
            zone_of(&member, (99.0, 130.0), 1.0, 8.0, selected),
            Some(Zone::Border)
        );
    }

    /// Off the item's own box the hit-test finds nothing, so the item
    /// already hovered holds while the pointer is still on its band, and
    /// only then.
    #[test]
    fn the_outer_band_keeps_the_item_already_hovered() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let content = pdf_edit::read_page_content(&base, PageId(0)).expect("the fixture parses");
        let inside = Fixture::middle(&content.text_runs[0]);
        let current = resolve(&content, None, 0, inside, 1.0, 1.0, Handles::default());
        let bbox = member_bbox(&current.as_ref().expect("hovered").member).expect("a box");
        let just_left = (bbox.x - 4.0, inside.1);
        let far_left = (bbox.x - 10.0, inside.1);
        let at = |current: Option<&Hover>, page: usize, point: (f64, f64)| {
            resolve(&content, current, page, point, 1.0, 1.0, Handles::default())
        };

        let held = at(current.as_ref(), 0, just_left).expect("the band holds the hover");
        assert_eq!(held.zone, Zone::Border);
        assert_eq!(at(None, 0, just_left), None);
        assert_eq!(at(current.as_ref(), 0, far_left), None);
        assert_eq!(
            at(current.as_ref(), 1, just_left),
            None,
            "a hover is held on its own page only"
        );
    }

    #[test]
    fn only_the_border_asks_for_the_move_cursor() {
        assert_eq!(cursor_names(Some(Zone::Border))[0], "move");
        assert!(cursor_names(Some(Zone::Border)).contains(&"fleur"));
        assert!(cursor_names(Some(Zone::Inside)).is_empty());
        assert!(cursor_names(Some(Zone::Handle)).is_empty());
        assert!(cursor_names(None).is_empty());
    }

    fn track(viewer: &Viewer, page_index: usize, x: f64, y: f64) -> bool {
        super::track(viewer, page_index, x, y, false)
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

    fn highlights(fixture: &Fixture) -> DrawingArea {
        fixture
            .viewer()
            .state
            .borrow()
            .session
            .as_ref()
            .expect("a session")
            .pages[0]
            .highlights
            .clone()
    }

    /// A point `by` points left of the first run, level with its middle.
    fn left_of_block(fixture: &Fixture, by: f64) -> (f64, f64) {
        let run = &fixture.runs[0];
        (run.bbox.x - by, Fixture::middle(run).1)
    }

    fn text_drag_run(viewer: &Viewer) -> Option<ContentItemId> {
        viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.text_drag.as_ref())
            .map(|drag| drag.run.id)
    }

    /// The move cursor shows on the border and nowhere else.
    #[gtk::test]
    fn gtk_ui_the_border_shows_the_move_cursor() {
        let fixture = Fixture::new(PageRotation::None);
        let area = highlights(&fixture);
        let inside = fixture.on_screen(Fixture::middle(&fixture.runs[0]));
        let border = fixture.on_screen(left_of_block(&fixture, 3.0));
        let cursor = |area: &DrawingArea| area.cursor().and_then(|cursor| cursor.name());

        pointer_moved(fixture.viewer(), &area, 0, inside.0, inside.1);
        assert_eq!(cursor(&area), None);
        pointer_moved(fixture.viewer(), &area, 0, border.0, border.1);
        let shown = cursor(&area).expect("the border sets a named cursor");
        assert!(MOVE_CURSOR_NAMES.contains(&shown.as_str()), "{shown}");
        pointer_moved(fixture.viewer(), &area, 0, 1.0, 1.0);
        assert_eq!(cursor(&area), None);
    }

    /// A press on the border band off the block's own box moves the block,
    /// where without the hover the same press starts a marquee.
    #[gtk::test]
    fn gtk_ui_a_press_on_the_border_band_moves_the_whole_block() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let press = left_of_block(&fixture, 3.0);

        assert!(super::super::begin_drag(viewer, 0, press, 8.0, false));
        assert_eq!(text_drag_run(viewer), None, "nothing hovered: a marquee");
        viewer
            .state
            .borrow_mut()
            .session
            .as_mut()
            .expect("a session")
            .selection_gesture = None;

        let inside = fixture.on_screen(Fixture::middle(&fixture.runs[0]));
        track(viewer, 0, inside.0, inside.1);
        let border = fixture.on_screen(press);
        track(viewer, 0, border.0, border.1);
        assert!(super::super::begin_drag(viewer, 0, press, 8.0, false));

        assert_eq!(text_drag_run(viewer), Some(fixture.runs[0].id));
        let state = viewer.state.borrow();
        let session = state.session.as_ref().expect("a session");
        assert!(session.selection_gesture.is_none());
        assert_eq!(
            session.text_drag.as_ref().map(|drag| drag.origin),
            Some(press)
        );
    }

    /// A click on the border band selects the block, as a click on its text
    /// does, rather than clearing the page.
    #[gtk::test]
    fn gtk_ui_a_click_on_the_border_band_selects_the_block() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let inside = fixture.on_screen(Fixture::middle(&fixture.runs[0]));
        track(viewer, 0, inside.0, inside.1);
        let press = left_of_block(&fixture, 3.0);
        let border = fixture.on_screen(press);
        track(viewer, 0, border.0, border.1);

        super::super::resolve_click(viewer, 0, press);

        let state = viewer.state.borrow();
        let selected = state
            .session
            .as_ref()
            .and_then(|session| session.selected_text_block.as_ref())
            .expect("the click selects the block");
        assert_eq!(selected.run.id, fixture.runs[0].id);
    }

    /// Hovers the first block's border band, as a pointer resting there
    /// leaves it, and returns the band point (PDF page space).
    fn hover_border(fixture: &Fixture) -> (f64, f64) {
        let inside = fixture.on_screen(Fixture::middle(&fixture.runs[0]));
        track(fixture.viewer(), 0, inside.0, inside.1);
        let press = left_of_block(fixture, 3.0);
        let border = fixture.on_screen(press);
        pointer_moved(
            fixture.viewer(),
            &highlights(fixture),
            0,
            border.0,
            border.1,
        );
        assert_eq!(
            hovered(fixture.viewer()).map(|hover| hover.zone),
            Some(Zone::Border)
        );
        press
    }

    /// Forgetting the parsed content — a checkpoint swapped the base —
    /// drops the hover, its outline and its move cursor: the ids it holds
    /// may name different items once the page is parsed again.
    #[gtk::test]
    fn gtk_ui_forgetting_page_content_drops_the_hover_and_its_cursor() {
        let fixture = Fixture::new(PageRotation::None);
        hover_border(&fixture);
        let area = highlights(&fixture);
        assert!(area.cursor().is_some(), "the band shows the move cursor");

        super::super::checkpoint::forget_page_content(
            fixture
                .viewer()
                .state
                .borrow_mut()
                .session
                .as_mut()
                .expect("a session"),
        );

        assert_eq!(hovered(fixture.viewer()), None);
        assert_eq!(area.cursor(), None);
    }

    /// A content edit recorded with the pointer still lets the hover go:
    /// the outline would otherwise stay where the item used to be.
    #[gtk::test]
    fn gtk_ui_a_recorded_content_edit_drops_the_hover() {
        let fixture = Fixture::new(PageRotation::None);
        hover_border(&fixture);
        let run = fixture.runs[0].clone();

        {
            let mut state = fixture.viewer().state.borrow_mut();
            let session = state.session.as_mut().expect("a session");
            super::super::command::record(session, pdf_document::Command::RemoveTextRun(run))
                .expect("the delete records");
        }

        assert_eq!(hovered(fixture.viewer()), None);
        assert_eq!(highlights(&fixture).cursor(), None);
    }

    /// Content ids are positional: re-parsed under a stale hover, the id it
    /// holds can name another item. A press on the old band must not move
    /// that one — it falls through to the ordinary press, a marquee here.
    #[gtk::test]
    fn gtk_ui_a_stale_hover_does_not_move_whatever_now_has_its_id() {
        let fixture = Fixture::new(PageRotation::None);
        let press = hover_border(&fixture);
        {
            let mut state = fixture.viewer().state.borrow_mut();
            let session = state.session.as_mut().expect("a session");
            let content = session.pages[0].content.as_mut().expect("parsed");
            let (first, second) = (content.text_runs[0].id, content.text_runs[1].id);
            content.text_runs[0].id = second;
            content.text_runs[1].id = first;
        }

        assert!(super::super::begin_drag(
            fixture.viewer(),
            0,
            press,
            8.0,
            false
        ));

        let state = fixture.viewer().state.borrow();
        let session = state.session.as_ref().expect("a session");
        assert!(
            session.text_drag.is_none(),
            "{:?}",
            text_drag_run(fixture.viewer())
        );
        assert!(session.selection_gesture.is_some(), "a marquee");
    }

    /// A block moved by its outer band is outlined again where it landed,
    /// with the move cursor, once the refresh has re-parsed the page —
    /// without the pointer moving off the spot it was released on.
    #[gtk::test]
    fn gtk_ui_the_hover_follows_a_border_move_once_the_page_reloads() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        // The test plays the pointer: the real one must not reach the layer
        // while the main loop runs.
        highlights(&fixture).set_can_target(false);
        let context = glib::MainContext::default();
        while context.iteration(false) {}
        let press = hover_border(&fixture);
        let to = (press.0 + 30.0, press.1);
        assert!(super::super::begin_drag(viewer, 0, press, 8.0, false));
        let released = fixture.on_screen(to);
        track(viewer, 0, released.0, released.1);
        super::super::extend_drag(viewer, to);
        // Held back so the move queues its refresh instead of reopening
        // anything through pdfium in a test.
        viewer.state.borrow_mut().preview_refresh_in_flight = true;
        assert!(super::super::text::finish_text_drag(viewer, 30.0, 0.0));
        assert_eq!(hovered(viewer), None, "the edit drops the hover");

        // What the refresh does once it lands.
        super::super::checkpoint::forget_page_content(
            viewer
                .state
                .borrow_mut()
                .session
                .as_mut()
                .expect("a session"),
        );
        super::super::reload_after_refresh(viewer);
        while context.iteration(false) {}

        let hover = hovered(viewer).expect("the moved block is hovered again");
        assert_eq!(hover.zone, Zone::Border);
        let bbox = member_bbox(&hover.member).expect("a box");
        assert!(
            (bbox.x - (fixture.runs[0].bbox.x + 30.0)).abs() < 0.5,
            "{bbox:?}"
        );
        let shown = highlights(&fixture)
            .cursor()
            .and_then(|cursor| cursor.name());
        assert!(shown.is_some_and(|name| MOVE_CURSOR_NAMES.contains(&name.as_str())));
    }

    /// SHIFT+press on the hovered block's outer band, off its own box,
    /// toggles the block into the selection as a press on its text does.
    #[gtk::test]
    fn gtk_ui_a_shift_press_on_the_outer_band_toggles_the_block() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let press = hover_border(&fixture);

        assert!(super::super::begin_drag(viewer, 0, press, 8.0, true));

        let state = viewer.state.borrow();
        let selection = state
            .session
            .as_ref()
            .and_then(|session| session.content_selection.as_ref())
            .expect("the press toggles the block in");
        assert!(matches!(
            selection.members.runs.as_slice(),
            [run, ..] if run.id == fixture.runs[0].id
        ));
    }

    /// An open editor stands the hover down: a border press commits the
    /// editor first, which can re-parse the page under the id the hover
    /// holds, so it must not start a border move at all.
    #[gtk::test]
    fn gtk_ui_an_open_editor_stands_the_hover_down() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let press = hover_border(&fixture);
        super::super::block::open_editor_at(viewer, 0, Fixture::middle(&fixture.runs[1]));
        assert!(viewer
            .state
            .borrow()
            .session
            .as_ref()
            .is_some_and(|session| session.content_editor.is_some()));

        assert_eq!(begin_border_move(viewer, 0, press, 8.0), None);
        let border = fixture.on_screen(press);
        track(viewer, 0, border.0, border.1);
        assert_eq!(hovered(viewer), None);
    }

    /// The cursor the pointer actually shows over the hovered block's border
    /// band: GTK takes it from the widget the pointer picks, or the nearest
    /// ancestor with one, so the move cursor only shows if nothing stacked
    /// over the page's layer is picked there instead.
    #[gtk::test]
    fn gtk_ui_the_widget_picked_on_the_border_shows_the_move_cursor() {
        let fixture = Fixture::new(PageRotation::None);
        let layer = highlights(&fixture);
        // The fixture's page has no raster to size it; give the layer the
        // A4 box the page would have.
        layer.set_size_request(595, 842);
        let context = glib::MainContext::default();
        while context.iteration(false) {}
        let press = hover_border(&fixture);
        let (x, y) = fixture.on_screen(press);

        let window = &fixture.built.window;
        let at = layer
            .compute_point(window, &gtk::graphene::Point::new(x as f32, y as f32))
            .expect("the layer is in the window");
        let picked = window
            .pick(
                f64::from(at.x()),
                f64::from(at.y()),
                gtk::PickFlags::DEFAULT,
            )
            .expect("something is under the pointer");
        let shown = std::iter::successors(Some(picked.clone()), |widget| widget.parent())
            .find_map(|widget| widget.cursor())
            .and_then(|cursor| cursor.name());

        assert_eq!(
            picked,
            layer.clone().upcast::<gtk::Widget>(),
            "picked {picked:?}"
        );
        assert!(
            shown.is_some_and(|name| MOVE_CURSOR_NAMES.contains(&name.as_str())),
            "the picked widget shows the default cursor"
        );
    }
}
