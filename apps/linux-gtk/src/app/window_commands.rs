//! The window's view commands as actions with keys: zoom in, zoom out,
//! actual size, the two fit modes, the two side panels, full screen, quit.
//!
//! Each one was already a control — a toolbar button, a toggle, the window's
//! close button — and each action walks through that control's own handler,
//! so the key, the button and the command palette cannot come to do
//! different things. Nothing here decides a zoom step or a limit: that is
//! `step_zoom`'s ladder, the one the toolbar and Ctrl+wheel use.

use gtk::prelude::*;
use gtk::{gio, ToggleButton};

use super::home::EDITOR_PAGE;
use super::layout::{set_zoom, Zoom};
use super::shortcuts::{self, Shortcut};
use super::state::Viewer;

/// What Ctrl+0 zooms to: the page at its real size.
///
/// 100% rather than the fit-width a document opens at. It is what the key
/// means in browsers and in other viewers, and fit-width has a key of its
/// own beside it (Ctrl+1) — two keys for one mode would leave no key for
/// the size a print-out has.
pub(crate) const ACTUAL_SIZE: Zoom = Zoom::Custom(1.0);

fn add(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    shortcut: Shortcut,
    run: impl Fn() + 'static,
) {
    let action = gio::SimpleAction::new(shortcut.action, None);
    action.connect_activate(move |_, _| run());
    window.add_action(&action);
    shortcut.install(application);
}

/// Flips a side panel's toggle, which is what folds or unfolds the column
/// (`side_panel::connect`). Only on the editor: the panels are part of that
/// screen, and a key pressed on Home must not change a layout the user
/// cannot see.
fn toggle_panel(viewer: &Viewer, toggle: &ToggleButton) {
    if viewer.view_stack.visible_child_name().as_deref() == Some(EDITOR_PAGE) {
        toggle.set_active(!toggle.is_active());
    }
}

fn toggle_fullscreen(window: &gtk::ApplicationWindow) {
    if window.is_fullscreen() {
        window.unfullscreen();
    } else {
        window.fullscreen();
    }
}

