//! Selecting several content items and moving or deleting them at once
//! (B25 T-208): what a marquee or SHIFT+click gathers on one page — text
//! blocks, images and vector paths. The Linux twin of Windows'
//! `ContentSelection` gestures and macOS's (#512).
//!
//! - A press on empty page starts a **marquee**. It selects every item whose
//!   box touches it, inclusively, so a hairline rule counts; a text run
//!   brings its whole block.
//! - **SHIFT+click** adds the item under the pointer, or takes it out.
//! - A drag inside the dashed box around the selection **moves** all of it;
//!   **Delete** removes it and **Esc** clears it.
//!
//! Every edit is one `Command::Batch` built by `pdf_edit::selection` and
//! recorded through `pdf_save::record_content_batch`, so a selection dragged
//! twice before a save is still one undo step.
//!
//! As elsewhere in this module, the `&Viewer` functions stay thin; what a
//! press, a marquee or a toggle means is pure and carries the tests.

use gtk::gio;
use gtk::prelude::*;
use pdf_document::{ImageItem, PageContent, PathItem, Rect, TextRun};
use pdf_edit::Selection;

use crate::app::selection;
use crate::app::state::{ContentSelection, SelectionGesture, Viewer};
use crate::app::update_content_edit_controls;
use crate::app::write::refresh_preview;

use super::{command, editor, geometry, model, path};

/// The window action Esc is bound to while a selection exists.
const CLEAR_ACTION: &str = "clear-content-selection";

/// A marquee or a group drag shorter than this, in PDF points on both axes,
/// is a click.
const CLICK_PT: f64 = 1.0;

/// One thing a SHIFT+click adds or takes out: a whole text block, an image,
/// or a path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Member {
    Runs(Vec<TextRun>),
    Image(ImageItem),
    Path(PathItem),
}

/// Whether two boxes touch, edges included — a flat rule's box has no area
/// and must still count.
pub(crate) fn touches(a: Rect, b: Rect) -> bool {
    a.x <= b.x + b.width && b.x <= a.x + a.width && a.y <= b.y + b.height && b.y <= a.y + a.height
}

/// The box a marquee from `origin` to `current` covers.
pub(crate) fn marquee_rect(origin: (f64, f64), current: (f64, f64)) -> Rect {
    Rect {
        x: origin.0.min(current.0),
        y: origin.1.min(current.1),
        width: (origin.0 - current.0).abs(),
        height: (origin.1 - current.1).abs(),
    }
}

/// Every item on `content` whose box touches `area`. A text run that
/// touches it brings the rest of its block.
pub(crate) fn members_in(content: &PageContent, area: Rect) -> Selection {
    let mut runs = Vec::new();
    for block in pdf_edit::group_text_runs(&content.text_runs) {
        if touches(block.bbox, area) {
            runs.extend(
                block
                    .runs
                    .iter()
                    .filter_map(|id| content.text_run(*id).cloned()),
            );
        }
    }
    Selection {
        runs,
        images: content
            .images
            .iter()
            .filter(|image| touches(image.bbox, area))
            .cloned()
            .collect(),
        paths: content
            .paths
            .iter()
            .filter(|path| touches(path.bbox, area))
            .cloned()
            .collect(),
    }
}

/// The item a SHIFT+click at `point` means, in the order a plain press asks:
/// an image, then text (as its whole block), then a path.
pub(crate) fn member_at(content: &PageContent, point: (f64, f64), reach: f64) -> Option<Member> {
    let at = (point.0 as f32, point.1 as f32);
    if let Some(image) = model::image_at(content, at) {
        return Some(Member::Image(image.clone()));
    }
    if let Some(run) = model::text_run_at(content, at) {
        let block = model::block_runs(content, run);
        return Some(Member::Runs(if block.is_empty() {
            vec![run.clone()]
        } else {
            block
        }));
    }
    path::path_at(&content.paths, point, None, reach)
        .cloned()
        .map(Member::Path)
}

