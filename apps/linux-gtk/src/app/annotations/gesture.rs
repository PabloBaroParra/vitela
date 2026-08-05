//! The press/move/release lifecycles: placing a new annotation with an armed
//! tool, and dragging an existing one by its body or a corner handle.
//!
//! `selection` owns the per-page gesture and calls in here first; a `false`
//! return means the drag was not claimed and text selection may have it.

use pdf_document::{AnnotationId, Command, PageId};

use crate::app::selection;
use crate::app::state::{AnnotationDrag, AnnotationDragMode, Placement, Viewer};

use super::builder::annotation_at;
use super::command::{apply_command, command, model};
use super::edit::supports_resize;
use super::geometry::{bounds, committed_rect, contains, corner_at, dragged};
use super::toolbar::{disarm, update_annotation_controls};
use super::SELECTION_GONE;

const DRAG_UNSUPPORTED: &str = "That annotation cannot be changed this way.";

/// Starts drawing an annotation, if a tool is armed. Returns whether the drag
/// was claimed — a `false` leaves it to text selection.
pub(crate) fn begin_placement(viewer: &Viewer, page_index: usize, point: (f64, f64)) -> bool {
    if viewer.annotation_editing_refusal().is_some() {
        return false;
    }
    let mut state = viewer.state.borrow_mut();
    let Some(tool) = state.active_tool else {
        return false;
    };
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    session.placement = Some(Placement {
        tool,
        page_index,
        origin: point,
        current: point,
        points: if tool.is_freehand() {
            vec![point]
        } else {
            Vec::new()
        },
    });
    true
}

/// Extends the placement in flight. Returns whether there was one.
pub(crate) fn extend_placement(viewer: &Viewer, point: (f64, f64)) -> bool {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(placement) = state
            .session
            .as_mut()
            .and_then(|session| session.placement.as_mut())
        else {
            return false;
        };
        placement.current = point;
        if placement.tool.is_freehand() {
            placement.points.push(point);
        }
    }
    selection::redraw(viewer);
    true
}

/// Commits the placement in flight, if any, and disarms the tool.
///
/// Disarming after one annotation is deliberate: a tool that stays armed turns
/// the next stray drag into another annotation, and this shell has no undo yet
/// (T-048) to take it back.
pub(crate) fn finish_placement(viewer: &Viewer) {
    let placement = {
        let mut state = viewer.state.borrow_mut();
        match state
            .session
            .as_mut()
            .and_then(|session| session.placement.take())
        {
            Some(placement) => placement,
            None => return,
        }
    };
    disarm(viewer);
    command(viewer, move |session| {
        let id = AnnotationId(session.next_annotation_id);
        let page = PageId(placement.page_index as u32);
        let annotation = annotation_at(&placement, id, page, committed_rect(&placement))?;
        {
            let document = model(session)?;
            apply_command(document, Command::AddAnnotation(annotation));
        }
        session.next_annotation_id += 1;
        session.selected_annotation = Some(id);
        Ok(format!(
            "{} added. Changes are pending save.",
            placement.tool.label()
        ))
    });
}

/// Grabs an annotation under the pointer: a corner handle of the selected one
/// resizes it, its body moves it, and any other annotation becomes the
/// selection. Returns whether the drag was claimed.
///
/// Tried after an armed tool and before text selection. Direct manipulation is
/// what the toolbar's fixed-step buttons cannot give: they nudge by a constant
/// the user never chose, which is fine as a fine adjustment and useless as the
/// only way to place something.
pub(crate) fn begin_annotation_drag(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> bool {
    if viewer.annotation_editing_refusal().is_some() {
        return false;
    }
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    let Some(document) = session.document_model.as_ref() else {
        return false;
    };

    // The selected annotation gets first refusal on the press, so its handles
    // stay reachable even where another annotation overlaps them.
    let selected = session
        .selected_annotation
        .and_then(|id| document.annotations.get(id))
        .filter(|annotation| annotation.page.0 as usize == page_index);
    if let Some(annotation) = selected {
        if let Some(rect) = bounds(annotation) {
            let mode = match corner_at(rect, point, reach) {
                // Ink cannot be resized, so it offers no corners to pull.
                Some(corner) if supports_resize(annotation) => {
                    Some(AnnotationDragMode::Resize(corner))
                }
                _ if contains(rect, point) => Some(AnnotationDragMode::Move),
                _ => None,
            };
            if let Some(mode) = mode {
                session.annotation_drag = Some(AnnotationDrag {
                    id: annotation.id,
                    mode,
                    origin: point,
                    current: point,
                });
                return true;
            }
        }
    }

    // Topmost first: the set paints in order, so the last one drawn is the one
    // the user sees on top and means to click.
    let hit = document
        .annotations
        .iter()
        .filter(|annotation| annotation.page.0 as usize == page_index)
        .filter(|annotation| bounds(annotation).is_some_and(|rect| contains(rect, point)))
        .last()
        .map(|annotation| annotation.id);
    match hit {
        Some(id) => {
            session.selected_annotation = Some(id);
            session.annotation_drag = Some(AnnotationDrag {
                id,
                mode: AnnotationDragMode::Move,
                origin: point,
                current: point,
            });
            drop(state);
            update_annotation_controls(viewer);
            selection::redraw(viewer);
            true
        }
        None => {
            // A press on bare page is a deselection: the user is pointing at
            // something that is not the annotation. The drag itself is not
            // claimed — it goes on to select text as usual.
            let deselected = session.selected_annotation.take().is_some();
            drop(state);
            if deselected {
                update_annotation_controls(viewer);
                selection::redraw(viewer);
            }
            false
        }
    }
}

/// Extends the annotation drag in flight. Returns whether there was one.
pub(crate) fn extend_annotation_drag(viewer: &Viewer, point: (f64, f64)) -> bool {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(drag) = state
            .session
            .as_mut()
            .and_then(|session| session.annotation_drag.as_mut())
        else {
            return false;
        };
        drag.current = point;
    }
    selection::redraw(viewer);
    true
}

/// Commits the annotation drag in flight, if any.
///
/// A drag that never moved is just a click that selected something, so it
/// records nothing: an `EditLog` full of no-op edits would make undo useless.
pub(crate) fn finish_annotation_drag(viewer: &Viewer) {
    let drag = {
        let mut state = viewer.state.borrow_mut();
        match state
            .session
            .as_mut()
            .and_then(|session| session.annotation_drag.take())
        {
            Some(drag) => drag,
            None => return,
        }
    };
    if drag.origin == drag.current {
        selection::redraw(viewer);
        return;
    }
    command(viewer, move |session| {
        let document = model(session)?;
        let before = document
            .annotations
            .get(drag.id)
            .cloned()
            .ok_or_else(|| SELECTION_GONE.to_string())?;
        let after = dragged(&before, &drag).ok_or_else(|| DRAG_UNSUPPORTED.to_string())?;
        apply_command(document, Command::ReplaceAnnotation { before, after });
        Ok(match drag.mode {
            AnnotationDragMode::Move => "Annotation moved. Changes are pending save.".to_string(),
            AnnotationDragMode::Resize(_) => {
                "Annotation resized. Changes are pending save.".to_string()
            }
        })
    });
}
