//! Escape: the one place that decides what the key lets go of.
//!
//! Several things could answer Escape at once — an open popover, an inline
//! editor, an armed tool, a selection, a toast. Each feature growing its own
//! key controller would make the winner an accident of propagation order, so
//! there is one window action, `win.escape`, and one precedence
//! ([`target`]):
//!
//! 1. **Whatever already uses the key keeps it.** The command palette, the
//!    inline content editor, an editable label being edited (a form field's
//!    name in the fill panel) and an open popover — the Find entry lives in
//!    one — each close or cancel on Escape themselves. Dialogs are windows
//!    of their own and never see this action.
//! 2. **An armed tool** is released: an annotation tool, a waiting drawn
//!    signature, a form-field kind, a content insert kind (`arming`).
//! 3. **A content selection** is cleared.
//! 4. **A toast** on screen is dismissed.
//!
//! One press does one thing; a second press reaches the next.
//!
//! ## A text field that has no use for the key
//!
//! Rule 1 is about who *uses* Escape, not about who has the cursor. A spin
//! button or a plain entry in a side panel does nothing with the key, and it
//! keeps the focus while the user works on the page with the mouse: a click,
//! a marquee or arming a tool moves no focus. Leaving Escape with such a
//! field made the key dead exactly when there was something to let go of.
//!
//! So with the cursor in an idle field ([`Focus::IdleField`]) rules 2 and 3
//! still apply. Rule 4 does not: a toast is not something the user is in
//! the middle of, and a key pressed while typing in a field — to give up an
//! input-method composition, say — must not be spent on dismissing one.
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

/// Where the keyboard focus is, as far as Escape is concerned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Focus {
    /// Not in a text field: on a button, on the canvas, nowhere.
    #[default]
    Elsewhere,
    /// In a text field that does nothing with Escape — a spin button, a
    /// form or metadata entry.
    IdleField,
    /// In the inline content editor, which cancels on Escape itself.
    Editor,
    /// In an editable label that is being edited — the form field's name in
    /// the fill panel, the only one the shell has. It cancels the rename on
    /// Escape itself; without the key the rename would be committed when the
    /// focus leaves instead.
    Renaming,
}

/// Which [`Focus`] a focused widget gives: `text_input` when it is a text
/// field at all, `in_content_editor` when it is inside the inline content
/// editor, and `in_editing_label` when it is, or is inside, an editable
/// label in editing state.
pub(crate) fn classify(text_input: bool, in_content_editor: bool, in_editing_label: bool) -> Focus {
    if !text_input {
        Focus::Elsewhere
    } else if in_content_editor {
        Focus::Editor
    } else if in_editing_label {
        Focus::Renaming
    } else {
        Focus::IdleField
    }
}

/// Everything the precedence reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Context {
    /// The command palette is open.
    pub(crate) palette_open: bool,
    pub(crate) focus: Focus,
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
    let field_cancels = matches!(context.focus, Focus::Editor | Focus::Renaming);
    if context.palette_open || context.popover_focused || field_cancels {
        return None;
    }
    let toast = context.toast && context.focus != Focus::IdleField;
    context
        .armed
        .map(Target::Armed)
        .or(context.selection.then_some(Target::Selection))
        .or(toast.then_some(Target::Toast))
}

fn focused(viewer: &Viewer) -> Option<gtk::Widget> {
    viewer
        .window()
        .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window))
}

fn focus_in_popover(viewer: &Viewer) -> bool {
    focused(viewer).is_some_and(|focused| focused.ancestor(gtk::Popover::static_type()).is_some())
}

/// Which [`Focus`] the window is in. `editor` is the frame of the inline
/// content editor, while one is open.
fn focus(viewer: &Viewer, editor: Option<&gtk::Widget>) -> Focus {
    let focused = focused(viewer);
    let in_editor = focused.as_ref().is_some_and(|focused| {
        editor.is_some_and(|editor| focused == editor || focused.is_ancestor(editor))
    });
    classify(
        super::annotations::text_input_has_focus(viewer),
        in_editor,
        focused.as_ref().is_some_and(in_editing_label),
    )
}

/// Whether `widget` is, or is inside, an editable label in editing state —
/// while editing, the focus is on the label's own text field.
/// (`ancestor` counts the widget itself.)
fn in_editing_label(widget: &gtk::Widget) -> bool {
    widget
        .ancestor(gtk::EditableLabel::static_type())
        .and_then(|label| label.downcast::<gtk::EditableLabel>().ok())
        .is_some_and(|label| label.is_editing())
}