/// `selection` with `member` added, or taken out if it was already in.
pub(crate) fn toggled(mut selection: Selection, member: Member) -> Selection {
    match member {
        Member::Runs(runs) => {
            let already = runs
                .iter()
                .all(|run| selection.runs.iter().any(|held| held.id == run.id));
            if already {
                selection
                    .runs
                    .retain(|held| !runs.iter().any(|run| run.id == held.id));
            } else {
                for run in runs {
                    if !selection.runs.iter().any(|held| held.id == run.id) {
                        selection.runs.push(run);
                    }
                }
            }
        }
        Member::Image(image) => {
            if let Some(at) = selection.images.iter().position(|held| held.id == image.id) {
                selection.images.remove(at);
            } else {
                selection.images.push(image);
            }
        }
        Member::Path(path) => {
            if let Some(at) = selection.paths.iter().position(|held| held.id == path.id) {
                selection.paths.remove(at);
            } else {
                selection.paths.push(path);
            }
        }
    }
    selection
}

/// The union of every member's box — the dashed box a group drag grabs.
pub(crate) fn union(selection: &Selection) -> Option<Rect> {
    selection
        .runs
        .iter()
        .map(|run| run.bbox)
        .chain(selection.images.iter().map(|image| image.bbox))
        .chain(selection.paths.iter().map(|path| path.bbox))
        .reduce(|union, bbox| {
            let left = union.x.min(bbox.x);
            let bottom = union.y.min(bbox.y);
            let right = (union.x + union.width).max(bbox.x + bbox.width);
            let top = (union.y + union.height).max(bbox.y + bbox.height);
            Rect {
                x: left,
                y: bottom,
                width: right - left,
                height: top - bottom,
            }
        })
}

/// `selection` with every member's box moved by `(dx, dy)` — what the
/// pending overlay reads back after a group move, under the same ids.
pub(crate) fn translated(selection: &Selection, dx: f64, dy: f64) -> Selection {
    let shift = |bbox: Rect| Rect {
        x: bbox.x + dx,
        y: bbox.y + dy,
        ..bbox
    };
    Selection {
        runs: selection
            .runs
            .iter()
            .map(|run| TextRun {
                bbox: shift(run.bbox),
                ..run.clone()
            })
            .collect(),
        images: selection
            .images
            .iter()
            .map(|image| ImageItem {
                bbox: shift(image.bbox),
                ..image.clone()
            })
            .collect(),
        paths: selection
            .paths
            .iter()
            .map(|path| PathItem {
                bbox: shift(path.bbox),
                ..path.clone()
            })
            .collect(),
    }
}

/// How many items a selection holds, counting a text block as one.
pub(crate) fn item_count(selection: &Selection) -> usize {
    pdf_edit::group_text_runs(&selection.runs).len()
        + selection.images.len()
        + selection.paths.len()
}

fn selected_message(selection: &Selection) -> String {
    let count = item_count(selection);
    format!(
        "{count} item{} selected — drag to move, Delete removes, Esc clears.",
        if count == 1 { "" } else { "s" }
    )
}

/// The page content a press on `page_index` is hit-tested against, parsed
/// on first need — the same door every gesture in this module uses.
fn with_content<R>(
    viewer: &Viewer,
    page_index: usize,
    read: impl FnOnce(&PageContent) -> R,
) -> Option<R> {
    let mut state = viewer.state.borrow_mut();
    let session = state.session.as_mut()?;
    let base = session.save_backing.as_ref().map(|backing| &backing.base)?;
    let document = session.document_model.as_ref()?;
    let pending = Some(&document.pending_edits);
    let page_id = super::content_page(session, page_index)?;
    let probe = super::page_probe(document, base, &session.imported_sources, page_id)?;
    let page = session.pages.get_mut(page_index)?;
    model::ensure_page_content(&mut page.content, probe, page_id, pending)
        .ok()
        .map(read)
}

