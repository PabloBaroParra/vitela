//! GTK annotation toolbar, the two-step placement gesture behind it, and the
//! adapter from those UI actions to the undoable `pdf-annotate` command
//! surface.
//!
//! Creating an annotation takes two steps: arm a tool on the toolbar, then
//! draw it on a page. The drag half is driven from `selection`, which owns the
//! per-page gesture — an armed tool claims the drag that would otherwise
//! select text.

use gtk::prelude::*;
use gtk::{
    gdk, gio, Box as GtkBox, Button, ColorChooserDialog, Orientation, PolicyType, ResponseType,
    ScrolledWindow, ToggleButton,
};
use pdf_document::{
    Annotation, AnnotationId, AnnotationKind, Color, Command, Document, PageId, Rect,
};
use pdf_render::TextRect;

use super::selection;
use super::state::{
    AnnotationDrag, AnnotationDragMode, AnnotationToolbar, Corner, DocumentSession, Placement,
    SessionToken, Tool, Viewer, ANNOTATION_MODEL_UNAVAILABLE,
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
/// How far the Nudge button shifts the selected annotation, in PDF points.
const NUDGE_PT: f64 = 12.0;
/// How much the Grow button enlarges the selected annotation.
const RESIZE_FACTOR: f64 = 1.25;

/// Size given to an annotation the user placed with a click rather than a
/// drag, in PDF points.
const CLICK_SIZE_PT: (f64, f64) = (144.0, 36.0);
/// How far the pointer must travel from the press point before the gesture
/// counts as a drag rather than a click.
///
/// Without a click affordance, a press that wobbles by one pixel would produce
/// a sliver the user can neither see nor grab again — and it would be
/// effectively unrecoverable, since deleting an annotation needs it selected.
const MIN_DRAG_PT: f64 = 8.0;
/// Floor for each side of a traced rect, so a flat sweep still leaves
/// something visible and selectable behind.
const MIN_TRACED_PT: f64 = 4.0;
/// Height of the band a freehand rule occupies, in PDF points. Thin enough to
/// read as a line, tall enough that the rect stays a real target for the edit
/// buttons rather than a degenerate one.
const RULE_BAND_PT: f64 = 2.0;

const SELECT_FIRST: &str = "Select an annotation first.";
const NO_DOCUMENT: &str = "Open a PDF before editing annotations.";
const SELECTION_GONE: &str = "The selected annotation no longer exists.";
const INK_NEEDS_A_DRAG: &str = "Drag to draw an ink annotation.";
const DRAG_UNSUPPORTED: &str = "That annotation cannot be changed this way.";

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
        // Named for what they actually do. These are fixed-step fine
        // adjustments; dragging the annotation itself is how you place it.
        move_selection: button("Nudge"),
        resize_selection: button("Grow"),
        restyle_selection: button("Restyle"),
        delete_selection: button("Delete"),
        delete_action: gio::SimpleAction::new("delete-annotation", None),
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
    connect(viewer, &buttons.restyle_selection, choose_restyle_color);
    connect(viewer, &buttons.delete_selection, delete);
}

/// Wires the Delete key to removing the selected annotation.
///
/// A window action with an accelerator, not an `EventControllerKey` — the same
/// reason Ctrl+C is one (see `selection::connect_copy`): a key controller runs
/// in the bubble phase, so whichever widget holds focus swallows the key
/// first.
///
/// An accelerator has the opposite hazard: it is resolved ahead of the focus
/// chain, so a live `Delete` accel would take the key away from the search
/// entry mid-word. Hence the enabled state, maintained by
/// [`update_annotation_controls`] — a *disabled* action does not consume its
/// accelerator, so the key travels on to the entry exactly as before.
pub(crate) fn connect_delete_shortcut(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    let action = &viewer.annotation_buttons.delete_action;
    action.set_enabled(false);
    action.connect_activate({
        let viewer = viewer.clone();
        move |_, _| delete(&viewer)
    });
    window.add_action(action);
    application.set_accels_for_action("win.delete-annotation", &["Delete"]);

    // Watched on the window, not on the entry: focus moving anywhere changes
    // whether the accelerator may fire, and the entry's own focus signal never
    // reports the case that matters (see [`search_has_focus`]).
    window.connect_focus_widget_notify({
        let viewer = viewer.clone();
        move |_| update_annotation_controls(&viewer)
    });
}

