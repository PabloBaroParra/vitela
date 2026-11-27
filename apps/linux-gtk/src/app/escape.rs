//! Escape: the one place that decides what the key lets go of.
//!
//! Several things could answer Escape at once — an open popover, an inline
//! editor, an armed tool, a selection, a toast. Each feature growing its own
//! key controller would make the winner an accident of propagation order, so
//! there is one window action, `win.escape`, and one precedence
//! ([`target`]):
//!
//! 1. **Whatever already owns the key keeps it.** The command palette, a
//!    text field (the inline content editor, the search entry, a form or
//!    metadata field) and an open popover each close or cancel on Escape
//!    themselves. Dialogs are windows of their own and never see this action.
//! 2. **An armed tool** is released: an annotation tool, a waiting drawn
//!    signature, a form-field kind, a content insert kind (`arming`).
//! 3. **A content selection** is cleared.
//! 4. **A toast** on screen is dismissed.
//!
//! One press does one thing; a second press reaches the next.
//!
//! ## How the first rule holds
//!
//! An application accelerator runs in the window's capture phase, ahead of
//! the widget that has the focus — but only while its action is enabled. A
//! disabled action does not consume its key. So the action is enabled
//! exactly while [`target`] has an answer, and that answer is `None`
//! whenever something in rule 1 holds the key. [`update`] recomputes it from
//! state at every change that can move it, the focus included.

use gtk::prelude::*;
use gtk::{gio, glib};

use super::arming::{self, Armed};
use super::state::Viewer;

/// The window action Escape is bound to.
pub(crate) const ACTION: &str = "escape";

/// What Escape acts on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Target {
    Armed(Armed),
    Selection,
    Toast,
}

/// Everything the precedence reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Context {
    /// The command palette is open.
    pub(crate) palette_open: bool,
    /// The focus is in a text field of the main window.
    pub(crate) text_input_focused: bool,
    /// The focus is inside a popover.
    pub(crate) popover_focused: bool,
    pub(crate) armed: Option<Armed>,
    /// A content-edit selection, or a selected text block.
    pub(crate) selection: bool,
    pub(crate) toast: bool,
}

/// What one press of Escape does in `context`, or `None` when the key is
/// not this action's to take.
pub(crate) fn target(context: Context) -> Option<Target> {
    if context.palette_open || context.text_input_focused || context.popover_focused {
        return None;
    }
    context
        .armed
        .map(Target::Armed)
        .or(context.selection.then_some(Target::Selection))
        .or(context.toast.then_some(Target::Toast))
}

fn focus_in_popover(viewer: &Viewer) -> bool {
    viewer
        .window()
        .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window))
        .is_some_and(|focused| focused.ancestor(gtk::Popover::static_type()).is_some())
}

/// Reads the context, or `None` while the state is mutably borrowed.
fn context(viewer: &Viewer) -> Option<Context> {
    let (palette_open, armed) = {
        let state = viewer.state.try_borrow().ok()?;
        (state.palette_open, arming::armed(&state))
    };
    Some(Context {
        palette_open,
        text_input_focused: super::annotations::text_input_has_focus(viewer),
        popover_focused: focus_in_popover(viewer),
        armed,
        selection: super::content_edit::multi::has_selection(viewer)
            || super::content_edit::block::has_selection(viewer),
        toast: viewer.feedback.toasts.is_showing(),
    })
}

fn action(viewer: &Viewer) -> Option<gio::SimpleAction> {
    viewer
        .window()
        .and_then(|window| window.lookup_action(ACTION))
        .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
}

/// Enables the action exactly while Escape has something of its own to do.
///
/// Safe to call from anywhere: when the state is mutably borrowed — a focus
/// change landing in the middle of a handler — the answer is recomputed on
/// the next idle instead.
pub(crate) fn update(viewer: &Viewer) {
    let Some(context) = context(viewer) else {
        glib::idle_add_local_once({
            let viewer = viewer.clone();
            move || update(&viewer)
        });
        return;
    };
    if let Some(action) = action(viewer) {
        action.set_enabled(target(context).is_some());
    }
}

/// Does what one press of Escape does right now.
fn run(viewer: &Viewer) {
    let Some(target) = context(viewer).and_then(target) else {
        return;
    };
    match target {
        Target::Armed(armed) => arming::disarm(viewer, armed),
        Target::Selection => super::content_edit::multi::clear_all(viewer),
        Target::Toast => viewer.feedback.toasts.dismiss(),
    }
    update(viewer);
}

