//! Select, move, reshape and delete a vector path in content-edit mode
//! (B25 T-206): the table borders, rules and shaded boxes a page paints.
//! The path twin of [`super::image`], following Windows' `PathGestures`
//! and macOS's `ViewerStore+ContentPaths`.
//!
//! Text outranks a path: a frame encloses its cells, so a press inside one
//! is asked of the text first (`super::begin_drag`). An outline is hit only
//! near its edges for the same reason, and the smallest hit wins — a rule
//! inside a frame inside a shaded panel is the one a reader aiming at it
//! means.
//!
//! Unlike an image, a path edited again before a save is not refused: the
//! core folds a second geometry edit into the queued one
//! (`pdf_save::record_content_command`), and the pending overlay reads the
//! path back at its new box, so the selection follows it there.
//!
//! As in `image`, the `&Viewer` functions stay thin and untested; what
//! decides what a press or a release means is pure ([`path_hit`],
//! [`path_at`], [`dragged_rect`], [`command_for`]) and carries the tests.

use pdf_document::{Command, PathItem, PathPaint, Rect};

use crate::app::selection;
use crate::app::state::{AnnotationDragMode, PathDrag, SelectedPath, Viewer};
use crate::app::update_content_edit_controls;
use crate::app::write::refresh_preview;

use super::{command, editor, geometry, model};

pub(crate) const PATH_SELECTED: &str =
    "Shape selected — drag it, drag a corner to resize it, or press Delete.";

/// Whether a press at `point` means `path`: anywhere on a filled shape, but
/// only within `reach` of an outline's edges — further inside a frame is its
/// cell, not its border. The box is padded by `reach` first, so a hairline
/// rule (a box of zero height) can be grabbed at all.
pub(crate) fn path_hit(path: &PathItem, point: (f64, f64), reach: f64) -> bool {
    let bbox = path.bbox;
    let padded = Rect {
        x: bbox.x - reach,
        y: bbox.y - reach,
        width: bbox.width + 2.0 * reach,
        height: bbox.height + 2.0 * reach,
    };
    if !geometry::contains(padded, point) {
        return false;
    }
    if path.paint != PathPaint::Stroke {
        return true;
    }
    let inner = Rect {
        x: bbox.x + reach,
        y: bbox.y + reach,
        width: bbox.width - 2.0 * reach,
        height: bbox.height - 2.0 * reach,
    };
    inner.width <= 0.0 || inner.height <= 0.0 || !geometry::contains(inner, point)
}

/// The path a press at `point` meant: the selected one while the press is on
/// it or on one of its corners, so a handle that overhangs a neighbour still
/// reshapes it; otherwise the smallest path hit.
pub(crate) fn path_at<'a>(
    paths: &'a [PathItem],
    point: (f64, f64),
    selected: Option<&PathItem>,
    reach: f64,
) -> Option<&'a PathItem> {
    if let Some(selected) = selected {
        if geometry::corner_at(selected.bbox, point, reach).is_some()
            || path_hit(selected, point, reach)
        {
            if let Some(found) = paths.iter().find(|path| path.id == selected.id) {
                return Some(found);
            }
        }
    }
    paths
        .iter()
        .filter(|path| path_hit(path, point, reach))
        .min_by(|a, b| area(&a.bbox).total_cmp(&area(&b.bbox)))
}

fn area(rect: &Rect) -> f64 {
    rect.width * rect.height
}

/// Where the path would be if the pointer came up now. An axis the path is
/// flat along — a rule's height — stays flat and in place when a corner is
/// dragged: the core cannot stretch nothing.
pub(crate) fn dragged_rect(drag: &PathDrag) -> Rect {
    let bbox = drag.item.bbox;
    match drag.mode {
        AnnotationDragMode::Move => Rect {
            x: bbox.x + drag.current.0 - drag.origin.0,
            y: bbox.y + drag.current.1 - drag.origin.1,
            ..bbox
        },
        AnnotationDragMode::Resize(corner) => {
            let resized = geometry::resized_rect(bbox, corner, drag.current);
            let (x, width) = if bbox.width == 0.0 {
                (bbox.x, 0.0)
            } else {
                (resized.x, resized.width)
            };
            let (y, height) = if bbox.height == 0.0 {
                (bbox.y, 0.0)
            } else {
                (resized.y, resized.height)
            };
            Rect {
                x,
                y,
                width,
                height,
            }
        }
    }
}