/// Whether the keyboard focus is inside the search entry.
///
/// Deliberately not `Entry::has_focus`. A `GtkEntry` delegates focus to an
/// internal `GtkText` child, so the entry itself never reports holding it —
/// the guard built on that call was dead, and Delete deleted the selected
/// annotation out from under someone typing a search term. Ask the window who
/// actually holds focus and whether the entry contains them.
fn search_has_focus(viewer: &Viewer) -> bool {
    let focused = viewer
        .search_entry
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok())
        // `focus` is ambiguous here: `RootExt` has one too, and it is the
        // window's notion of the focus widget that is wanted.
        .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window));
    focused.is_some_and(|focused| {
        focused == *viewer.search_entry.upcast_ref::<gtk::Widget>()
            || focused.is_ancestor(&viewer.search_entry)
    })
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
    // Text already selected plus a markup tool is not a request to arm
    // anything — it is the annotation itself. Applied here, before arming, so
    // the button never latches for a click that is already finished.
    if active && markup_text_selection(viewer, tool) {
        disarm(viewer);
        return;
    }
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

/// Marks up the current text selection, when there is one and `tool` is a
/// text-markup kind. Returns whether it handled the click.
///
/// One annotation per selected line, because `AnnotationKind` carries a single
/// rect: the PDF way to say "these three lines" in one annotation is
/// `/QuadPoints`, which the model does not express yet. Until it does, three
/// bands beat one box swallowing the margins between them.
fn markup_text_selection(viewer: &Viewer, tool: Tool) -> bool {
    if !tool.marks_up_text() {
        return false;
    }
    // Read before `command` takes its own mutable borrow of the state.
    let Some((page_index, rects)) = selection::selected_line_rects(viewer) else {
        return false;
    };
    command(viewer, move |session| {
        let page = PageId(page_index as u32);
        for rect in &rects {
            let id = AnnotationId(session.next_annotation_id);
            let annotation = markup_annotation(tool, id, page, text_rect_to_pdf(*rect))?;
            {
                let document = model(session)?;
                apply_command(document, Command::AddAnnotation(annotation));
            }
            session.next_annotation_id += 1;
            session.selected_annotation = Some(id);
        }
        // The selection has become the annotation; leaving it live would let a
        // second click stack an identical one over the same words.
        session.selection = None;
        Ok(format!(
            "{} applied to the selected text. Changes are pending save.",
            tool.label()
        ))
    });
    true
}

/// Widens a renderer-space text rect into the document model's `f64` rect.
fn text_rect_to_pdf(rect: TextRect) -> Rect {
    Rect {
        x: f64::from(rect.x_pt),
        y: f64::from(rect.y_pt),
        width: f64::from(rect.width_pt),
        height: f64::from(rect.height_pt),
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
    // The key and the button delete the same thing, so they light up together
    // — except while the search entry has focus, where Delete belongs to the
    // text being typed and the accelerator must stand down.
    buttons
        .delete_action
        .set_enabled(has_selection && !search_has_focus(viewer));
    let history = session
        .document_model
        .as_ref()
        .map(|document| &document.pending_edits);
    viewer
        .undo_action
        .set_enabled(history.is_some_and(|log| log.can_undo()));
    viewer
        .redo_action
        .set_enabled(history.is_some_and(|log| log.can_redo()));
    viewer
        .save_button
        .set_sensitive(history.is_some_and(|log| log.can_undo()));
}

/// Connects model-native history to the window actions and standard shortcuts.
pub(crate) fn connect_history_shortcuts(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    viewer.undo_action.connect_activate({
        let viewer = viewer.clone();
        move |_, _| undo(&viewer)
    });
    viewer.redo_action.connect_activate({
        let viewer = viewer.clone();
        move |_, _| redo(&viewer)
    });
    window.add_action(&viewer.undo_action);
    window.add_action(&viewer.redo_action);
    application.set_accels_for_action("win.undo", &["<Control>z"]);
    application.set_accels_for_action("win.redo", &["<Control>y"]);
}

pub(crate) fn undo(viewer: &Viewer) {
    history(viewer, true);
}

pub(crate) fn redo(viewer: &Viewer) {
    history(viewer, false);
}

fn history(viewer: &Viewer, undo: bool) {
    let changed = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(document) = session.document_model.as_mut() else {
            return;
        };
        let mut log = std::mem::take(&mut document.pending_edits);
        let changed = if undo {
            log.undo(document)
        } else {
            log.redo(document)
        };
        document.pending_edits = log;
        if changed {
            session.edit_revision += 1;
            if session
                .selected_annotation
                .is_some_and(|id| document.annotations.get(id).is_none())
            {
                session.selected_annotation = None;
            }
        }
        changed
    };
    if changed {
        viewer.status.set_text(if undo {
            "Edit undone. Changes are pending save."
        } else {
            "Edit redone. Changes are pending save."
        });
        update_annotation_controls(viewer);
        selection::redraw(viewer);
    }
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
        Ok(message) => {
            if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
                session.edit_revision += 1;
            }
            viewer.status.set_text(&message);
        }
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

