//! GTK annotation toolbar, the two-step placement gesture behind it, and the
//! adapter from those UI actions to the undoable `pdf-annotate` command
//! surface.
//!
//! Creating an annotation takes two steps: arm a tool on the toolbar, then
//! draw it on a page. The drag half is driven from `selection`, which owns the
//! per-page gesture — an armed tool claims the drag that would otherwise
//! select text.

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Orientation, PolicyType, ScrolledWindow, ToggleButton};
use pdf_document::{
    Annotation, AnnotationId, AnnotationKind, Color, Command, Document, PageId, Rect,
};

use super::selection;
use super::state::{
    AnnotationToolbar, DocumentSession, Placement, Tool, Viewer, ANNOTATION_MODEL_UNAVAILABLE,
};

/// Fallback rect for the one operation that can still need one without a
/// pointer behind it — see [`edit_resize`].
const DEFAULT_RECT: Rect = Rect {
    x: 72.0,
    y: 72.0,
    width: 144.0,
    height: 36.0,
};
const DEFAULT_COLOR: Color = Color {
    r: 255,
    g: 220,
    b: 0,
};
const RESTYLE_COLOR: Color = Color {
    r: 220,
    g: 40,
    b: 40,
};
/// How far the Move button nudges the selected annotation, in PDF points.
const NUDGE_PT: f64 = 12.0;
/// How much the Resize button grows the selected annotation.
const RESIZE_FACTOR: f64 = 1.25;

/// Size given to an annotation the user placed with a click rather than a
/// drag, in PDF points.
const CLICK_SIZE_PT: (f64, f64) = (144.0, 36.0);
/// A drag shorter than this on either axis counts as a click.
///
/// Without a floor, a click that wobbles by one pixel produces a sliver the
/// user can neither see nor grab again — and the annotation would be
/// effectively unrecoverable, since deleting it needs it selected.
const MIN_DRAG_PT: f64 = 8.0;

const SELECT_FIRST: &str = "Select an annotation first.";
const NO_DOCUMENT: &str = "Open a PDF before editing annotations.";
const SELECTION_GONE: &str = "The selected annotation no longer exists.";
const INK_NEEDS_A_DRAG: &str = "Drag to draw an ink annotation.";

/// A 1×1 opaque PNG, inlined so the Stamp button has something valid to stamp
/// without reaching for a file. `stamp_from_image_bytes` decodes it to read the
/// alpha channel, so a placeholder still has to be a real image — this is the
/// smallest one that is. Replaced by the clipboard/drag-and-drop image once
/// T-050/T-051 land.
const PLACEHOLDER_STAMP_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0,
    0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 207, 192, 0, 0, 3, 1, 1,
    0, 24, 221, 141, 176, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Builds the annotation toolbar and the row that carries it.
///
/// The row is a `ScrolledWindow`, and that is load-bearing, not decoration.
/// A plain `GtkBox` reports its natural width as the window's *minimum* width,
/// and these twelve labelled controls need more of it than a laptop screen
/// has. On maximize the compositor hands the window a fixed size; a GTK window
/// that cannot shrink to it commits an oversized buffer and Wayland kills the
/// client outright:
///
/// ```text
/// xdg_surface buffer (2039 x 1032) does not match
/// the configured maximized state (1920 x 1032)
/// ```
///
/// A `ScrolledWindow` has a small minimum width regardless of its child, so
/// the window can always reach the size it is told to be, and the controls
/// that do not fit scroll instead of crashing the app. Do not swap it back for
/// a bare box, and do not "fix" a future overflow by widening the window.
pub(crate) fn add_annotation_toolbar() -> (AnnotationToolbar, ScrolledWindow) {
    let annotation_toolbar = GtkBox::new(Orientation::Horizontal, 4);
    // Nothing is editable until a document opens and reports that it permits
    // annotation changes — `update_annotation_controls` owns every transition
    // out of that state, for both button kinds.
    let toggle = |tool: Tool| {
        let button = ToggleButton::with_label(tool.label());
        button.set_sensitive(false);
        annotation_toolbar.append(&button);
        (tool, button)
    };
    let button = |label: &str| {
        let button = Button::with_label(label);
        button.set_sensitive(false);
        annotation_toolbar.append(&button);
        button
    };

    let create = Tool::ALL.iter().map(|&tool| toggle(tool)).collect();
    let toolbar_buttons = AnnotationToolbar {
        create,
        select_previous: button("Previous annotation"),
        move_selection: button("Move"),
        resize_selection: button("Resize"),
        restyle_selection: button("Restyle"),
        delete_selection: button("Delete"),
    };

    let row = ScrolledWindow::builder()
        .child(&annotation_toolbar)
        .hscrollbar_policy(PolicyType::Automatic)
        // One row tall: without this the scroller claims vertical space it has
        // no use for and steals it from the page area.
        .vscrollbar_policy(PolicyType::Never)
        .propagate_natural_height(true)
        .build();
    (toolbar_buttons, row)
}