/// SHIFT+click: adds the item under `point` to the selection, or takes it
/// out. An image or shape already selected on its own joins first, so the
/// gesture extends what the reader can see is selected. Claims the press
/// whenever content editing is possible, so a SHIFT press never starts
/// something else.
pub(crate) fn toggle_at(viewer: &Viewer, page_index: usize, point: (f64, f64), reach: f64) -> bool {
    if viewer.content_edit_refusal().is_some()
        || viewer.state.borrow().content_insert_mode.is_some()
    {
        return false;
    }
    editor::commit(viewer);
    if editor::keep_refused_editor(viewer) {
        return true;
    }
    let Some(member) = with_content(viewer, page_index, |content| {
        member_at(content, point, reach)
    }) else {
        return true;
    };
    {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return true;
        };
        let mut current = session
            .content_selection
            .take()
            .filter(|held| held.page_index == page_index)
            .map(|held| held.members)
            .unwrap_or_default();
        if let Some(image) = session.selected_image.take() {
            if image.page_index == page_index {
                current = toggled(current, Member::Image(image.item));
            }
        }
        if let Some(path) = session.selected_path.take() {
            if path.page_index == page_index {
                current = toggled(current, Member::Path(path.item));
            }
        }
        session.image_drag = None;
        session.path_drag = None;
        if let Some(member) = member {
            current = toggled(current, member);
        }
        if !current.is_empty() {
            viewer.status.set_text(&selected_message(&current));
            session.content_selection = Some(ContentSelection {
                page_index,
                members: current,
            });
        }
    }
    selection_changed(viewer);
    true
}

/// A press inside the dashed box around the selection on `page_index`
/// grabs all of it. Returns whether it did.
pub(crate) fn begin_group_move(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> bool {
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    let Some(bbox) = session
        .content_selection
        .as_ref()
        .filter(|held| held.page_index == page_index)
        .and_then(|held| union(&held.members))
    else {
        return false;
    };
    let grab = Rect {
        x: bbox.x - reach,
        y: bbox.y - reach,
        width: bbox.width + 2.0 * reach,
        height: bbox.height + 2.0 * reach,
    };
    if !geometry::contains(grab, point) {
        return false;
    }
    session.selection_gesture = Some(SelectionGesture::Move {
        page_index,
        origin: point,
        current: point,
    });
    true
}

/// A press that nothing else claimed starts a marquee — unless an insert is
/// armed, where every click places new content.
pub(crate) fn begin_marquee(viewer: &Viewer, page_index: usize, point: (f64, f64)) -> bool {
    if viewer.content_edit_refusal().is_some() {
        return false;
    }
    let mut state = viewer.state.borrow_mut();
    if state.content_insert_mode.is_some() {
        return false;
    }
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    session.selection_gesture = Some(SelectionGesture::Marquee {
        page_index,
        origin: point,
        current: point,
    });
    true
}

/// Extends the marquee or group drag in flight. Returns whether there was one.
pub(crate) fn extend(viewer: &Viewer, point: (f64, f64)) -> bool {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(gesture) = state
            .session
            .as_mut()
            .and_then(|session| session.selection_gesture.as_mut())
        else {
            return false;
        };
        match gesture {
            SelectionGesture::Marquee { current, .. } | SelectionGesture::Move { current, .. } => {
                *current = point;
            }
        }
    }
    selection::redraw(viewer);
    true
}

/// Finishes the marquee or group drag in flight. A marquee that never grew
/// is the click on empty page it began as, so it reports `false` and the
/// caller resolves that click as before.
pub(crate) fn finish(viewer: &Viewer) -> bool {
    let Some(gesture) = viewer
        .state
        .borrow_mut()
        .session
        .as_mut()
        .and_then(|session| session.selection_gesture.take())
    else {
        return false;
    };
    match gesture {
        SelectionGesture::Marquee {
            page_index,
            origin,
            current,
        } => finish_marquee(viewer, page_index, origin, current),
        SelectionGesture::Move {
            page_index,
            origin,
            current,
        } => {
            finish_group_move(viewer, page_index, origin, current);
            true
        }
    }
}

fn is_click(origin: (f64, f64), current: (f64, f64)) -> bool {
    (current.0 - origin.0).abs() < CLICK_PT && (current.1 - origin.1).abs() < CLICK_PT
}

fn finish_marquee(
    viewer: &Viewer,
    page_index: usize,
    origin: (f64, f64),
    current: (f64, f64),
) -> bool {
    if is_click(origin, current) {
        selection::redraw(viewer);
        return false;
    }
    let area = marquee_rect(origin, current);
    let members =
        with_content(viewer, page_index, |content| members_in(content, area)).unwrap_or_default();
    {
        let mut state = viewer.state.borrow_mut();
        if let Some(session) = state.session.as_mut() {
            session.selected_image = None;
            session.selected_path = None;
            session.content_selection = (!members.is_empty()).then(|| ContentSelection {
                page_index,
                members: members.clone(),
            });
        }
    }
    viewer.status.set_text(&if members.is_empty() {
        "Nothing there to select.".to_string()
    } else {
        selected_message(&members)
    });
    selection_changed(viewer);
    true
}