/// Converts GTK's normalized RGB representation to the model's RGB-only color.
fn color_from_rgba(red: f64, green: f64, blue: f64, _alpha: f64) -> Option<Color> {
    fn channel(value: f64) -> Option<u8> {
        value
            .is_finite()
            .then(|| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    Some(Color {
        r: channel(red)?,
        g: channel(green)?,
        b: channel(blue)?,
    })
}

fn selected_color(annotation: &Annotation) -> Option<Color> {
    match annotation.kind {
        AnnotationKind::Highlight { color, .. }
        | AnnotationKind::Underline { color, .. }
        | AnnotationKind::Strikeout { color, .. }
        | AnnotationKind::Ink { color, .. }
        | AnnotationKind::Shape { color, .. } => Some(color),
        _ => None,
    }
}

fn choose_restyle_color(viewer: &Viewer) {
    let (token, id, before, initial) = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return;
        };
        let Some(id) = session.selected_annotation else {
            return;
        };
        let Some(before) = session
            .document_model
            .as_ref()
            .and_then(|document| document.annotations.get(id))
            .cloned()
        else {
            return;
        };
        let Some(initial) = selected_color(&before) else {
            return;
        };
        (
            SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            },
            id,
            before,
            initial,
        )
    };
    let parent = viewer
        .status
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let dialog = ColorChooserDialog::new(Some("Choose annotation color"), parent.as_ref());
    dialog.set_use_alpha(false);
    dialog.set_rgba(&gdk::RGBA::new(
        f32::from(initial.r) / 255.0,
        f32::from(initial.g) / 255.0,
        f32::from(initial.b) / 255.0,
        1.0,
    ));
    dialog.connect_response({
        let viewer = viewer.clone();
        move |dialog, response| {
            let chosen = dialog.rgba();
            dialog.destroy();
            if response != ResponseType::Ok {
                return;
            }
            let Some(color) = color_from_rgba(
                f64::from(chosen.red()),
                f64::from(chosen.green()),
                f64::from(chosen.blue()),
                f64::from(chosen.alpha()),
            ) else {
                return;
            };
            // The dialog is modeless as far as the model is concerned: anything
            // could have replaced the document or edited it while it was open.
            let current = {
                let state = viewer.state.borrow();
                state
                    .session
                    .as_ref()
                    .is_some_and(|session| token.matches(state.generation, session.edit_revision))
            };
            if !current {
                return;
            }
            // `connect_response` is an `Fn`, so the command below cannot consume
            // the captured annotation — it works from a clone per response.
            let before = before.clone();
            command(&viewer, move |session| {
                let current = session
                    .document_model
                    .as_ref()
                    .and_then(|document| document.annotations.get(id));
                if current != Some(&before) {
                    return Ok("Color selection is no longer current.".to_string());
                }
                let mut after = before.clone();
                pdf_annotate::restyle_annotation(&mut after, color)
                    .map_err(|error| error.to_string())?;
                let document = model(session)?;
                apply_command(document, Command::ReplaceAnnotation { before, after });
                Ok("Annotation restyled. Changes are pending save.".to_string())
            });
        }
    });
    dialog.present();
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
    rect: Rect,
) -> Result<Annotation, String> {
    match placement.tool {
        Tool::Highlight | Tool::Underline | Tool::Strikeout => {
            markup_annotation(placement.tool, id, page, rect)
        }
        Tool::Ink => {
            // A tap leaves a single point, which is not a stroke — better to
            // say so than to record an invisible annotation.
            if placement.points.len() < 2 {
                return Err(INK_NEEDS_A_DRAG.to_string());
            }
            Ok(pdf_annotate::ink(
                id,
                page,
                placement.points.clone(),
                DEFAULT_COLOR,
            ))
        }
        Tool::TextNote => Ok(pdf_annotate::text_note(id, page, rect, "Note")),
        Tool::Shape => Ok(pdf_annotate::shape(id, page, rect, DEFAULT_COLOR)),
        Tool::Stamp => pdf_annotate::stamp_from_image_bytes(id, page, PLACEHOLDER_STAMP_PNG, rect)
            .map_err(|error| error.to_string()),
    }
}

/// Builds one text-markup annotation over `rect`.
///
/// Shared by the two ways a markup annotation can be born: dragged over a
/// region, or applied to an existing text selection. Keeping one builder is
/// what stops the two paths from drifting into different colours or kinds.
fn markup_annotation(
    tool: Tool,
    id: AnnotationId,
    page: PageId,
    rect: Rect,
) -> Result<Annotation, String> {
    match tool {
        Tool::Highlight => Ok(pdf_annotate::highlight(id, page, rect, DEFAULT_COLOR)),
        Tool::Underline => Ok(pdf_annotate::underline(id, page, rect, DEFAULT_COLOR)),
        Tool::Strikeout => Ok(pdf_annotate::strikeout(id, page, rect, DEFAULT_COLOR)),
        other => Err(format!("{} does not mark up text.", other.label())),
    }
}

