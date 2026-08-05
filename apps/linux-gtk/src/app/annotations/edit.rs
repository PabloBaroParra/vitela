//! Operations on the annotation that is already selected: the toolbar's
//! fixed-step Nudge and Grow, Delete, and stepping the selection itself.

use pdf_document::{Annotation, AnnotationId, AnnotationKind, Command, Rect};

use crate::app::selection;
use crate::app::state::Viewer;

use super::command::{apply_command, command, model};
use super::toolbar::update_annotation_controls;
use super::SELECTION_GONE;

/// Fallback rect for the one operation that can still need one without a
/// pointer behind it — see [`edit_resize`].
const DEFAULT_RECT: Rect = Rect {
    x: 72.0,
    y: 72.0,
    width: 144.0,
    height: 36.0,
};
/// How far the Nudge button shifts the selected annotation, in PDF points.
const NUDGE_PT: f64 = 12.0;
/// How much the Grow button enlarges the selected annotation.
const RESIZE_FACTOR: f64 = 1.25;

const SELECT_FIRST: &str = "Select an annotation first.";

fn edit(
    viewer: &Viewer,
    operation: impl FnOnce(&mut Annotation) -> Result<(), pdf_annotate::AnnotateError>,
) {
    command(viewer, |session| {
        let id = session
            .selected_annotation
            .ok_or_else(|| SELECT_FIRST.to_string())?;
        let document = model(session)?;
        let before = document
            .annotations
            .get(id)
            .cloned()
            .ok_or_else(|| SELECTION_GONE.to_string())?;
        let mut after = before.clone();
        operation(&mut after).map_err(|error| error.to_string())?;
        apply_command(document, Command::ReplaceAnnotation { before, after });
        Ok("Annotation edited. Changes are pending save.".to_string())
    });
}

pub(super) fn edit_move(viewer: &Viewer) {
    edit(viewer, |annotation| {
        pdf_annotate::move_annotation(annotation, NUDGE_PT, NUDGE_PT)
    });
}

pub(super) fn edit_resize(viewer: &Viewer) {
    edit(viewer, |annotation| {
        let rect = resize_rect(annotation).unwrap_or(DEFAULT_RECT);
        pdf_annotate::resize_annotation(annotation, rect)
    });
}

pub(super) fn delete(viewer: &Viewer) {
    command(viewer, |session| {
        let id = session
            .selected_annotation
            .ok_or_else(|| SELECT_FIRST.to_string())?;
        {
            let document = model(session)?;
            let annotation = document
                .annotations
                .get(id)
                .cloned()
                .ok_or_else(|| SELECTION_GONE.to_string())?;
            apply_command(document, Command::RemoveAnnotation(annotation));
        }
        session.selected_annotation = None;
        Ok("Annotation deleted. Changes are pending save.".to_string())
    });
}

/// Steps the selection one annotation backwards through the set, wrapping.
///
/// Not routed through `command`: changing which annotation is selected is
/// not an edit, records nothing in the `EditLog`, and so is not something the
/// document's annotate permission has any say over.
pub(super) fn select_previous(viewer: &Viewer) {
    let selected = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(document) = session.document_model.as_ref() else {
            return;
        };
        let ids: Vec<_> = document
            .annotations
            .iter()
            .map(|annotation| annotation.id)
            .collect();
        let Some(selected) = session
            .selected_annotation
            .and_then(|id| previous_annotation_id(&ids, id))
        else {
            return;
        };
        session.selected_annotation = Some(selected);
        selected
    };
    viewer
        .status
        .set_text(&format!("Selected annotation {}.", selected.0));
    update_annotation_controls(viewer);
    selection::redraw(viewer);
}

fn previous_annotation_id(ids: &[AnnotationId], selected: AnnotationId) -> Option<AnnotationId> {
    let index = ids.iter().position(|candidate| *candidate == selected)?;
    Some(ids[(index + ids.len() - 1) % ids.len()])
}

/// The grown rect for a resize, or `None` for a kind that has no rect to grow
/// (`Ink` is a polyline — `pdf-annotate` rejects resizing it).
fn resize_rect(annotation: &Annotation) -> Option<Rect> {
    let rect = match &annotation.kind {
        AnnotationKind::Highlight { rect, .. }
        | AnnotationKind::Underline { rect, .. }
        | AnnotationKind::Strikeout { rect, .. }
        | AnnotationKind::Shape { rect, .. }
        | AnnotationKind::TextNote { rect, .. }
        | AnnotationKind::Stamp { rect, .. } => rect,
        _ => return None,
    };
    Some(Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width * RESIZE_FACTOR,
        height: rect.height * RESIZE_FACTOR,
    })
}

pub(super) fn supports_resize(annotation: &Annotation) -> bool {
    resize_rect(annotation).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::annotations::style::supports_restyle;
    use crate::app::annotations::test_support::{built, drag};
    use crate::app::state::Tool;

    #[test]
    fn an_edit_is_reversible_through_its_inverse() {
        let before = built(&drag(Tool::Highlight, (100.0, 100.0), (200.0, 160.0)));
        let mut after = before.clone();
        pdf_annotate::move_annotation(&mut after, NUDGE_PT, NUDGE_PT).expect("move");

        let inverse = Command::ReplaceAnnotation {
            before: before.clone(),
            after: after.clone(),
        }
        .inverse();

        // The inverse must restore the *original* value, not merely be another
        // ReplaceAnnotation: an inverse that forgot to swap would still match
        // the variant.
        assert_eq!(
            inverse,
            Command::ReplaceAnnotation {
                before: after,
                after: before,
            }
        );
    }

    #[test]
    fn resize_remains_rejected_for_ink() {
        let mut annotation = built(&drag(Tool::Ink, (10.0, 10.0), (100.0, 100.0)));

        assert!(pdf_annotate::resize_annotation(&mut annotation, DEFAULT_RECT).is_err());
    }

    #[test]
    fn resize_grows_the_annotation_without_moving_its_origin() {
        let mut annotation = built(&drag(Tool::Shape, (100.0, 100.0), (200.0, 160.0)));
        pdf_annotate::move_annotation(&mut annotation, 20.0, 30.0).expect("move");

        let rect = resize_rect(&annotation).expect("shape has a rect");

        assert_eq!((rect.x, rect.y), (120.0, 130.0));
        assert_eq!(
            (rect.width, rect.height),
            (100.0 * RESIZE_FACTOR, 60.0 * RESIZE_FACTOR)
        );
    }

    #[test]
    fn only_supported_operations_are_enabled_for_each_annotation_kind() {
        let ink = built(&drag(Tool::Ink, (10.0, 10.0), (100.0, 100.0)));
        let note = built(&drag(Tool::TextNote, (10.0, 10.0), (100.0, 100.0)));

        assert!(!supports_resize(&ink));
        assert!(supports_restyle(&ink));
        assert!(supports_resize(&note));
        assert!(!supports_restyle(&note));
    }

    #[test]
    fn previous_annotation_selection_wraps_to_an_earlier_annotation() {
        let ids = [AnnotationId(1), AnnotationId(2), AnnotationId(3)];

        assert_eq!(
            previous_annotation_id(&ids, AnnotationId(1)),
            Some(AnnotationId(3))
        );
        assert_eq!(
            previous_annotation_id(&ids, AnnotationId(3)),
            Some(AnnotationId(2))
        );
        assert_eq!(previous_annotation_id(&ids, AnnotationId(9)), None);
    }
}