fn finish_group_move(viewer: &Viewer, page_index: usize, origin: (f64, f64), current: (f64, f64)) {
    if is_click(origin, current) {
        selection::redraw(viewer);
        return;
    }
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        selection::redraw(viewer);
        return;
    }
    let (dx, dy) = (current.0 - origin.0, current.1 - origin.1);
    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(members) = session
            .content_selection
            .as_ref()
            .filter(|held| held.page_index == page_index)
            .map(|held| held.members.clone())
        else {
            return;
        };
        let result = command::record_selection_move(session, &members, dx, dy);
        if result.is_ok() {
            // The overlay reads every member back at its new box under the
            // same id, so the selection follows them there, and a second
            // drag folds into this batch.
            session.content_selection = Some(ContentSelection {
                page_index,
                members: translated(&members, dx, dy),
            });
        }
        result
    };
    match result {
        Ok(()) => refresh_preview(viewer, "Selection moved."),
        Err(error) => viewer.status.set_text(&error),
    }
    selection_changed(viewer);
}

/// Deletes every selected item as one undo step. Returns whether there was a
/// selection, so the Delete key can fall through to what else it deletes.
pub(crate) fn delete_selected(viewer: &Viewer) -> bool {
    if !has_selection(viewer) {
        return false;
    }
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return true;
    }
    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return true;
        };
        let Some(held) = session.content_selection.take() else {
            return true;
        };
        let result = command::record_selection_removal(session, &held.members);
        if result.is_err() {
            session.content_selection = Some(held);
        }
        result
    };
    match result {
        Ok(()) => refresh_preview(viewer, "Selection deleted."),
        Err(error) => viewer.status.set_text(&error),
    }
    selection_changed(viewer);
    true
}

/// Drops the selection and any marquee or group drag in flight.
pub(crate) fn clear(viewer: &Viewer) {
    let cleared = {
        let mut state = viewer.state.borrow_mut();
        state.session.as_mut().is_some_and(|session| {
            session.selection_gesture = None;
            session.content_selection.take().is_some()
        })
    };
    if cleared {
        selection_changed(viewer);
    }
}

/// Whether several-item selection holds anything right now.
pub(crate) fn has_selection(viewer: &Viewer) -> bool {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| session.content_selection.is_some())
}

/// Esc clears the selection. The action is enabled only while there is one,
/// and a disabled action does not consume its accelerator, so Esc still
/// reaches an open editor or dialog the rest of the time.
pub(crate) fn connect_clear_shortcut(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    let action = gio::SimpleAction::new(CLEAR_ACTION, None);
    action.set_enabled(false);
    action.connect_activate({
        let viewer = viewer.clone();
        move |_, _| {
            clear(&viewer);
            viewer.status.set_text("Selection cleared.");
        }
    });
    window.add_action(&action);
    application.set_accels_for_action(&format!("win.{CLEAR_ACTION}"), &["Escape"]);
}

