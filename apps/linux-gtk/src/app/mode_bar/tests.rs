use gtk::{Button, Label, Orientation};

use super::*;
use crate::app::i18n::{speak, Language};
use crate::app::test_fixtures::model_session;
use crate::app::tools_panel::ANNOTATE_PAGE;
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;

/// A mode button's visible text: its child is an icon and a label in a box.
fn caption(button: &Button) -> String {
    let content = button.child().expect("a mode button has content");
    std::iter::successors(content.first_child(), |child| child.next_sibling())
        .find_map(|child| child.downcast::<Label>().ok())
        .map(|label| label.text().to_string())
        .expect("a mode button has a label")
}

/// Gives the window a document without touching pdfium: the mode gestures
/// only ask whether a session exists.
fn open_model_document(built: &BuiltUi) {
    built.viewer.state.borrow_mut().session = Some(model_session(pdf_document::Document::blank()));
}

/// Clears the session first, or closing raises the unsaved-changes prompt.
fn close(built: BuiltUi) {
    built.viewer.state.borrow_mut().session = None;
    built.window.close();
}

#[gtk::test]
fn gtk_ui_the_bar_lists_the_six_modes_in_order_in_spanish() {
    let _spanish = speak(Language::Spanish);
    let bar = build_mode_bar();

    let captions: Vec<String> = Mode::ALL
        .iter()
        .map(|mode| caption(bar.button(*mode)))
        .collect();

    assert_eq!(
        captions,
        [
            "Inicio",
            "Editar",
            "Anotar",
            "Organizar",
            "Firmar",
            "Proteger"
        ]
    );
}

#[gtk::test]
fn gtk_ui_every_mode_keeps_its_accessible_name_and_tooltip() {
    use gtk::glib::translate::{from_glib_full, ToGlibPtr};

    let bar = build_mode_bar();
    for mode in Mode::ALL {
        let button = bar.button(mode);
        let title = caption(button);
        let accessible: &gtk::Accessible = button.as_ref();
        let expected = <str as ToGlibPtr<'_, *const std::ffi::c_char>>::to_glib_none(&title);
        let mismatch: Option<gtk::glib::GString> = unsafe {
            from_glib_full(gtk::ffi::gtk_test_accessible_check_property(
                accessible.to_glib_none().0,
                gtk::ffi::GTK_ACCESSIBLE_PROPERTY_LABEL,
                expected.0,
            ))
        };
        assert!(mismatch.is_none(), "{title}: {mismatch:?}");
        assert_eq!(button.tooltip_text().as_deref(), Some(title.as_str()));
    }
}

/// The same lesson as the tab strip it replaced: a row of labelled controls
/// that cannot wrap sets the window's minimum width.
#[gtk::test]
fn gtk_ui_the_bar_wraps_instead_of_setting_the_window_floor() {
    let bar = build_mode_bar();

    let (minimum, _, _, _) = bar.root.measure(Orientation::Horizontal, -1);
    let sum_of_tabs: i32 = Mode::ALL
        .iter()
        .map(|mode| bar.button(*mode).measure(Orientation::Horizontal, -1).1)
        .sum();

    assert!(
        minimum < sum_of_tabs,
        "mode bar minimum {minimum} is not below the {sum_of_tabs} its tabs add up to"
    );
}

#[test]
fn every_screen_reads_as_exactly_one_place_mode() {
    let cases = [
        ((HOME_PAGE, ANNOTATE_PAGE), Mode::Home),
        ((ORGANIZE_PAGE, EDIT_PAGE), Mode::Organize),
        ((EDITOR_PAGE, EDIT_PAGE), Mode::Edit),
        ((EDITOR_PAGE, ANNOTATE_PAGE), Mode::Annotate),
        ((EDITOR_PAGE, FILL_SIGN_PAGE), Mode::Sign),
    ];
    for ((view, tools), mode) in cases {
        assert_eq!(Mode::of_screen(Some(view), Some(tools)), Some(mode));
    }
    assert_eq!(Mode::of_screen(None, Some(EDIT_PAGE)), None);
    assert!(Mode::ALL
        .iter()
        .filter(|mode| !mode.is_place())
        .eq([Mode::Protect].iter()));
}