/// Reads the context, or `None` while the state is mutably borrowed.
fn context(viewer: &Viewer) -> Option<Context> {
    let (palette_open, armed, editor) = {
        let state = viewer.state.try_borrow().ok()?;
        let editor = state
            .session
            .as_ref()
            .and_then(|session| session.content_editor.as_ref())
            .map(|editor| editor.frame.clone().upcast::<gtk::Widget>());
        (state.palette_open, arming::armed(&state), editor)
    };
    Some(Context {
        palette_open,
        focus: focus(viewer, editor.as_ref()),
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
    super::shortcuts::ESCAPE.install(application);

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
    fn the_palette_the_inline_editor_and_a_popover_each_keep_the_key() {
        for context in [
            Context {
                palette_open: true,
                ..everything()
            },
            Context {
                focus: Focus::Editor,
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

    /// A spin button or an entry does nothing with Escape, and keeps the
    /// cursor while the user arms a tool or drags a marquee with the mouse.
    #[test]
    fn an_idle_text_field_does_not_keep_the_key_from_a_tool_or_a_selection() {
        let idle = Context {
            focus: Focus::IdleField,
            ..everything()
        };

        assert_eq!(target(idle), Some(Target::Armed(INK)));
        assert_eq!(
            target(Context {
                armed: None,
                ..idle
            }),
            Some(Target::Selection)
        );
    }

    /// A field name being renamed in an editable label cancels on Escape
    /// itself. Taking the key from it would commit the rename on focus
    /// leave instead.
    #[test]
    fn an_editable_label_being_edited_keeps_the_key() {
        assert_eq!(
            target(Context {
                focus: Focus::Renaming,
                ..everything()
            }),
            None
        );
    }

    #[test]
    fn the_focus_is_classified_by_who_uses_the_key() {
        assert_eq!(classify(false, false, false), Focus::Elsewhere);
        assert_eq!(classify(false, true, true), Focus::Elsewhere);
        assert_eq!(classify(true, false, false), Focus::IdleField);
        assert_eq!(classify(true, true, false), Focus::Editor);
        assert_eq!(classify(true, false, true), Focus::Renaming);
    }

    /// A toast is not something the user is in the middle of: with the
    /// cursor in a field and nothing armed or selected, the key is the
    /// field's.
    #[test]
    fn a_toast_alone_does_not_take_the_key_from_a_text_field() {
        assert_eq!(
            target(Context {
                focus: Focus::IdleField,
                toast: true,
                ..Context::default()
            }),
            None
        );
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

    /// The observed defect: a marquee selection made while the cursor sat
    /// in a spin button could not be cleared with Escape, because nothing
    /// about selecting moves the focus off the field.
    #[gtk::test]
    fn gtk_ui_escape_clears_a_selection_while_an_idle_field_has_the_cursor() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let selected = |viewer: &Viewer| crate::app::content_edit::multi::has_selection(viewer);
        let spin = gtk::SpinButton::with_range(4.0, 400.0, 1.0);
        viewer.pages.append(&spin);
        pump();
        assert!(spin.grab_focus(), "the test window takes focus");
        pump();
        assert!(crate::app::annotations::text_input_has_focus(viewer));
        assert!(!escape(viewer).is_enabled(), "nothing to let go of yet");

        crate::app::content_edit::set_mode(viewer, true);
        if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
            session.content_selection = Some(ContentSelection {
                page_index: 0,
                members: Default::default(),
            });
        }
        crate::app::update_content_edit_controls(viewer);
        assert!(selected(viewer));
        assert!(escape(viewer).is_enabled(), "the selection is Escape's");

        escape(viewer).activate(None);

        assert!(!selected(viewer));
        assert!(crate::app::annotations::text_input_has_focus(viewer));
        assert!(!escape(viewer).is_enabled());
    }

    /// The same field with a tool armed: Escape releases the tool. With
    /// only a toast up, the key stays with the field.
    #[gtk::test]
    fn gtk_ui_escape_releases_a_tool_but_not_a_toast_from_an_idle_field() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let entry = gtk::Entry::new();
        viewer.pages.append(&entry);
        pump();
        assert!(entry.grab_focus(), "the test window takes focus");
        pump();

        viewer.fail("Could not save.");
        pump();
        assert!(
            !escape(viewer).is_enabled(),
            "a toast alone leaves the key with the field"
        );

        tool_button(viewer, Tool::Ink).set_active(true);
        assert!(escape(viewer).is_enabled());
        escape(viewer).activate(None);

        assert_eq!(viewer.state.borrow().active_tool, None);
        assert!(viewer.feedback.toasts.is_showing(), "the toast is left");
        assert!(!escape(viewer).is_enabled());
    }

    /// The fill panel's field name is an editable label: double-clicked, it
    /// is a text field that cancels the rename on Escape. With a tool armed
    /// the key must still reach it.
    #[gtk::test]
    fn gtk_ui_escape_stays_with_an_editable_label_being_edited() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let label = gtk::EditableLabel::new("text_1");
        viewer.pages.append(&label);
        pump();
        tool_button(viewer, Tool::Ink).set_active(true);
        assert!(escape(viewer).is_enabled(), "the tool is Escape's");

        label.start_editing();
        pump();
        assert!(label.is_editing());
        assert!(crate::app::annotations::text_input_has_focus(viewer));

        assert!(
            !escape(viewer).is_enabled(),
            "the key must reach the label, which cancels the rename"
        );
        label.stop_editing(false);
        pump();
        assert!(
            escape(viewer).is_enabled(),
            "and comes back once it is done"
        );
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