/// The command a finished drag records, or `None` for a release that left
/// the path where it was — that press only selected it.
pub(crate) fn command_for(drag: &PathDrag) -> Option<Command> {
    let to = dragged_rect(drag);
    if to == drag.item.bbox {
        return None;
    }
    let item = drag.item.clone();
    Some(match drag.mode {
        AnnotationDragMode::Move => Command::MovePath { item, to },
        AnnotationDragMode::Resize(_) => Command::ResizePath { item, to },
    })
}

fn message_for(mode: AnnotationDragMode) -> &'static str {
    match mode {
        AnnotationDragMode::Move => "Shape moved.",
        AnnotationDragMode::Resize(_) => "Shape resized.",
    }
}

/// Grabs the path under `point` on `page_index`, selecting it; a press on
/// the selected path's corner reshapes it. A miss drops the selection and
/// leaves the press unclaimed.
pub(crate) fn begin_path_drag(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> bool {
    if viewer.content_edit_refusal().is_some() {
        return false;
    }
    // An armed insert claims every click for the new content.
    if viewer.state.borrow().content_insert_mode.is_some() {
        return false;
    }
    // The same preamble as `text::begin_text_drag`: whatever editor is open
    // resolves first, and one whose retype was refused keeps the cursor.
    editor::commit(viewer);
    if editor::keep_refused_editor(viewer) {
        return false;
    }
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    let Some(base) = session.save_backing.as_ref().map(|backing| &backing.base) else {
        return false;
    };
    let Some(document) = session.document_model.as_ref() else {
        return false;
    };
    let pending = Some(&document.pending_edits);
    let Some(page_id) = super::content_page(session, page_index) else {
        return false;
    };
    let Some(probe) = super::page_probe(document, base, &session.imported_sources, page_id) else {
        return false;
    };
    let selected = session
        .selected_path
        .as_ref()
        .filter(|selected| selected.page_index == page_index)
        .map(|selected| selected.item.clone());
    let Some(page) = session.pages.get_mut(page_index) else {
        return false;
    };
    let hit = model::ensure_page_content(&mut page.content, probe, page_id, pending)
        .ok()
        .and_then(|content| path_at(&content.paths, point, selected.as_ref(), reach).cloned());

    let Some(item) = hit else {
        let deselected = session.selected_path.take().is_some();
        drop(state);
        if deselected {
            update_content_edit_controls(viewer);
            selection::redraw(viewer);
        }
        return false;
    };
    let mode = match geometry::corner_at(item.bbox, point, reach) {
        Some(corner) if selected.as_ref().is_some_and(|path| path.id == item.id) => {
            AnnotationDragMode::Resize(corner)
        }
        _ => AnnotationDragMode::Move,
    };
    session.selected_image = None;
    session.image_drag = None;
    session.selected_path = Some(SelectedPath {
        page_index,
        item: item.clone(),
    });
    session.path_drag = Some(PathDrag {
        page_index,
        item,
        mode,
        origin: point,
        current: point,
    });
    drop(state);
    viewer.status.set_text(PATH_SELECTED);
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
    true
}

/// Extends the path drag in flight. Returns whether there was one.
pub(crate) fn extend_path_drag(viewer: &Viewer, point: (f64, f64)) -> bool {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(drag) = state
            .session
            .as_mut()
            .and_then(|session| session.path_drag.as_mut())
        else {
            return false;
        };
        drag.current = point;
    }
    selection::redraw(viewer);
    true
}

/// Records the path drag in flight, if any. A release that left the path
/// where it was only selected it. Returns whether a drag existed.
pub(crate) fn finish_path_drag(viewer: &Viewer) -> bool {
    let Some(drag) = viewer
        .state
        .borrow_mut()
        .session
        .as_mut()
        .and_then(|session| session.path_drag.take())
    else {
        return false;
    };
    let Some(edit) = command_for(&drag) else {
        selection::redraw(viewer);
        return true;
    };
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        selection::redraw(viewer);
        return true;
    }
    let to = dragged_rect(&drag);
    let result = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return true;
        };
        let result = command::record(session, edit);
        if result.is_ok() {
            // The pending overlay reads the path back at `to` under the same
            // id, so the selection follows it there rather than staying on
            // the box it left.
            if let Some(selected) = session.selected_path.as_mut() {
                selected.item.bbox = to;
            }
        }
        result
    };
    match result {
        Ok(()) => refresh_preview(viewer, message_for(drag.mode)),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
    true
}

