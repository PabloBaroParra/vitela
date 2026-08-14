//! The annotation toolbar: the controls themselves, the keyboard shortcuts
//! that stand in for them, arming a creation tool, and the single place that
//! decides when each control is sensitive.

use gtk::prelude::*;
use gtk::{gio, Box as GtkBox, Button, Orientation, PolicyType, ScrolledWindow, ToggleButton};
use pdf_document::{AnnotationId, Command, PageId, Rect};
use pdf_render::TextRect;

use crate::app::selection;
use crate::app::state::{AnnotationToolbar, Tool, Viewer};

use super::builder::markup_annotation;
use super::command::{apply_command, command, model};
use super::edit::{delete, edit_move, edit_resize, select_previous, supports_resize};
use super::style::{choose_restyle_color, supports_restyle};

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
    if active {
        // Mutual exclusion with content-edit mode, the other direction from
        // `content_edit::set_mode`'s call to `disarm` — one mode claims a
        // page click at a time.
        crate::app::content_edit::set_mode(viewer, false);
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
///
/// `pub(crate)`, not `pub(super)`: `content_edit::set_mode` also calls this,
/// to keep an armed creation tool and content-edit mode mutually exclusive.
pub(crate) fn disarm(viewer: &Viewer) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