pub(crate) fn connect_annotation_toolbar(viewer: &Viewer) {
    for (tool, button) in &viewer.annotation_buttons.create {
        button.connect_toggled({
            let viewer = viewer.clone();
            let tool = *tool;
            move |button| arm_tool(&viewer, tool, button.is_active())
        });
    }

    let buttons = &viewer.annotation_buttons;
    connect(viewer, &buttons.select_previous, select_previous);
    connect(viewer, &buttons.move_selection, edit_move);
    connect(viewer, &buttons.resize_selection, edit_resize);
    connect(viewer, &buttons.restyle_selection, edit_restyle);
    connect(viewer, &buttons.delete_selection, delete);
}

fn connect(viewer: &Viewer, button: &Button, action: fn(&Viewer)) {
    button.connect_clicked({
        let viewer = viewer.clone();
        move |_| action(&viewer)
    });
}

/// Arms or disarms a creation tool, keeping at most one armed.
///
/// Switching tools untoggles the previous button, which re-enters this
/// function with `active = false`. That call is a no-op because the tool it
/// names is no longer the armed one — which is why the guard compares against
/// `active_tool` instead of just clearing it.
fn arm_tool(viewer: &Viewer, tool: Tool, active: bool) {
    {
        let mut state = viewer.state.borrow_mut();
        if active {
            state.active_tool = Some(tool);
        } else if state.active_tool == Some(tool) {
            state.active_tool = None;
        } else {
            return;
        }
    }
    if active {
        for (other, button) in &viewer.annotation_buttons.create {
            if *other != tool {
                button.set_active(false);
            }
        }
        viewer.status.set_text(&format!(
            "{} armed — drag on a page to place it.",
            tool.label()
        ));
    } else {
        viewer
            .status
            .set_text(&format!("{} disarmed.", tool.label()));
    }
}

/// Clears the armed tool and releases its button.
fn disarm(viewer: &Viewer) {
    viewer.state.borrow_mut().active_tool = None;
    for (_, button) in &viewer.annotation_buttons.create {
        button.set_active(false);
    }
}

pub(crate) fn update_annotation_controls(viewer: &Viewer) {
    let state = viewer.state.borrow();
    let buttons = &viewer.annotation_buttons;
    let Some(session) = state.session.as_ref() else {
        return;
    };
    let enabled = session.annotation_access.refusal().is_none();
    for (_, button) in &buttons.create {
        button.set_sensitive(enabled);
    }

    let selected = session
        .selected_annotation
        .and_then(|id| session.document_model.as_ref()?.annotations.get(id));
    let has_selection = enabled && selected.is_some();
    buttons.select_previous.set_sensitive(has_selection);
    buttons.move_selection.set_sensitive(has_selection);
    buttons
        .resize_selection
        .set_sensitive(enabled && selected.is_some_and(supports_resize));
    buttons
        .restyle_selection
        .set_sensitive(enabled && selected.is_some_and(supports_restyle));
    buttons.delete_selection.set_sensitive(has_selection);
}