/// The window opens on Home, and the bar says so before anything is clicked.
#[gtk::test]
fn gtk_ui_the_window_opens_with_home_checked() {
    let built = built_ui();

    assert_eq!(built.mode_bar.checked(), Some(Mode::Home));

    close(built);
}

/// Each document mode lands on its own screen and tools page, and only its
/// own tab ends up checked.
#[gtk::test]
fn gtk_ui_each_mode_opens_its_screen_and_tools_page() {
    let built = built_ui();
    open_model_document(&built);

    for (mode, tools_page) in [
        (Mode::Edit, EDIT_PAGE),
        (Mode::Annotate, ANNOTATE_PAGE),
        (Mode::Sign, FILL_SIGN_PAGE),
    ] {
        built.mode_bar.button(mode).emit_clicked();

        assert_eq!(
            built.viewer.view_stack.visible_child_name().as_deref(),
            Some(EDITOR_PAGE),
            "{mode:?}"
        );
        assert_eq!(
            built.viewer.tools_stack.visible_child_name().as_deref(),
            Some(tools_page),
            "{mode:?}"
        );
        assert_eq!(built.mode_bar.checked(), Some(mode));
    }

    built.mode_bar.button(Mode::Home).emit_clicked();
    assert_eq!(
        built.viewer.view_stack.visible_child_name().as_deref(),
        Some(HOME_PAGE)
    );
    assert_eq!(built.mode_bar.checked(), Some(Mode::Home));

    close(built);
}

/// Clicking the tab that is already checked re-asserts it rather than
/// leaving the bar with nothing checked.
#[gtk::test]
fn gtk_ui_clicking_the_checked_mode_keeps_it_checked() {
    let built = built_ui();

    // Twice: a bare `ToggleButton` would end up released either the first
    // time (it starts checked) or the second.
    built.mode_bar.button(Mode::Home).emit_clicked();
    assert_eq!(built.mode_bar.checked(), Some(Mode::Home));
    built.mode_bar.button(Mode::Home).emit_clicked();
    assert_eq!(built.mode_bar.checked(), Some(Mode::Home));

    close(built);
}

/// The bar follows screens changed by paths that never go through it:
/// opening a file, Organize's back button, a refresh restoring its screen.
#[gtk::test]
fn gtk_ui_the_bar_follows_screens_changed_elsewhere() {
    let built = built_ui();

    home::show_editor(&built.viewer);
    assert_eq!(built.mode_bar.checked(), Some(Mode::Annotate));

    built
        .viewer
        .tools_stack
        .set_visible_child_name(FILL_SIGN_PAGE);
    assert_eq!(built.mode_bar.checked(), Some(Mode::Sign));

    built
        .viewer
        .view_stack
        .set_visible_child_name(ORGANIZE_PAGE);
    assert_eq!(built.mode_bar.checked(), Some(Mode::Organize));

    home::show_home(&built.viewer);
    assert_eq!(built.mode_bar.checked(), Some(Mode::Home));

    close(built);
}

/// Without a document, a document mode does what the rail did: it stays
/// clickable, picks a file first and arms itself for the open that follows.
/// The screen has not moved, so neither has the checked tab.
#[gtk::test]
fn gtk_ui_a_document_mode_without_a_document_arms_itself_and_stays_on_home() {
    let built = built_ui();

    for (mode, tool) in [
        (Mode::Edit, HomeTool::Edit),
        (Mode::Protect, HomeTool::Protect),
    ] {
        assert!(built.mode_bar.button(mode).is_sensitive(), "{mode:?}");
        built.mode_bar.button(mode).emit_clicked();

        assert_eq!(built.viewer.state.borrow().pending_tool, Some(tool));
        assert_eq!(built.mode_bar.checked(), Some(Mode::Home), "{mode:?}");
    }

    close(built);
}

/// Protect opens a dialog over the screen; it is never the checked tab.
#[gtk::test]
fn gtk_ui_protect_is_an_action_not_a_tab() {
    let bar = build_mode_bar();

    assert!(bar
        .button(Mode::Protect)
        .downcast_ref::<gtk::ToggleButton>()
        .is_none());
    for mode in Mode::ALL.into_iter().filter(|mode| mode.is_place()) {
        assert!(
            bar.button(mode)
                .downcast_ref::<gtk::ToggleButton>()
                .is_some(),
            "{mode:?}"
        );
    }
}