/// Installs `win.escape`, binds the key to it, and keeps it in step with
/// the focus and the toast — the two inputs no feature setter reports.
pub(crate) fn connect(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    let action = gio::SimpleAction::new(ACTION, None);
    action.set_enabled(false);
    action.connect_activate({
        let viewer = viewer.clone();
        move |_, _| run(&viewer)
    });
    window.add_action(&action);
    application.set_accels_for_action(&format!("win.{ACTION}"), &["Escape"]);

    window.connect_focus_widget_notify({
        let viewer = viewer.clone();
        move |_| update(&viewer)
    });
    viewer.feedback.toasts.connect_changed({
        let viewer = viewer.clone();
        move || update(&viewer)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Tool;

    const INK: Armed = Armed::Tool(Tool::Ink);

    fn everything() -> Context {
        Context {
            armed: Some(INK),
            selection: true,
            toast: true,
            ..Context::default()
        }
    }

    #[test]
    fn with_nothing_to_let_go_of_the_key_is_not_taken() {
        assert_eq!(target(Context::default()), None);
    }

    #[test]
    fn an_armed_tool_goes_before_a_selection_and_a_selection_before_a_toast() {
        assert_eq!(target(everything()), Some(Target::Armed(INK)));
        assert_eq!(
            target(Context {
                armed: None,
                ..everything()
            }),
            Some(Target::Selection)
        );
        assert_eq!(
            target(Context {
                armed: None,
                selection: false,
                ..everything()
            }),
            Some(Target::Toast)
        );
    }

    #[test]
    fn the_palette_a_text_field_and_a_popover_each_keep_the_key() {
        for context in [
            Context {
                palette_open: true,
                ..everything()
            },
            Context {
                text_input_focused: true,
                ..everything()
            },
            Context {
                popover_focused: true,
                ..everything()
            },
        ] {
            assert_eq!(target(context), None, "{context:?}");
        }
    }

    // Over a real window from here on.

    use pdf_render::PageRotation;

    use crate::app::content_edit::test_support::{Fixture, OpenRetype};
    use crate::app::i18n::{tr, tr_args};
    use crate::app::state::{ContentInsertKind, ContentSelection, FieldKind};
    use crate::app::test_support::pump;

    fn escape(viewer: &Viewer) -> gio::SimpleAction {
        action(viewer).expect("build_ui installs win.escape")
    }

    fn cursor_on(viewer: &Viewer, page_index: usize) -> Option<String> {
        viewer
            .state
            .borrow()
            .session
            .as_ref()
            .expect("a session")
            .pages[page_index]
            .highlights
            .cursor()
            .and_then(|cursor| cursor.name())
            .map(String::from)
    }

    fn tool_button(viewer: &Viewer, tool: Tool) -> gtk::ToggleButton {
        viewer
            .annotation_buttons
            .create
            .iter()
            .find(|(candidate, _)| *candidate == tool)
            .map(|(_, button)| button.clone())
            .expect("every tool has a button")
    }

    #[gtk::test]
    fn gtk_ui_escape_releases_an_armed_annotation_tool_and_its_cursor() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let ink = tool_button(viewer, Tool::Ink);
        assert!(!escape(viewer).is_enabled(), "nothing to let go of yet");

        ink.set_active(true);
        assert_eq!(cursor_on(viewer, 0).as_deref(), Some(arming::PLACE));
        assert!(escape(viewer).is_enabled());

        escape(viewer).activate(None);

        assert_eq!(viewer.state.borrow().active_tool, None);
        assert!(!ink.is_active(), "the button is released with the tool");
        assert_eq!(cursor_on(viewer, 0), None, "back to the plain pointer");
        assert!(!escape(viewer).is_enabled());
        assert_eq!(
            viewer.last_said(),
            tr_args(
                "annotate-tool-disarmed",
                &[("tool", Tool::Ink.label().into())]
            )
        );
    }

    #[gtk::test]
    fn gtk_ui_the_text_box_tool_shows_the_text_cursor() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();

        tool_button(viewer, Tool::FreeText).set_active(true);
        assert_eq!(cursor_on(viewer, 0).as_deref(), Some(arming::TYPE));

        // Switching tools swaps the cursor rather than stacking a second.
        tool_button(viewer, Tool::Shape).set_active(true);
        assert_eq!(cursor_on(viewer, 0).as_deref(), Some(arming::PLACE));
    }

    /// Escape leaves the insert kind, not the mode it lives in.
    #[gtk::test]
    fn gtk_ui_escape_releases_an_armed_insert_kind_and_stays_in_edit_mode() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();

        crate::app::content_edit::set_insert_mode(viewer, Some(ContentInsertKind::Text));
        assert_eq!(cursor_on(viewer, 0).as_deref(), Some(arming::TYPE));
        crate::app::content_edit::set_insert_mode(viewer, Some(ContentInsertKind::Image));
        assert_eq!(cursor_on(viewer, 0).as_deref(), Some(arming::PLACE));

        escape(viewer).activate(None);

        let state = viewer.state.borrow();
        assert_eq!(state.content_insert_mode, None);
        assert!(state.content_edit_mode, "the mode itself is not a tool");
        drop(state);
        assert!(!viewer.insert_image_button.is_active());
        assert_eq!(cursor_on(viewer, 0), None);
        assert!(!escape(viewer).is_enabled());
    }

    #[gtk::test]
    fn gtk_ui_escape_releases_an_armed_form_field_kind_and_stays_in_forms_mode() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();

        crate::app::forms::set_field_kind(viewer, Some(FieldKind::Checkbox));
        assert_eq!(cursor_on(viewer, 0).as_deref(), Some(arming::PLACE));
        assert!(escape(viewer).is_enabled());

        escape(viewer).activate(None);

        let state = viewer.state.borrow();
        assert_eq!(state.form_field_kind, None);
        assert!(state.form_edit_mode, "the mode itself is not a tool");
        drop(state);
        assert_eq!(cursor_on(viewer, 0), None);
        assert!(!escape(viewer).is_enabled());
    }

    /// One press, one thing: what is armed first, then the selection, then
    /// the toast — which outlives both.
    #[gtk::test]
    fn gtk_ui_each_escape_lets_go_of_one_thing_in_order() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let selected = |viewer: &Viewer| crate::app::content_edit::multi::has_selection(viewer);
        tool_button(viewer, Tool::Ink).set_active(true);
        viewer.fail("Could not save.");

        escape(viewer).activate(None);
        assert_eq!(viewer.state.borrow().active_tool, None);
        assert!(
            viewer.feedback.toasts.is_showing(),
            "the toast outlives the first press"
        );

        // Arming the tool left content-edit mode; a selection needs it back.
        crate::app::content_edit::set_mode(viewer, true);
        if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
            session.content_selection = Some(ContentSelection {
                page_index: 0,
                members: Default::default(),
            });
        }
        crate::app::update_content_edit_controls(viewer);
        assert!(selected(viewer));

        escape(viewer).activate(None);
        assert!(!selected(viewer));
        assert_eq!(viewer.status.text(), tr("edit-multi-cleared"));
        assert!(viewer.feedback.toasts.is_showing(), "and the second");

        escape(viewer).activate(None);
        assert!(!viewer.feedback.toasts.is_showing());
        assert!(!escape(viewer).is_enabled(), "nothing is left");
    }

    /// The inline editor cancels on Escape itself. Its field has the focus
    /// for as long as it is open, and that alone keeps this action off.
    #[gtk::test]
    fn gtk_ui_escape_stays_with_an_open_inline_editor() {
        let retype = OpenRetype::new();
        let viewer = retype.viewer();
        pump();
        assert!(
            crate::app::annotations::text_input_has_focus(viewer),
            "the retype box holds the focus"
        );

        viewer.fail("Could not save.");
        pump();

        assert!(viewer.feedback.toasts.is_showing());
        assert!(
            !escape(viewer).is_enabled(),
            "the key must reach the editor, whatever else is waiting"
        );
    }

    /// A tool outlives the document it was armed on, so pages built or
    /// brought back afterwards must be told.
    #[gtk::test]
    fn gtk_ui_a_page_built_after_arming_gets_the_cursor_too() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        tool_button(viewer, Tool::Stamp).set_active(true);

        let overlay = gtk::Overlay::new();
        viewer.pages.append(&overlay);
        if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
            session
                .pages
                .push(crate::app::test_fixtures::page_slot(overlay));
            session.page_heights.push(842);
        }
        assert_eq!(cursor_on(viewer, 1), None, "a fresh layer has none");

        crate::app::update_content_edit_controls(viewer);

        assert_eq!(cursor_on(viewer, 1).as_deref(), Some(arming::PLACE));
    }
}