/// Runs one annotation command against the open document, then reports the
/// outcome and repaints.
///
/// Every button shares this shape: refuse early when the document withholds
/// the permission, resolve the session, run the command, report. Keeping the
/// refusal in one place is what stops a future button from forgetting it.
fn command(
    viewer: &Viewer,
    operation: impl FnOnce(&mut DocumentSession) -> Result<String, String>,
) {
    if let Some(refusal) = viewer.annotation_editing_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    // The borrow ends before the reporting below: both
    // `update_annotation_controls` and `redraw` borrow the state again.
    let result = {
        let mut state = viewer.state.borrow_mut();
        match state.session.as_mut() {
            Some(session) => operation(session),
            None => Err(NO_DOCUMENT.to_string()),
        }
    };
    match result {
        Ok(message) => viewer.status.set_text(&message),
        Err(error) => viewer.status.set_text(&error),
    }
    // Both outcomes, not just success: a rejected placement has already been
    // taken off the session, and its preview has to stop being painted.
    update_annotation_controls(viewer);
    selection::redraw(viewer);
}

/// The editable model for the open document.
///
/// Unreachable in practice — a session without a model reports
/// `AnnotationAccess::Unavailable`, which [`command`] refuses before it gets
/// here — but it reports the same message that refusal does, so the two can
/// never contradict each other if that ever stops being true.
fn model(session: &mut DocumentSession) -> Result<&mut Document, String> {
    session
        .document_model
        .as_mut()
        .ok_or_else(|| ANNOTATION_MODEL_UNAVAILABLE.to_string())
}

/// Records `command` in the document's own `EditLog`.
///
/// The log lives *inside* the document it mutates, so it is moved out for the
/// duration of the call and put back afterwards — `EditLog::apply` needs a
/// `&mut Document` that cannot also be borrowed through the log.
fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

/// Builds the annotation a finished [`Placement`] describes.
///
/// The geometry comes from where the user actually dragged — this is the one
/// place the pointer becomes an annotation, and the reason no creation path
/// carries a hard-coded rect any more.
fn annotation_at(
    placement: &Placement,
    id: AnnotationId,
    page: PageId,
) -> Result<Annotation, String> {
    let rect = placement_rect(placement);
    Ok(match placement.tool {
        Tool::Highlight => pdf_annotate::highlight(id, page, rect, DEFAULT_COLOR),
        Tool::Underline => pdf_annotate::underline(id, page, rect, DEFAULT_COLOR),
        Tool::Strikeout => pdf_annotate::strikeout(id, page, rect, DEFAULT_COLOR),
        Tool::Ink => {
            // A tap leaves a single point, which is not a stroke — better to
            // say so than to record an invisible annotation.
            if placement.points.len() < 2 {
                return Err(INK_NEEDS_A_DRAG.to_string());
            }
            pdf_annotate::ink(id, page, placement.points.clone(), DEFAULT_COLOR)
        }
        Tool::TextNote => pdf_annotate::text_note(id, page, rect, "Note"),
        Tool::Shape => pdf_annotate::shape(id, page, rect, DEFAULT_COLOR),
        Tool::Stamp => pdf_annotate::stamp_from_image_bytes(id, page, PLACEHOLDER_STAMP_PNG, rect)
            .map_err(|error| error.to_string())?,
    })
}

/// The rect a placement drag traced, normalised so dragging in any of the four
/// directions produces the same rectangle.
///
/// A drag too small on either axis is treated as a click: the annotation gets
/// a default size with the press point at its top-left corner. PDF space has a
/// bottom-left origin, so "top-left" is `y - height`.
fn placement_rect(placement: &Placement) -> Rect {
    let (origin_x, origin_y) = placement.origin;
    let (current_x, current_y) = placement.current;
    let width = (current_x - origin_x).abs();
    let height = (current_y - origin_y).abs();

    if width < MIN_DRAG_PT || height < MIN_DRAG_PT {
        let (width, height) = CLICK_SIZE_PT;
        return Rect {
            x: origin_x,
            y: origin_y - height,
            width,
            height,
        };
    }
    Rect {
        x: origin_x.min(current_x),
        y: origin_y.min(current_y),
        width,
        height,
    }
}

/// The annotation to paint for a placement still in progress, or `None` when
/// it cannot be built yet (an ink stroke of one point, say).
///
/// Returns a real [`Annotation`] so the preview goes through the same painter
/// as a committed one — what the user drags is what they get. The id is
/// irrelevant to drawing and never reaches the document.
pub(crate) fn placement_preview(placement: &Placement) -> Option<Annotation> {
    annotation_at(
        placement,
        AnnotationId(0),
        PageId(placement.page_index as u32),
    )
    .ok()
}

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
        let annotation = annotation_at(&placement, id, page)?;
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