fn selection_changed(viewer: &Viewer) {
    let selected = has_selection(viewer);
    if let Some(action) = viewer
        .window()
        .and_then(|window| window.lookup_action(CLEAR_ACTION))
        .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
    {
        action.set_enabled(selected);
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{ContentItemId, FontKind, PageId, PathPaint};

    fn a_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn a_run(id: u64, bbox: Rect) -> TextRun {
        TextRun {
            id: ContentItemId(id),
            page: PageId(0),
            bbox,
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "word".to_string(),
        }
    }

    fn a_path(id: u64, bbox: Rect) -> PathItem {
        PathItem {
            id: ContentItemId(id),
            page: PageId(0),
            bbox,
            paint: PathPaint::Stroke,
        }
    }

    #[test]
    fn a_hairline_rule_touching_the_marquee_counts() {
        let rule = a_rect(100.0, 300.0, 200.0, 0.0);
        assert!(touches(rule, a_rect(150.0, 300.0, 10.0, 10.0)));
        assert!(!touches(rule, a_rect(150.0, 300.5, 10.0, 10.0)));
    }

    #[test]
    fn a_marquee_dragged_up_and_left_covers_the_same_box() {
        assert_eq!(
            marquee_rect((200.0, 100.0), (50.0, 300.0)),
            a_rect(50.0, 100.0, 150.0, 200.0)
        );
    }

    #[test]
    fn a_marquee_gathers_paths_and_text_it_touches_and_nothing_else() {
        let content = PageContent {
            text_runs: vec![
                a_run(1, a_rect(100.0, 500.0, 50.0, 12.0)),
                a_run(2, a_rect(100.0, 100.0, 50.0, 12.0)),
            ],
            images: Vec::new(),
            paths: vec![a_path(3, a_rect(90.0, 490.0, 100.0, 30.0))],
        };

        let selection = members_in(&content, a_rect(80.0, 480.0, 40.0, 40.0));

        assert_eq!(
            selection.runs.iter().map(|run| run.id).collect::<Vec<_>>(),
            [ContentItemId(1)]
        );
        assert_eq!(selection.paths.len(), 1);
    }

    #[test]
    fn a_shift_click_adds_an_item_and_a_second_one_takes_it_out() {
        let rule = a_path(3, a_rect(90.0, 490.0, 100.0, 0.0));
        let once = toggled(Selection::default(), Member::Path(rule.clone()));
        assert_eq!(once.paths, std::slice::from_ref(&rule));
        assert!(toggled(once, Member::Path(rule)).is_empty());
    }

    #[test]
    fn a_block_toggles_as_a_whole() {
        let runs = vec![
            a_run(1, a_rect(100.0, 500.0, 30.0, 12.0)),
            a_run(2, a_rect(132.0, 500.0, 30.0, 12.0)),
        ];
        let added = toggled(Selection::default(), Member::Runs(runs.clone()));
        assert_eq!(added.runs.len(), 2);
        assert!(toggled(added, Member::Runs(runs)).is_empty());
    }

    #[test]
    fn the_union_box_covers_every_member() {
        let selection = Selection {
            runs: vec![a_run(1, a_rect(100.0, 500.0, 50.0, 12.0))],
            images: Vec::new(),
            paths: vec![a_path(3, a_rect(90.0, 300.0, 300.0, 0.0))],
        };
        assert_eq!(union(&selection), Some(a_rect(90.0, 300.0, 300.0, 212.0)));
    }

    #[test]
    fn a_moved_selection_keeps_its_ids_at_the_new_boxes() {
        let selection = Selection {
            runs: vec![a_run(1, a_rect(100.0, 500.0, 50.0, 12.0))],
            images: Vec::new(),
            paths: vec![a_path(3, a_rect(90.0, 300.0, 300.0, 0.0))],
        };
        let moved = translated(&selection, 10.0, -20.0);
        assert_eq!(moved.runs[0].id, ContentItemId(1));
        assert_eq!(moved.runs[0].bbox, a_rect(110.0, 480.0, 50.0, 12.0));
        assert_eq!(moved.paths[0].bbox, a_rect(100.0, 280.0, 300.0, 0.0));
    }

    /// A table — its frame and the text inside — dragged twice before a
    /// save is one batch, recorded through the core.
    #[test]
    fn a_selection_moved_twice_is_one_queued_batch() {
        use pdf_document::Command;

        let base = gen_fixtures::build_multi_line_page_document(&["Hello", "world"]);
        let content = pdf_edit::read_page_content(&base, PageId(0)).expect("the page parses");
        let mut session = crate::app::test_fixtures::backed_session(base);
        let members = Selection {
            runs: content.text_runs.clone(),
            ..Selection::default()
        };

        command::record_batch(
            &mut session,
            pdf_edit::move_selection(&members, 20.0, -10.0),
        )
        .expect("the first move");
        let moved = translated(&members, 20.0, -10.0);
        command::record_batch(&mut session, pdf_edit::move_selection(&moved, 5.0, 5.0))
            .expect("the second move folds");

        let entries = session
            .document_model
            .as_ref()
            .expect("a model")
            .pending_edits
            .entries();
        assert!(
            matches!(entries, [Command::Batch(children)] if children.len() == members.runs.len())
        );
    }
}