/// Exactly the rect the pointer traced, normalised so dragging in any of the
/// four directions produces the same rectangle.
///
/// Each side is floored at [`MIN_TRACED_PT`] so a perfectly flat sweep — which
/// is what highlighting a line of text *is* — cannot collapse into a
/// zero-height band that paints nothing and can never be selected again. A
/// floor only stops the rect shrinking; it never moves it, so the preview
/// keeps tracking the pointer.
fn traced_rect(placement: &Placement) -> Rect {
    let (origin_x, origin_y) = placement.origin;
    let (current_x, current_y) = placement.current;
    let width = (current_x - origin_x).abs().max(MIN_TRACED_PT);

    if placement.tool.draws_a_rule() {
        // Pinned to the press point: the rule stays exactly where the pointer
        // went down, however much the hand drifted while sweeping sideways.
        return Rect {
            x: origin_x.min(current_x),
            y: origin_y,
            width,
            height: RULE_BAND_PT,
        };
    }
    Rect {
        x: origin_x.min(current_x),
        y: origin_y.min(current_y),
        width,
        height: (current_y - origin_y).abs().max(MIN_TRACED_PT),
    }
}

/// A default-sized rect with the press point at its top-left corner. PDF space
/// has a bottom-left origin, so "top-left" is `y - height`.
fn click_rect(placement: &Placement) -> Rect {
    let (origin_x, origin_y) = placement.origin;
    let (width, height) = CLICK_SIZE_PT;

    if placement.tool.draws_a_rule() {
        // A default-length rule *on* the press point, not a box hanging below
        // it — dropping the line 36pt away from the click would be a surprise.
        return Rect {
            x: origin_x,
            y: origin_y,
            width,
            height: RULE_BAND_PT,
        };
    }
    Rect {
        x: origin_x,
        y: origin_y - height,
        width,
        height,
    }
}

/// Inserts an image stamp at a PDF-space point through the same command path
/// as every toolbar annotation, preserving permission checks, undo, redraw,
/// and control state.
pub(crate) fn stamp_from_image_bytes(
    viewer: &Viewer,
    page_index: usize,
    point: (f64, f64),
    image_bytes: Vec<u8>,
) {
    command(viewer, move |session| {
        let id = AnnotationId(session.next_annotation_id);
        let page = PageId(page_index as u32);
        let rect = stamp_rect(&image_bytes, point)
            .map_err(|error| format!("Could not use the image: {error}"))?;
        let annotation = pdf_annotate::stamp_from_image_bytes(id, page, &image_bytes, rect)
            .map_err(|error| format!("Could not use the image: {error}"))?;
        {
            let document = model(session)?;
            apply_command(document, Command::AddAnnotation(annotation));
        }
        // The surface is GTK-only session state: PDF save keeps using the
        // annotation's core image data, while this gives the pending edit a
        // live shell preview. A failed shell decode still leaves the valid PDF
        // annotation in place and uses the outline fallback when painted.
        if let Some(surface) = selection::stamp_surface(image_bytes) {
            session.stamp_surfaces.insert(id, surface);
        }
        session.next_annotation_id += 1;
        session.selected_annotation = Some(id);
        Ok("Image stamp added. Changes are pending save.".to_string())
    });
}

/// Default stamp placement anchored at the pointer's top-left corner.
///
/// The size comes from the core, not from here: a dropped or pasted image
/// keeps its own proportions, so the WinUI shell — which reaches the same
/// function through `pdf_ffi::stamp_placement` — cannot disagree with this one
/// about where a given image lands.
fn stamp_rect(image_bytes: &[u8], point: (f64, f64)) -> Result<Rect, pdf_annotate::AnnotateError> {
    pdf_annotate::stamp_placement(image_bytes, point, pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT)
}

/// Whether the pointer travelled far enough for this to be a drag rather than
/// a click.
///
/// Measured as distance from the press point, **not** per axis. Highlighting a
/// line of text is a deliberately wide, shallow sweep whose height stays under
/// any sane threshold; judging that axis on its own called a real drag a
/// click, so the preview jumped to a default-sized box, and crossing the
/// threshold while moving up and down made it flip back and forth.
fn is_a_drag(placement: &Placement) -> bool {
    let (origin_x, origin_y) = placement.origin;
    let (current_x, current_y) = placement.current;
    let across = current_x - origin_x;

    if placement.tool.draws_a_rule() {
        // Only travel along the rule counts, for the same reason its height is
        // pinned: vertical movement changes nothing about the mark, so letting
        // it decide drag-versus-click would contradict what the user sees.
        return across.abs() >= MIN_DRAG_PT;
    }
    across.hypot(current_y - origin_y) >= MIN_DRAG_PT
}