/// Installs the actions on `window` and binds their keys. `show_pages` and
/// `show_tools` are the toolbar's two panel toggles.
pub(super) fn connect(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
    show_pages: &ToggleButton,
    show_tools: &ToggleButton,
) {
    for (shortcut, increase) in [(shortcuts::ZOOM_IN, true), (shortcuts::ZOOM_OUT, false)] {
        add(application, window, shortcut, {
            let viewer = viewer.clone();
            move || super::step_zoom(&viewer, increase)
        });
    }
    for (shortcut, zoom) in [
        (shortcuts::ZOOM_RESET, ACTUAL_SIZE),
        (shortcuts::FIT_WIDTH, Zoom::FitWidth),
        (shortcuts::FIT_PAGE, Zoom::FitPage),
    ] {
        add(application, window, shortcut, {
            let viewer = viewer.clone();
            move || set_zoom(&viewer, zoom)
        });
    }
    for (shortcut, toggle) in [
        (shortcuts::TOGGLE_PAGES, show_pages),
        (shortcuts::TOGGLE_TOOLS, show_tools),
    ] {
        add(application, window, shortcut, {
            let viewer = viewer.clone();
            let toggle = toggle.clone();
            move || toggle_panel(&viewer, &toggle)
        });
    }
    add(application, window, shortcuts::FULLSCREEN, {
        let window = window.clone();
        move || toggle_fullscreen(&window)
    });
    // The window's own close: the same unsaved-changes question, the same
    // refusal while a document is still being written
    // (`document::confirm_closing_edits`).
    add(application, window, shortcuts::QUIT, {
        let window = window.clone();
        move || window.close()
    });
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::app::home::{show_editor, show_home};
    use crate::app::test_fixtures;
    use crate::app::test_support::FileDocument;
    use crate::app::ui_tests::built_ui;
    use crate::app::BuiltUi;

    fn run(built: &BuiltUi, shortcut: Shortcut) {
        gtk::prelude::ActionGroupExt::activate_action(&built.window, shortcut.action, None);
    }

    /// A window over a one-page model session, as `wheel_zoom`'s tests
    /// build one: enough for the zoom path, with no pdfium behind it.
    fn with_a_page() -> BuiltUi {
        let built = built_ui();
        let mut session = test_fixtures::model_session(pdf_document::Document::blank());
        session
            .pages
            .push(test_fixtures::page_slot(gtk::Overlay::new()));
        session.page_heights.push(842);
        built.viewer.state.borrow_mut().session = Some(session);
        built
    }

    fn zoom(built: &BuiltUi) -> Zoom {
        built
            .viewer
            .state
            .borrow()
            .session
            .as_ref()
            .expect("a session")
            .zoom
    }

    fn close(built: BuiltUi) {
        built.viewer.state.borrow_mut().session = None;
        built.window.close();
    }

    /// The keys take the toolbar's steps and stop at its limits.
    #[gtk::test]
    fn gtk_ui_the_zoom_actions_step_the_toolbar_s_ladder() {
        let built = with_a_page();
        for (start, shortcut, expected) in [
            (1.0, shortcuts::ZOOM_IN, 1.25),
            (1.0, shortcuts::ZOOM_OUT, 0.75),
            (8.0, shortcuts::ZOOM_IN, 8.0),
            (0.10, shortcuts::ZOOM_OUT, 0.10),
        ] {
            set_zoom(&built.viewer, Zoom::Custom(start));

            run(&built, shortcut);

            assert!(
                zoom(&built) == Zoom::Custom(expected),
                "{start} {shortcut:?}"
            );
            assert_eq!(
                built.viewer.zoom_label.text(),
                format!("{:.0}%", expected * 100.0)
            );
        }
        close(built);
    }

    #[gtk::test]
    fn gtk_ui_control_zero_is_actual_size_and_the_fit_keys_fit() {
        let built = with_a_page();
        set_zoom(&built.viewer, Zoom::Custom(3.0));

        run(&built, shortcuts::ZOOM_RESET);
        assert!(zoom(&built) == Zoom::Custom(1.0));
        assert_eq!(built.viewer.zoom_label.text(), "100%");

        run(&built, shortcuts::FIT_PAGE);
        assert!(zoom(&built) == Zoom::FitPage);
        run(&built, shortcuts::FIT_WIDTH);
        assert!(zoom(&built) == Zoom::FitWidth);
        close(built);
    }

    #[gtk::test]
    fn gtk_ui_the_zoom_actions_do_nothing_without_a_document() {
        let built = built_ui();

        for shortcut in [
            shortcuts::ZOOM_IN,
            shortcuts::ZOOM_OUT,
            shortcuts::ZOOM_RESET,
            shortcuts::FIT_WIDTH,
            shortcuts::FIT_PAGE,
        ] {
            run(&built, shortcut);
        }

        assert_eq!(built.viewer.zoom_label.text(), "100%");
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_the_panel_keys_flip_the_toolbar_toggles_on_the_editor_only() {
        let built = built_ui();
        let pages = &built.chrome.show_pages;
        let tools = &built.chrome.show_tools;
        assert!(pages.is_active() && tools.is_active());

        show_home(&built.viewer);
        run(&built, shortcuts::TOGGLE_PAGES);
        run(&built, shortcuts::TOGGLE_TOOLS);
        assert!(
            pages.is_active() && tools.is_active(),
            "nothing changes behind another screen"
        );

        show_editor(&built.viewer);
        run(&built, shortcuts::TOGGLE_PAGES);
        assert!(!pages.is_active() && tools.is_active());
        run(&built, shortcuts::TOGGLE_TOOLS);
        assert!(!pages.is_active() && !tools.is_active());
        run(&built, shortcuts::TOGGLE_PAGES);
        run(&built, shortcuts::TOGGLE_TOOLS);
        assert!(pages.is_active() && tools.is_active());
        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_f11_enters_full_screen_and_leaves_it_again() {
        let built = built_ui();
        assert!(!built.window.is_fullscreen());

        run(&built, shortcuts::FULLSCREEN);
        assert!(built.window.is_fullscreen());
        run(&built, shortcuts::FULLSCREEN);
        assert!(!built.window.is_fullscreen());
        built.window.close();
    }

    /// Quit is the window's close, not a way around it.
    #[gtk::test]
    fn gtk_ui_quit_asks_the_window_to_close() {
        let built = built_ui();
        // On screen: closing a window that was never shown does nothing.
        built.window.present();
        let asked = Rc::new(Cell::new(0));
        built.window.connect_close_request({
            let asked = asked.clone();
            move |_| {
                asked.set(asked.get() + 1);
                gtk::glib::Propagation::Proceed
            }
        });

        run(&built, shortcuts::QUIT);

        assert_eq!(asked.get(), 1);
    }

    /// With unsaved work the same question comes up as for the close
    /// button, and the window stays until it is answered.
    #[gtk::test]
    fn gtk_ui_quit_with_unsaved_changes_asks_before_closing() {
        let document = FileDocument::new("quit-dirty");
        document.open_file();
        document.mark_dirty();

        run(&document.built, shortcuts::QUIT);

        assert!(
            crate::app::test_support::wait_until(|| document.dialogs().len() == 1),
            "the unsaved-changes question is up"
        );
        assert!(document.dirty(), "nothing was discarded");
        assert_eq!(document.tab_count(), 1, "and nothing was closed");
    }
}