fn edit_move(viewer: &Viewer) {
    edit(viewer, |annotation| {
        pdf_annotate::move_annotation(annotation, NUDGE_PT, NUDGE_PT)
    });
}

fn edit_resize(viewer: &Viewer) {
    edit(viewer, |annotation| {
        let rect = resize_rect(annotation).unwrap_or(DEFAULT_RECT);
        pdf_annotate::resize_annotation(annotation, rect)
    });
}

fn edit_restyle(viewer: &Viewer) {
    edit(viewer, |annotation| {
        pdf_annotate::restyle_annotation(annotation, RESTYLE_COLOR)
    });
}

fn delete(viewer: &Viewer) {
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
/// Not routed through [`command`]: changing which annotation is selected is
/// not an edit, records nothing in the `EditLog`, and so is not something the
/// document's annotate permission has any say over.
fn select_previous(viewer: &Viewer) {
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

fn supports_resize(annotation: &Annotation) -> bool {
    resize_rect(annotation).is_some()
}

fn supports_restyle(annotation: &Annotation) -> bool {
    matches!(
        &annotation.kind,
        AnnotationKind::Highlight { .. }
            | AnnotationKind::Underline { .. }
            | AnnotationKind::Strikeout { .. }
            | AnnotationKind::Ink { .. }
            | AnnotationKind::Shape { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A placement drag from `origin` to `end` on page 0, recording every
    /// point the way the gesture does for freehand tools.
    fn drag(tool: Tool, origin: (f64, f64), end: (f64, f64)) -> Placement {
        Placement {
            tool,
            page_index: 0,
            origin,
            current: end,
            points: if tool.is_freehand() {
                vec![origin, end]
            } else {
                Vec::new()
            },
        }
    }

    /// A click: pointer down and up in the same spot, no drag.
    fn click(tool: Tool, at: (f64, f64)) -> Placement {
        Placement {
            tool,
            page_index: 0,
            origin: at,
            current: at,
            points: if tool.is_freehand() { vec![at] } else { vec![] },
        }
    }

    fn built(placement: &Placement) -> Annotation {
        annotation_at(placement, AnnotationId(1), PageId(0)).unwrap_or_else(|error| {
            panic!("{:?} must build from a placement: {error}", placement.tool)
        })
    }

    fn rect_of(annotation: &Annotation) -> Rect {
        match &annotation.kind {
            AnnotationKind::Highlight { rect, .. }
            | AnnotationKind::Underline { rect, .. }
            | AnnotationKind::Strikeout { rect, .. }
            | AnnotationKind::Shape { rect, .. }
            | AnnotationKind::TextNote { rect, .. }
            | AnnotationKind::Stamp { rect, .. } => *rect,
            _ => panic!("kind has no rect"),
        }
    }

    #[test]
    fn every_toolbar_tool_builds_the_annotation_kind_its_label_promises() {
        for (tool, expected) in [
            (Tool::Highlight, "Highlight"),
            (Tool::Underline, "Underline"),
            (Tool::Strikeout, "Strikeout"),
            (Tool::Ink, "Ink"),
            (Tool::TextNote, "TextNote"),
            (Tool::Shape, "Shape"),
            (Tool::Stamp, "Stamp"),
        ] {
            let kind = built(&drag(tool, (100.0, 100.0), (200.0, 160.0))).kind;
            let name = match kind {
                AnnotationKind::Highlight { .. } => "Highlight",
                AnnotationKind::Underline { .. } => "Underline",
                AnnotationKind::Strikeout { .. } => "Strikeout",
                AnnotationKind::Ink { .. } => "Ink",
                AnnotationKind::TextNote { .. } => "TextNote",
                AnnotationKind::Shape { .. } => "Shape",
                AnnotationKind::Stamp { .. } => "Stamp",
                _ => "unknown",
            };
            assert_eq!(name, expected, "{tool:?} built the wrong kind");
        }
    }

    #[test]
    fn the_toolbar_offers_one_button_per_annotation_type() {
        // The spec calls for seven annotation types; the buttons and the
        // handler wiring both read from `Tool::ALL`, so this pins the count.
        assert_eq!(Tool::ALL.len(), 7);
    }

    #[test]
    fn every_tool_has_its_own_label() {
        let mut labels: Vec<_> = Tool::ALL.iter().map(|tool| tool.label()).collect();
        labels.sort_unstable();
        labels.dedup();

        assert_eq!(labels.len(), Tool::ALL.len());
    }

    /// The whole point of the placement gesture: the annotation lands where
    /// the user dragged, not at a fixed rect.
    #[test]
    fn a_drag_places_the_annotation_where_it_was_drawn() {
        let rect = rect_of(&built(&drag(
            Tool::Highlight,
            (120.0, 400.0),
            (300.0, 460.0),
        )));

        assert_eq!((rect.x, rect.y), (120.0, 400.0));
        assert_eq!((rect.width, rect.height), (180.0, 60.0));
    }

    /// PDF space has a bottom-left origin, so a drag "down and left" on screen
    /// still has to normalise into the same rectangle.
    #[test]
    fn a_drag_in_any_direction_produces_the_same_rect() {
        let forward = rect_of(&built(&drag(Tool::Shape, (120.0, 400.0), (300.0, 460.0))));
        let backward = rect_of(&built(&drag(Tool::Shape, (300.0, 460.0), (120.0, 400.0))));

        assert_eq!((forward.x, forward.y), (backward.x, backward.y));
        assert_eq!(
            (forward.width, forward.height),
            (backward.width, backward.height)
        );
    }

    /// A click is a legitimate way to drop an annotation; it must not produce
    /// a zero-size one the user can never select again.
    #[test]
    fn a_click_places_a_default_sized_annotation_at_the_pointer() {
        let rect = rect_of(&built(&click(Tool::TextNote, (200.0, 500.0))));
        let (width, height) = CLICK_SIZE_PT;

        assert_eq!((rect.width, rect.height), (width, height));
        // Anchored by its top-left corner, which in PDF space is `y - height`.
        assert_eq!((rect.x, rect.y), (200.0, 500.0 - height));
    }

    #[test]
    fn a_drag_below_the_minimum_is_treated_as_a_click() {
        let barely = drag(
            Tool::Shape,
            (200.0, 500.0),
            (200.0 + MIN_DRAG_PT / 2.0, 500.0 + MIN_DRAG_PT / 2.0),
        );
        let rect = rect_of(&built(&barely));

        assert_eq!((rect.width, rect.height), CLICK_SIZE_PT);
    }

    #[test]
    fn an_ink_stroke_keeps_every_point_the_pointer_visited() {
        let mut placement = drag(Tool::Ink, (10.0, 10.0), (30.0, 40.0));
        placement.points = vec![(10.0, 10.0), (20.0, 25.0), (30.0, 40.0)];

        match built(&placement).kind {
            AnnotationKind::Ink { points, .. } => assert_eq!(points, placement.points),
            other => panic!("expected an ink annotation, got {other:?}"),
        }
    }

    /// A tap with the ink tool is not a stroke. Refusing it beats recording an
    /// annotation that paints nothing and cannot be found again.
    #[test]
    fn a_tap_with_the_ink_tool_is_refused_rather_than_recorded() {
        let error = annotation_at(&click(Tool::Ink, (10.0, 10.0)), AnnotationId(1), PageId(0))
            .expect_err("a single point is not a stroke");

        assert_eq!(error, INK_NEEDS_A_DRAG);
    }

    #[test]
    fn a_placement_preview_paints_the_same_annotation_the_drag_will_commit() {
        let placement = drag(Tool::Highlight, (120.0, 400.0), (300.0, 460.0));

        let preview = placement_preview(&placement).expect("a drag previews");

        assert_eq!(rect_of(&preview), rect_of(&built(&placement)));
    }

    #[test]
    fn a_placement_that_cannot_be_built_yet_has_no_preview() {
        assert!(placement_preview(&click(Tool::Ink, (10.0, 10.0))).is_none());
    }

    #[test]
    fn the_placeholder_stamp_decodes_as_a_real_image() {
        assert!(annotation_at(
            &drag(Tool::Stamp, (10.0, 10.0), (100.0, 100.0)),
            AnnotationId(1),
            PageId(0)
        )
        .is_ok());
    }

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