/// The rect to commit: what the pointer traced, or a default-sized box when it
/// barely moved.
///
/// Decided here, at the end of the gesture, rather than continuously while it
/// runs. A mid-drag decision means the shape under the pointer can change
/// identity as the threshold is crossed, which reads as the preview popping.
fn committed_rect(placement: &Placement) -> Rect {
    if is_a_drag(placement) {
        traced_rect(placement)
    } else {
        click_rect(placement)
    }
}

/// The annotation to paint for a placement still in progress, or `None` when
/// it cannot be built yet (an ink stroke of one point, say).
///
/// Always the traced rect: while the pointer is down the preview follows it
/// exactly, with no threshold to cross and nothing to pop. The click fallback
/// belongs to [`committed_rect`], and a click has no drag to preview anyway.
pub(crate) fn placement_preview(placement: &Placement) -> Option<Annotation> {
    annotation_at(
        placement,
        AnnotationId(0),
        PageId(placement.page_index as u32),
        traced_rect(placement),
    )
    .ok()
}

/// The rectangle an annotation occupies, whatever its kind.
///
/// Ink has no rect of its own, so it reports the bounding box of its stroke —
/// enough to hit-test and to move, though not to resize (`pdf-annotate`
/// refuses that, and squashing a polyline into a box would be a guess).
pub(crate) fn bounds(annotation: &Annotation) -> Option<Rect> {
    match &annotation.kind {
        AnnotationKind::Highlight { rect, .. }
        | AnnotationKind::Underline { rect, .. }
        | AnnotationKind::Strikeout { rect, .. }
        | AnnotationKind::Shape { rect, .. }
        | AnnotationKind::TextNote { rect, .. }
        | AnnotationKind::Stamp { rect, .. } => Some(*rect),
        AnnotationKind::Ink { points, .. } => {
            let (first_x, first_y) = *points.first()?;
            let (mut min_x, mut min_y) = (first_x, first_y);
            let (mut max_x, mut max_y) = (first_x, first_y);
            for &(x, y) in points {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            Some(Rect {
                x: min_x,
                y: min_y,
                width: max_x - min_x,
                height: max_y - min_y,
            })
        }
        _ => None,
    }
}

/// Where a corner sits, in PDF page space.
fn corner_point(rect: Rect, corner: Corner) -> (f64, f64) {
    let (left, bottom) = (rect.x, rect.y);
    let (right, top) = (rect.x + rect.width, rect.y + rect.height);
    match corner {
        Corner::BottomLeft => (left, bottom),
        Corner::BottomRight => (right, bottom),
        Corner::TopLeft => (left, top),
        Corner::TopRight => (right, top),
    }
}

/// The corner whose handle is under `point`, if any.
///
/// `reach` is the grab radius in PDF points. The caller derives it from the
/// zoom so the handle stays the same size under the pointer however far the
/// page is scaled — a handle that shrank with the page would become
/// impossible to hit when zoomed out.
fn corner_at(rect: Rect, point: (f64, f64), reach: f64) -> Option<Corner> {
    Corner::ALL.into_iter().find(|corner| {
        let (corner_x, corner_y) = corner_point(rect, *corner);
        (point.0 - corner_x).abs() <= reach && (point.1 - corner_y).abs() <= reach
    })
}

/// Whether `point` is inside the annotation's body.
fn contains(rect: Rect, point: (f64, f64)) -> bool {
    point.0 >= rect.x
        && point.0 <= rect.x + rect.width
        && point.1 >= rect.y
        && point.1 <= rect.y + rect.height
}

/// The rect a resize drag produces: the grabbed corner follows the pointer,
/// the opposite corner stays put, and the result is normalised so dragging a
/// corner past its opposite flips the rect rather than inverting it.
fn resized_rect(rect: Rect, corner: Corner, point: (f64, f64)) -> Rect {
    let (anchor_x, anchor_y) = corner_point(rect, opposite(corner));
    Rect {
        x: anchor_x.min(point.0),
        y: anchor_y.min(point.1),
        width: (point.0 - anchor_x).abs().max(MIN_TRACED_PT),
        height: (point.1 - anchor_y).abs().max(MIN_TRACED_PT),
    }
}

fn opposite(corner: Corner) -> Corner {
    match corner {
        Corner::BottomLeft => Corner::TopRight,
        Corner::BottomRight => Corner::TopLeft,
        Corner::TopLeft => Corner::BottomRight,
        Corner::TopRight => Corner::BottomLeft,
    }
}

/// Applies a drag in progress to a copy of the annotation, for previewing and
/// for the value finally committed. One function, so what the user drags is
/// what lands in the `EditLog`.
pub(crate) fn dragged(annotation: &Annotation, drag: &AnnotationDrag) -> Option<Annotation> {
    let mut moved = annotation.clone();
    match drag.mode {
        AnnotationDragMode::Move => {
            let dx = drag.current.0 - drag.origin.0;
            let dy = drag.current.1 - drag.origin.1;
            pdf_annotate::move_annotation(&mut moved, dx, dy).ok()?;
        }
        AnnotationDragMode::Resize(corner) => {
            let rect = resized_rect(bounds(annotation)?, corner, drag.current);
            pdf_annotate::resize_annotation(&mut moved, rect).ok()?;
        }
    }
    Some(moved)
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

    #[test]
    fn rgba_conversion_rounds_rgb_and_ignores_alpha() {
        let converted = color_from_rgba(0.0, 0.5, 1.0, 0.05).expect("finite RGB converts");

        assert_eq!(
            converted,
            Color {
                r: 0,
                g: 128,
                b: 255
            }
        );
    }

    #[test]
    fn rgba_conversion_rejects_non_finite_channels() {
        assert_eq!(color_from_rgba(f64::NAN, 0.5, 1.0, 1.0), None);
    }

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
        annotation_at(
            placement,
            AnnotationId(1),
            PageId(0),
            committed_rect(placement),
        )
        .unwrap_or_else(|error| panic!("{:?} must build from a placement: {error}", placement.tool))
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

    /// A wide RGB PNG at 3:1 — deliberately *not* `CLICK_SIZE_PT`'s own 4:1
    /// ratio, which is the one shape where the old fixed rect happened to be
    /// right and so proves nothing.
    fn wide_png() -> Vec<u8> {
        use image::{DynamicImage, ImageFormat, RgbImage};
        use std::io::Cursor;

        let mut buf = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::from_pixel(300, 100, image::Rgb([10, 20, 30])))
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encoding a test png should succeed");
        buf.into_inner()
    }

    #[test]
    fn an_image_stamp_is_anchored_at_its_drop_point() {
        let rect = stamp_rect(&wide_png(), (200.0, 500.0)).expect("valid png");

        assert_eq!((rect.x, rect.y), (200.0, 500.0 - rect.height));
    }

    #[test]
    fn an_image_stamp_keeps_its_own_proportions_rather_than_the_click_size() {
        let rect = stamp_rect(&wide_png(), (200.0, 500.0)).expect("valid png");

        // 3:1 in, 3:1 out. The old behaviour squashed every image into
        // `CLICK_SIZE_PT`, which is a text-annotation size, not an image one.
        assert_eq!((rect.width, rect.height), (144.0, 48.0));
        assert_ne!((rect.width, rect.height), CLICK_SIZE_PT);
    }

    #[test]
    fn a_square_image_is_not_flattened_into_a_strip() {
        use image::{DynamicImage, ImageFormat, RgbImage};
        use std::io::Cursor;

        let mut buf = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 64, image::Rgb([10, 20, 30])))
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encoding a test png should succeed");

        let rect = stamp_rect(&buf.into_inner(), (200.0, 500.0)).expect("valid png");

        assert_eq!(rect.width, rect.height);
    }

    #[test]
    fn an_image_that_cannot_be_decoded_yields_no_rect() {
        assert!(stamp_rect(b"not an image", (200.0, 500.0)).is_err());
    }

    #[test]
    fn a_press_that_barely_moves_is_treated_as_a_click() {
        let barely = drag(Tool::Shape, (200.0, 500.0), (201.0, 500.5));
        let rect = rect_of(&built(&barely));

        assert_eq!((rect.width, rect.height), CLICK_SIZE_PT);
    }

    /// The bug this pair exists for: sweeping a highlight along a line of text
    /// is deliberately wide and shallow. Judging each axis on its own called
    /// that a click and swapped in a default-sized box mid-gesture, and moving
    /// up and down across the threshold made the preview flip back and forth.
    #[test]
    fn a_wide_shallow_sweep_is_a_drag_not_a_click() {
        let sweep = drag(Tool::Highlight, (100.0, 500.0), (400.0, 502.0));

        assert!(is_a_drag(&sweep));
        let rect = rect_of(&built(&sweep));
        assert_eq!(rect.x, 100.0);
        assert_eq!(rect.width, 300.0);
        assert_ne!((rect.width, rect.height), CLICK_SIZE_PT);
    }

    /// Every point of a sweep past the click threshold must keep tracing, so
    /// the preview never changes identity while the pointer is down.
    #[test]
    fn a_sweep_never_flips_back_to_a_click_as_it_grows() {
        for x in [110.0, 150.0, 200.0, 300.0, 400.0] {
            for y in [498.0, 500.0, 502.0, 505.0] {
                let sweep = drag(Tool::Highlight, (100.0, 500.0), (x, y));
                assert!(is_a_drag(&sweep), "sweep to ({x}, {y}) must stay a drag");
            }
        }
    }

    /// A rule is a line. Sweeping right with a shaky hand must still leave a
    /// straight one at the height the pointer went down.
    #[test]
    fn a_rule_swept_sideways_stays_on_the_press_point() {
        for tool in [Tool::Underline, Tool::Strikeout] {
            let shaky = drag(tool, (100.0, 500.0), (300.0, 537.0));
            let rect = rect_of(&built(&shaky));

            assert_eq!(rect.y, 500.0, "{tool:?} must not follow vertical drift");
            assert_eq!(rect.height, RULE_BAND_PT, "{tool:?} must stay a line");
            assert_eq!(rect.width, 200.0, "{tool:?} must follow the sweep");
        }
    }

    #[test]
    fn vertical_drift_changes_nothing_about_a_rule() {
        let straight = rect_of(&built(&drag(
            Tool::Underline,
            (100.0, 500.0),
            (300.0, 500.0),
        )));

        for drift in [-40.0, -5.0, 5.0, 40.0] {
            let wobbled = rect_of(&built(&drag(
                Tool::Underline,
                (100.0, 500.0),
                (300.0, 500.0 + drift),
            )));

            assert_eq!(
                (wobbled.x, wobbled.y, wobbled.width, wobbled.height),
                (straight.x, straight.y, straight.width, straight.height),
                "a drift of {drift} must not move the rule"
            );
        }
    }

    /// The rule treatment must not leak into the band-shaped tools.
    #[test]
    fn a_highlight_still_uses_both_axes() {
        let rect = rect_of(&built(&drag(
            Tool::Highlight,
            (100.0, 500.0),
            (300.0, 540.0),
        )));

        assert_eq!(rect.height, 40.0);
    }

    #[test]
    fn a_clicked_rule_lands_on_the_pointer_not_below_it() {
        let rect = rect_of(&built(&click(Tool::Underline, (200.0, 500.0))));

        assert_eq!(rect.y, 500.0);
        assert_eq!(rect.height, RULE_BAND_PT);
        assert_eq!(rect.width, CLICK_SIZE_PT.0);
    }

    #[test]
    fn a_flat_sweep_still_leaves_something_selectable() {
        let flat = drag(Tool::Highlight, (100.0, 500.0), (400.0, 500.0));
        let rect = traced_rect(&flat);

        assert_eq!(rect.width, 300.0);
        assert_eq!(rect.height, MIN_TRACED_PT);
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
        let tap = click(Tool::Ink, (10.0, 10.0));
        let error = annotation_at(&tap, AnnotationId(1), PageId(0), committed_rect(&tap))
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

    /// Exactly the PDF text-markup kinds, and nothing else: a Shape or a Stamp
    /// applied to a text selection would be a guess about what the user meant.
    #[test]
    fn only_the_text_markup_tools_apply_to_a_selection() {
        let marking: Vec<_> = Tool::ALL
            .iter()
            .filter(|tool| tool.marks_up_text())
            .copied()
            .collect();

        assert_eq!(
            marking,
            vec![Tool::Highlight, Tool::Underline, Tool::Strikeout]
        );
    }

    /// The two ways a markup annotation is born must share one builder.
    ///
    /// Their *geometry* legitimately differs — a text selection supplies the
    /// selected line's own rect, while a freehand rule pins its own height
    /// (see `a_rule_swept_sideways_stays_on_the_press_point`) — so this pins
    /// the thing that must not differ: given the same rect, both produce the
    /// same annotation.
    #[test]
    fn both_creation_paths_share_one_markup_builder() {
        for tool in Tool::ALL.iter().filter(|tool| tool.marks_up_text()) {
            let placement = drag(*tool, (10.0, 20.0), (110.0, 32.0));
            let rect = committed_rect(&placement);

            let from_drag = built(&placement);
            let from_selection = markup_annotation(*tool, AnnotationId(1), PageId(0), rect)
                .expect("a markup tool builds from a rect");

            assert_eq!(
                from_drag, from_selection,
                "{tool:?} must go through the same builder either way"
            );
        }
    }

    #[test]
    fn a_non_markup_tool_is_refused_rather_than_guessed_at() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };

        assert!(markup_annotation(Tool::Shape, AnnotationId(1), PageId(0), rect).is_err());
        assert!(markup_annotation(Tool::Ink, AnnotationId(1), PageId(0), rect).is_err());
    }

    #[test]
    fn a_text_rect_widens_into_the_model_rect_unchanged() {
        let converted = text_rect_to_pdf(TextRect {
            x_pt: 12.5,
            y_pt: 700.25,
            width_pt: 88.0,
            height_pt: 11.5,
        });

        assert_eq!(
            (converted.x, converted.y, converted.width, converted.height),
            (12.5, 700.25, 88.0, 11.5)
        );
    }

    #[test]
    fn the_placeholder_stamp_decodes_as_a_real_image() {
        let stamp = drag(Tool::Stamp, (10.0, 10.0), (100.0, 100.0));

        assert!(annotation_at(&stamp, AnnotationId(1), PageId(0), committed_rect(&stamp)).is_ok());
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

    fn a_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_press_inside_an_annotation_hits_it_and_one_outside_does_not() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);

        assert!(contains(rect, (150.0, 520.0)));
        assert!(contains(rect, (100.0, 500.0)), "edges count as inside");
        assert!(!contains(rect, (99.0, 520.0)));
        assert!(!contains(rect, (150.0, 541.0)));
    }

    #[test]
    fn each_corner_has_its_own_handle() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);
        let reach = 5.0;

        assert_eq!(
            corner_at(rect, (100.0, 500.0), reach),
            Some(Corner::BottomLeft)
        );
        assert_eq!(
            corner_at(rect, (300.0, 500.0), reach),
            Some(Corner::BottomRight)
        );
        assert_eq!(
            corner_at(rect, (100.0, 540.0), reach),
            Some(Corner::TopLeft)
        );
        assert_eq!(
            corner_at(rect, (300.0, 540.0), reach),
            Some(Corner::TopRight)
        );
        assert_eq!(
            corner_at(rect, (200.0, 520.0), reach),
            None,
            "the body is not a handle"
        );
    }

    /// Pulling a corner must hold the opposite one still — otherwise the
    /// annotation slides while it resizes and the user chases it.
    #[test]
    fn a_resize_holds_the_opposite_corner_still() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);

        let grown = resized_rect(rect, Corner::TopRight, (400.0, 600.0));

        assert_eq!((grown.x, grown.y), (100.0, 500.0), "anchor must not move");
        assert_eq!((grown.width, grown.height), (300.0, 100.0));
    }

    #[test]
    fn dragging_a_corner_past_its_opposite_flips_rather_than_inverts() {
        let rect = a_rect(100.0, 500.0, 200.0, 40.0);

        // TopRight dragged below-left of the anchored BottomLeft corner.
        let flipped = resized_rect(rect, Corner::TopRight, (40.0, 460.0));

        assert!(flipped.width > 0.0 && flipped.height > 0.0);
        assert_eq!((flipped.x, flipped.y), (40.0, 460.0));
    }

    #[test]
    fn a_move_drag_shifts_the_annotation_by_the_pointer_delta() {
        let annotation = built(&drag(Tool::Shape, (100.0, 500.0), (200.0, 560.0)));
        let moved = dragged(
            &annotation,
            &AnnotationDrag {
                id: annotation.id,
                mode: AnnotationDragMode::Move,
                origin: (150.0, 520.0),
                current: (170.0, 495.0),
            },
        )
        .expect("a shape can be moved");

        let before = bounds(&annotation).expect("shape has bounds");
        let after = bounds(&moved).expect("shape has bounds");

        assert_eq!((after.x - before.x, after.y - before.y), (20.0, -25.0));
        assert_eq!((after.width, after.height), (before.width, before.height));
    }

    /// Ink has no rect to reshape, so it reports bounds for hit-testing and
    /// moving but must refuse a resize rather than squash its polyline.
    #[test]
    fn ink_can_be_moved_but_not_resized() {
        let ink = built(&drag(Tool::Ink, (10.0, 10.0), (60.0, 40.0)));
        let handle = AnnotationDrag {
            id: ink.id,
            mode: AnnotationDragMode::Resize(Corner::TopRight),
            origin: (60.0, 40.0),
            current: (90.0, 70.0),
        };

        assert!(bounds(&ink).is_some(), "ink still reports bounds");
        assert!(dragged(&ink, &handle).is_none(), "ink refuses a resize");
        assert!(dragged(
            &ink,
            &AnnotationDrag {
                mode: AnnotationDragMode::Move,
                ..handle
            }
        )
        .is_some());
    }

    #[test]
    fn ink_bounds_cover_every_point_of_the_stroke() {
        let mut placement = drag(Tool::Ink, (10.0, 10.0), (60.0, 40.0));
        placement.points = vec![(10.0, 30.0), (35.0, 10.0), (60.0, 40.0)];
        let ink = built(&placement);

        let rect = bounds(&ink).expect("ink has bounds");

        assert_eq!((rect.x, rect.y), (10.0, 10.0));
        assert_eq!((rect.width, rect.height), (50.0, 30.0));
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