/// Deletes the selected path. Returns whether one was selected, so the
/// Delete key can fall through to whatever else it deletes.
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
        let Some(selected) = session.selected_path.take() else {
            return true;
        };
        let result = command::record(session, Command::RemovePath(selected.item.clone()));
        if result.is_err() {
            session.selected_path = Some(selected);
        }
        result
    };
    match result {
        Ok(()) => refresh_preview(viewer, "Shape deleted."),
        Err(error) => viewer.status.set_text(&error),
    }
    update_content_edit_controls(viewer);
    selection::redraw(viewer);
    true
}

/// Whether a path is selected right now.
pub(crate) fn has_selection(viewer: &Viewer) -> bool {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| session.selected_path.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Corner;
    use pdf_document::{ContentItemId, PageId};

    fn a_path(id: u64, bbox: (f64, f64, f64, f64), paint: PathPaint) -> PathItem {
        PathItem {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: Rect {
                x: bbox.0,
                y: bbox.1,
                width: bbox.2,
                height: bbox.3,
            },
            paint,
        }
    }

    fn a_drag(item: PathItem, mode: AnnotationDragMode, current: (f64, f64)) -> PathDrag {
        PathDrag {
            page_index: 0,
            item,
            mode,
            origin: (0.0, 0.0),
            current,
        }
    }

    #[test]
    fn a_filled_shape_is_hit_anywhere_inside_it() {
        let panel = a_path(1, (100.0, 100.0, 200.0, 100.0), PathPaint::Fill);
        assert!(path_hit(&panel, (200.0, 150.0), 3.0));
    }

    #[test]
    fn an_outline_is_hit_near_its_edges_but_not_in_its_middle() {
        let frame = a_path(1, (100.0, 100.0, 200.0, 100.0), PathPaint::Stroke);
        assert!(path_hit(&frame, (101.0, 150.0), 3.0));
        assert!(!path_hit(&frame, (200.0, 150.0), 3.0));
    }

    #[test]
    fn a_hairline_rule_is_hit_within_reach_of_its_line() {
        let rule = a_path(1, (100.0, 300.0, 200.0, 0.0), PathPaint::Stroke);
        assert!(path_hit(&rule, (150.0, 302.0), 3.0));
        assert!(!path_hit(&rule, (150.0, 310.0), 3.0));
    }

    #[test]
    fn the_smallest_path_under_the_press_wins() {
        let panel = a_path(1, (0.0, 0.0, 500.0, 500.0), PathPaint::Fill);
        let swatch = a_path(2, (90.0, 90.0, 20.0, 20.0), PathPaint::Fill);
        let paths = [panel, swatch];
        assert_eq!(
            path_at(&paths, (100.0, 100.0), None, 3.0).map(|path| path.id),
            Some(ContentItemId(2))
        );
    }

    #[test]
    fn the_selected_path_keeps_a_press_on_its_corner() {
        let frame = a_path(1, (100.0, 100.0, 200.0, 100.0), PathPaint::Stroke);
        let swatch = a_path(2, (95.0, 95.0, 8.0, 8.0), PathPaint::Fill);
        let paths = [frame.clone(), swatch];
        assert_eq!(
            path_at(&paths, (100.0, 100.0), Some(&frame), 3.0).map(|path| path.id),
            Some(ContentItemId(1))
        );
    }

    #[test]
    fn a_move_keeps_the_size_and_follows_the_pointer() {
        let frame = a_path(1, (100.0, 100.0, 200.0, 100.0), PathPaint::Stroke);
        let drag = a_drag(frame, AnnotationDragMode::Move, (10.0, -5.0));
        assert_eq!(
            dragged_rect(&drag),
            Rect {
                x: 110.0,
                y: 95.0,
                width: 200.0,
                height: 100.0
            }
        );
    }

    #[test]
    fn a_corner_drag_on_a_rule_keeps_it_flat() {
        let rule = a_path(1, (100.0, 300.0, 200.0, 0.0), PathPaint::Stroke);
        let drag = a_drag(
            rule,
            AnnotationDragMode::Resize(Corner::TopRight),
            (400.0, 350.0),
        );
        assert_eq!(
            dragged_rect(&drag),
            Rect {
                x: 100.0,
                y: 300.0,
                width: 300.0,
                height: 0.0
            }
        );
    }

    #[test]
    fn a_release_where_the_press_landed_records_nothing() {
        let frame = a_path(1, (100.0, 100.0, 200.0, 100.0), PathPaint::Stroke);
        let drag = a_drag(frame, AnnotationDragMode::Move, (0.0, 0.0));
        assert_eq!(command_for(&drag), None);
    }

    /// A frame, a hairline rule under it, and a clip the parser leaves out —
    /// the table page `pdf-ffi`'s `content_paths` tests use.
    fn table_page() -> lopdf::Document {
        use lopdf::{dictionary, Object, Stream};
        let mut document = lopdf::Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let content = b"0.5 w 319 135 251 125 re S\n319 120 m 570 120 l S\nq 0 0 50 50 re W n Q";
        let content_id = document.add_object(Stream::new(dictionary! {}, content.to_vec()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document
    }

    fn table_session() -> (crate::app::state::DocumentSession, Vec<PathItem>) {
        use crate::app::state::SaveBacking;
        use crate::app::test_fixtures::model_session;
        use pdf_document::{Document, Orientation, Page, PageSize, Rotation};

        let base = table_page();
        let paths = pdf_edit::read_page_content(&base, PageId(0))
            .expect("the table page parses")
            .paths;
        let mut session = model_session(Document::with_pages(vec![Page::base(
            PageId(0),
            0,
            PageSize::A4,
            Orientation::Portrait,
            Rotation::None,
        )]));
        session.save_backing = Some(SaveBacking {
            base: pdf_manip::LopdfDocument::from_lopdf(base).into(),
            original_bytes: Vec::new().into(),
            password: None,
        });
        (session, paths)
    }

    fn entries(session: &crate::app::state::DocumentSession) -> &[Command] {
        session
            .document_model
            .as_ref()
            .expect("a model")
            .pending_edits
            .entries()
    }

    /// The fold that lets a shape be dragged again before a save lives in
    /// the core (`pdf_save::record`); this shell must reach it, not refuse
    /// the second drag the way it still refuses a second image edit.
    #[test]
    fn a_path_moved_twice_is_one_queued_move_from_its_original_box() {
        let (mut session, paths) = table_session();
        let frame = paths
            .iter()
            .find(|path| path.bbox.height > 0.0)
            .expect("the frame")
            .clone();
        let first = Rect {
            y: frame.bbox.y + 100.0,
            ..frame.bbox
        };
        command::record(
            &mut session,
            Command::MovePath {
                item: frame.clone(),
                to: first,
            },
        )
        .expect("the first move records");

        // What the overlay hands back now: the same path at its new box.
        let moved = PathItem {
            bbox: first,
            ..frame.clone()
        };
        let second = Rect {
            x: first.x - 50.0,
            ..first
        };
        command::record(
            &mut session,
            Command::MovePath {
                item: moved,
                to: second,
            },
        )
        .expect("the second move folds");

        assert_eq!(
            entries(&session),
            [Command::MovePath {
                item: frame,
                to: second
            }]
        );
        assert_eq!(session.edit_revision, 2);
    }

    #[test]
    fn a_moved_path_deleted_before_a_save_is_one_queued_removal() {
        let (mut session, paths) = table_session();
        let rule = paths
            .iter()
            .find(|path| path.bbox.height == 0.0)
            .expect("the rule")
            .clone();
        let to = Rect {
            y: rule.bbox.y - 40.0,
            ..rule.bbox
        };
        command::record(
            &mut session,
            Command::MovePath {
                item: rule.clone(),
                to,
            },
        )
        .expect("the move records");
        command::record(
            &mut session,
            Command::RemovePath(PathItem {
                bbox: to,
                ..rule.clone()
            }),
        )
        .expect("the removal folds");

        assert_eq!(entries(&session), [Command::RemovePath(rule)]);
    }

    #[test]
    fn a_corner_drag_records_a_resize_and_a_body_drag_a_move() {
        let frame = a_path(1, (100.0, 100.0, 200.0, 100.0), PathPaint::Stroke);
        let moved = a_drag(frame.clone(), AnnotationDragMode::Move, (5.0, 5.0));
        assert!(matches!(
            command_for(&moved),
            Some(Command::MovePath { .. })
        ));
        let resized = a_drag(
            frame,
            AnnotationDragMode::Resize(Corner::TopRight),
            (350.0, 250.0),
        );
        assert!(matches!(
            command_for(&resized),
            Some(Command::ResizePath { .. })
        ));
    }
}
