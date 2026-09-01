//! The press/move/release lifecycles for form fields (T-141): placing a new
//! field when a kind is armed, and dragging an existing one by its body or a
//! corner handle — the form-field twin of `annotations::gesture`.
//!
//! `forms` owns the per-page gesture dispatch and calls in here; a `false`
//! return means the press was not claimed.

use pdf_document::{Command, FormFieldId};

use crate::app::selection;
use crate::app::state::{AnnotationDragMode, FormFieldDrag, FormPlacement, Viewer};

use super::builder::field_for_placement;
use super::command::{apply_command, command, model, structural_edit_refusal};
use super::fill::focus_field;
use super::geometry::{contains, corner_at, dragged_rect};
use super::toolbar::update_forms_controls;
use super::SELECTION_GONE;

/// Starts placing a field, if a kind is armed. Returns whether the drag was
/// claimed — a `false` leaves it to [`begin_field_drag`].
pub(crate) fn begin_placement(viewer: &Viewer, page_index: usize, point: (f64, f64)) -> bool {
    if structural_edit_refusal(viewer).is_some() {
        return false;
    }
    let mut state = viewer.state.borrow_mut();
    let Some(kind) = state.form_field_kind else {
        return false;
    };
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    session.form_placement = Some(FormPlacement {
        kind,
        page_index,
        origin: point,
        current: point,
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
            .and_then(|session| session.form_placement.as_mut())
        else {
            return false;
        };
        placement.current = point;
    }
    selection::redraw(viewer);
    true
}

/// Commits the placement in flight, if any, and disarms the placement kind —
/// the same "one placement, then back to select" posture
/// `annotations::gesture::finish_placement` documents: an armed kind that
/// stayed armed would turn the next stray drag into another field.
pub(crate) fn finish_placement(viewer: &Viewer) {
    let placement = {
        let mut state = viewer.state.borrow_mut();
        match state
            .session
            .as_mut()
            .and_then(|session| session.form_placement.take())
        {
            Some(placement) => placement,
            None => return,
        }
    };
    super::set_field_kind(viewer, None);
    command(viewer, move |session| {
        let id = FormFieldId(session.next_form_field_id);
        {
            let document = model(session)?;
            let field = field_for_placement(&document.form_fields, id, &placement);
            apply_command(document, Command::AddFormField(field));
        }
        session.next_form_field_id += 1;
        session.selected_form_field = Some(id);
        Ok(format!(
            "{} added. Changes are pending save.",
            placement.kind.label()
        ))
    });
    // T-143: the panel's own focus follows the field this placement just
    // selected — read back from state rather than threading `id` out of the
    // closure above, since `command` already rebuilt `fill_rows` (and with
    // it `focus_targets`) by the time this runs.
    //
    // The lookup is bound to `selected` *before* the `if let`, not inlined
    // into its scrutinee: a `Ref` produced there would otherwise stay
    // borrowed for the whole block (temporary lifetime extension), including
    // the `focus_field` call below. `focus_field` calls `grab_focus`, which
    // GTK dispatches synchronously — it re-enters `fill::mark_selected`
    // through the widget's focus-enter signal before returning, and that
    // handler needs its own `borrow_mut()` on the same `RefCell`. With the
    // outer borrow still held, that collision panicked with "RefCell already
    // borrowed" inside a GTK C callback, which cannot unwind — aborting the
    // whole process instead of just this handler.
    let selected = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.selected_form_field);
    if let Some(id) = selected {
        focus_field(viewer, id);
    }
}

/// Grabs a field under the pointer: a corner handle of the selected one
/// resizes it, its body moves it, and any other field on the page becomes
/// the selection. Returns whether the drag was claimed — a `false` leaves
/// the press to deselect (or, outside forms-edit mode, to reach text
/// selection instead — `forms::begin_drag`'s caller decides that part).
///
/// Tried after [`begin_placement`]: a press that lands while a kind is armed
/// always places a new field, never targets an existing one — mirrors
/// `content_edit::handle_drag_end`'s own "an insert kind always creates,
/// never falls back to hit-testing" rule.
pub(crate) fn begin_field_drag(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    reach: f64,
) -> bool {
    if structural_edit_refusal(viewer).is_some() {
        return false;
    }
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    let Some(document) = session.document_model.as_ref() else {
        return false;
    };

    // The selected field gets first refusal on the press, so its handles
    // stay reachable even where another field overlaps them — mirrors
    // `annotations::gesture::begin_annotation_drag`'s own precedence.
    let selected = session
        .selected_form_field
        .and_then(|id| document.form_fields.get(id))
        .filter(|field| field.page.0 as usize == page_index);
    if let Some(field) = selected {
        let mode = match corner_at(field.rect, point, reach) {
            Some(corner) => Some(AnnotationDragMode::Resize(corner)),
            None if contains(field.rect, point) => Some(AnnotationDragMode::Move),
            None => None,
        };
        if let Some(mode) = mode {
            session.form_field_drag = Some(FormFieldDrag {
                id: field.id,
                mode,
                origin: point,
                current: point,
            });
            return true;
        }
    }

    // Topmost first: the set paints in order, so the last one drawn is the
    // one the user sees on top and means to click.
    let hit = document
        .form_fields
        .iter()
        .filter(|field| field.page.0 as usize == page_index)
        .filter(|field| contains(field.rect, point))
        .last()
        .map(|field| field.id);
    match hit {
        Some(id) => {
            session.selected_form_field = Some(id);
            session.form_field_drag = Some(FormFieldDrag {
                id,
                mode: AnnotationDragMode::Move,
                origin: point,
                current: point,
            });
            drop(state);
            update_forms_controls(viewer);
            selection::redraw(viewer);
            // T-143: clicking a different field into selection also moves
            // the panel's keyboard focus to it.
            focus_field(viewer, id);
            true
        }
        None => {
            let deselected = session.selected_form_field.take().is_some();
            drop(state);
            if deselected {
                update_forms_controls(viewer);
                selection::redraw(viewer);
            }
            false
        }
    }
}

/// Extends the field drag in flight. Returns whether there was one.
pub(crate) fn extend_field_drag(viewer: &Viewer, point: (f64, f64)) -> bool {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(drag) = state
            .session
            .as_mut()
            .and_then(|session| session.form_field_drag.as_mut())
        else {
            return false;
        };
        drag.current = point;
    }
    selection::redraw(viewer);
    true
}

/// Commits the field drag in flight, if any. A drag that never moved is just
/// the click that selected the field, so it records nothing.
pub(crate) fn finish_field_drag(viewer: &Viewer) {
    let drag = {
        let mut state = viewer.state.borrow_mut();
        match state
            .session
            .as_mut()
            .and_then(|session| session.form_field_drag.take())
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
            .form_fields
            .get(drag.id)
            .map(|field| field.rect)
            .ok_or_else(|| SELECTION_GONE.to_string())?;
        let after = dragged_rect(before, &drag);
        apply_command(
            document,
            match drag.mode {
                AnnotationDragMode::Move => Command::MoveFormField {
                    id: drag.id,
                    from: before,
                    to: after,
                },
                AnnotationDragMode::Resize(_) => Command::ResizeFormField {
                    id: drag.id,
                    from: before,
                    to: after,
                },
            },
        );
        Ok(match drag.mode {
            AnnotationDragMode::Move => "Field moved. Changes are pending save.".to_string(),
            AnnotationDragMode::Resize(_) => "Field resized. Changes are pending save.".to_string(),
        })
    });
}
