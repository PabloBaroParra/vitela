//! The window shortcuts, written down once (B27 decision 7).
//!
//! Each [`Shortcut`] pairs a `win.*` action name with its accelerator. The
//! code that registers the action (`connect_standard_shortcuts`,
//! `annotations::connect_history_shortcuts`) and the command palette's
//! catalog and Home's shortcut card all read these constants, so the key the
//! palette and the card show is the key GTK listens for. [`accel_label`] is
//! the one place an accelerator becomes the text the user reads.
//!
//! The text-editing keys ([`TEXT_EDITING`]) are handed to the palette's
//! search and the Find entry while they have the cursor: see
//! [`connect_search_field_handoff`].
//!
//! [`table`] lists every key the window answers to — these accelerators and
//! the canvas keys that are not actions — grouped and labelled, and
//! [`help`] prints that list as the keyboard shortcuts window.

pub(crate) mod help;
pub(crate) mod table;

use gtk::gdk;
use gtk::prelude::*;

use crate::app::i18n::tr;

/// One window action and the accelerator that triggers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
    /// The action's name inside the window's `win` group, e.g. `"open"`.
    pub(crate) action: &'static str,
    /// The accelerator in GTK's syntax, e.g. `"<Control>o"`. The one shown
    /// wherever the shortcut is printed.
    pub(crate) accel: &'static str,
    /// Other accelerators for the same action: the keypad's key, the key a
    /// layout puts the symbol on, a second convention for the same command.
    pub(crate) also: &'static [&'static str],
}

pub(crate) const OPEN: Shortcut = Shortcut {
    action: "open",
    accel: "<Control>o",
    also: &[],
};
pub(crate) const SAVE: Shortcut = Shortcut {
    action: "save",
    accel: "<Control>s",
    also: &[],
};
/// Saves under a name the user picks, where [`SAVE`] writes back to the file
/// the document came from.
pub(crate) const SAVE_AS: Shortcut = Shortcut {
    action: "save-as",
    accel: "<Control><Shift>s",
    also: &[],
};
pub(crate) const PRINT: Shortcut = Shortcut {
    action: "print",
    accel: "<Control>p",
    also: &[],
};
pub(crate) const FIND: Shortcut = Shortcut {
    action: "find",
    accel: "<Control>f",
    also: &[],
};
pub(crate) const NEW: Shortcut = Shortcut {
    action: "new",
    accel: "<Control>n",
    also: &[],
};
pub(crate) const UNDO: Shortcut = Shortcut {
    action: "undo",
    accel: "<Control>z",
    also: &[],
};
/// Ctrl+Y and Ctrl+Shift+Z: both conventions are in use on Linux desktops.
pub(crate) const REDO: Shortcut = Shortcut {
    action: "redo",
    accel: "<Control>y",
    also: &["<Control><Shift>z"],
};
/// Cuts the content selected in content-edit mode (B25 T-209).
pub(crate) const CUT: Shortcut = Shortcut {
    action: "cut-content",
    accel: "<Control>x",
    also: &[],
};
/// Copies the content selection, or the selected page text.
pub(crate) const COPY: Shortcut = Shortcut {
    action: "copy",
    accel: "<Control>c",
    also: &[],
};
/// Pastes the held content clip, or a bitmap as a stamp.
pub(crate) const PASTE: Shortcut = Shortcut {
    action: "paste",
    accel: "<Control>v",
    also: &[],
};
/// Closes the active document's tab, asking about unsaved changes first.
pub(crate) const CLOSE_TAB: Shortcut = Shortcut {
    action: "close-tab",
    accel: "<Control>w",
    also: &[],
};
/// Opens the command palette (B27).
pub(crate) const COMMAND_PALETTE: Shortcut = Shortcut {
    action: "command-palette",
    accel: "<Control>k",
    also: &[],
};

/// Bold, italic and underline for the text content-edit mode is on
/// (`content_edit::style_toggles`).
pub(crate) const BOLD: Shortcut = Shortcut {
    action: "content-bold",
    accel: "<Control>b",
    also: &[],
};
pub(crate) const ITALIC: Shortcut = Shortcut {
    action: "content-italic",
    accel: "<Control>i",
    also: &[],
};
pub(crate) const UNDERLINE: Shortcut = Shortcut {
    action: "content-underline",
    accel: "<Control>u",
    also: &[],
};

/// Zoom, one step of the toolbar's ladder. Ctrl+= is the same physical key
/// as Ctrl++ on layouts that put `+` over `=`.
pub(crate) const ZOOM_IN: Shortcut = Shortcut {
    action: "zoom-in",
    accel: "<Control>plus",
    also: &["<Control>equal", "<Control>KP_Add"],
};
pub(crate) const ZOOM_OUT: Shortcut = Shortcut {
    action: "zoom-out",
    accel: "<Control>minus",
    also: &["<Control>KP_Subtract"],
};
/// Actual size: 100%, what the key means in browsers and in other viewers.
pub(crate) const ZOOM_RESET: Shortcut = Shortcut {
    action: "zoom-reset",
    accel: "<Control>0",
    also: &["<Control>KP_0"],
};
pub(crate) const FIT_WIDTH: Shortcut = Shortcut {
    action: "fit-width",
    accel: "<Control>1",
    also: &["<Control>KP_1"],
};
pub(crate) const FIT_PAGE: Shortcut = Shortcut {
    action: "fit-page",
    accel: "<Control>2",
    also: &["<Control>KP_2"],
};
pub(crate) const FULLSCREEN: Shortcut = Shortcut {
    action: "fullscreen",
    accel: "F11",
    also: &[],
};
/// Shows or hides the page thumbnails and the tools panel.
pub(crate) const TOGGLE_PAGES: Shortcut = Shortcut {
    action: "toggle-pages-panel",
    accel: "F9",
    also: &[],
};
pub(crate) const TOGGLE_TOOLS: Shortcut = Shortcut {
    action: "toggle-tools-panel",
    accel: "<Shift>F9",
    also: &[],
};
pub(crate) const FIND_NEXT: Shortcut = Shortcut {
    action: "find-next",
    accel: "F3",
    also: &["<Control>g"],
};
pub(crate) const FIND_PREVIOUS: Shortcut = Shortcut {
    action: "find-previous",
    accel: "<Shift>F3",
    also: &["<Control><Shift>g"],
};
/// The next and previous document tab. Shift+Tab arrives as `ISO_Left_Tab`
/// on X11 keymaps; GTK folds that into `<Shift>Tab` when it registers and
/// matches an accelerator, so one spelling covers both.
pub(crate) const NEXT_TAB: Shortcut = Shortcut {
    action: "next-tab",
    accel: "<Control>Tab",
    also: &["<Control>Page_Down"],
};
pub(crate) const PREVIOUS_TAB: Shortcut = Shortcut {
    action: "previous-tab",
    accel: "<Control><Shift>Tab",
    also: &["<Control>Page_Up"],
};
/// Closes the window, asking about unsaved changes as the close button does.
pub(crate) const QUIT: Shortcut = Shortcut {
    action: "quit",
    accel: "<Control>q",
    also: &[],
};
/// Opens the keyboard shortcuts window ([`help`]).
pub(crate) const SHORTCUTS_HELP: Shortcut = Shortcut {
    action: "shortcuts",
    accel: "F1",
    also: &["<Control>question"],
};
/// Deletes the selected annotation or content item
/// (`annotations::connect_delete_shortcut`).
pub(crate) const DELETE: Shortcut = Shortcut {
    action: "delete-annotation",
    accel: "Delete",
    also: &[],
};
/// Retypes the selected text block (`content_edit::block`).
pub(crate) const RETYPE: Shortcut = Shortcut {
    action: "retype-text",
    accel: "F2",
    also: &[],
};
/// Lets go of an armed tool, a selection or a toast (`escape`).
pub(crate) const ESCAPE: Shortcut = Shortcut {
    action: "escape",
    accel: "Escape",
    also: &[],
};

impl Shortcut {
    /// Every accelerator bound to the action, the shown one first.
    pub(crate) fn accels(self) -> Vec<&'static str> {
        std::iter::once(self.accel)
            .chain(self.also.iter().copied())
            .collect()
    }

    /// The action's detailed name, `win.<action>`.
    pub(crate) fn detailed_action(self) -> String {
        format!("win.{}", self.action)
    }

    /// Binds the accelerator to the window action on `application`.
    pub(crate) fn install(self, application: &gtk::Application) {
        application.set_accels_for_action(&self.detailed_action(), &self.accels());
    }

    /// Removes the accelerator, leaving the action itself as it is.
    fn uninstall(self, application: &gtk::Application) {
        application.set_accels_for_action(&self.detailed_action(), &[]);
    }
}

/// An accelerator in GTK syntax as the user reads it, e.g. `"Ctrl+O"` or
/// `"Ctrl+Shift+S"`, or `None` when GTK cannot parse it.
///
/// The one formatter for every place a shortcut is shown — the tooltips,
/// the command palette's rows, Home's shortcut card and the keyboard
/// shortcuts window — so no two of them can spell the same key differently.
/// See [`key_label`] for the shape.
pub(crate) fn accel_label(accel: &str) -> Option<String> {
    let (key, modifiers) = gtk::accelerator_parse(accel)?;
    Some(key_label(key, modifiers))
}

/// `key` held with `modifiers`, as the user reads it: the modifiers in a
/// fixed order — Ctrl, Alt, Shift — then the key, joined by `+`.
///
/// The modifier names are the window's own words (Fluent), so they follow
/// the language the rest of the window speaks; GTK's label put Shift first
/// and spoke the system locale. The key itself is GTK's label for it alone
/// — `+`, `Page Up`, `F9` — which names keys the way the rest of the desktop
/// does. Other modifiers are not used by any binding here and are left out.
pub(crate) fn key_label(key: gdk::Key, modifiers: gdk::ModifierType) -> String {
    let names = modifier_keys(
        modifiers.contains(gdk::ModifierType::CONTROL_MASK),
        modifiers.contains(gdk::ModifierType::ALT_MASK),
        modifiers.contains(gdk::ModifierType::SHIFT_MASK),
    )
    .into_iter()
    .map(tr);
    let key = gtk::accelerator_get_label(key, gdk::ModifierType::empty());
    compose(names, &key)
}

/// The message keys of the modifiers held, in the order they are printed.
fn modifier_keys(control: bool, alt: bool, shift: bool) -> Vec<&'static str> {
    [
        (control, "shortcut-modifier-ctrl"),
        (alt, "shortcut-modifier-alt"),
        (shift, "shortcut-modifier-shift"),
    ]
    .into_iter()
    .filter_map(|(held, key)| held.then_some(key))
    .collect()
}

/// `modifiers` and `key` joined by `+`.
fn compose(modifiers: impl IntoIterator<Item = String>, key: &str) -> String {
    modifiers
        .into_iter()
        .chain(std::iter::once(key.to_owned()))
        .collect::<Vec<_>>()
        .join("+")
}

/// `label` followed by the shortcut's key in brackets, e.g. `"Undo (Ctrl+Z)"`
/// — every tooltip that names a key, with the key read from the table rather
/// than typed into the translation, where it could go on naming a key that
/// has since moved, or spell it unlike the palette.
pub(crate) fn tooltip(label: &str, shortcut: Shortcut) -> String {
    match accel_label(shortcut.accel) {
        Some(keys) => format!("{label} ({keys})"),
        None => label.to_owned(),
    }
}

/// The window shortcuts a text field has keys of its own for: cut, copy,
/// paste, undo and redo. While a search field has the cursor these belong
/// to the query being typed, not to the document.
pub(crate) const TEXT_EDITING: [Shortcut; 5] = [CUT, COPY, PASTE, UNDO, REDO];

/// Hands the [`TEXT_EDITING`] keys to the two search fields — the command
/// palette's and the Find entry — while one of them has the cursor, and
/// gives them back to the window when it leaves.
///
/// `win.*` accelerators are resolved before the focused widget sees the key,
/// so without this Ctrl+X in the palette's search cut the selected page
/// content and Ctrl+Z undid a document edit.
///
/// Only these two fields, named explicitly, and not every text input
/// (`annotations::text_input_has_focus`): a query is never page content,
/// but other fields keep the cursor while the user goes on working on the
/// page with the mouse — a click on the page does not move focus — so a
/// form's font-size spin button left focused would keep Ctrl+Z dead. The
/// content-edit line and paragraph editors keep document undo and the
/// content clipboard on purpose, as before.
///
/// The accelerators are unbound rather than the actions disabled, which is
/// how Delete and F2 stand down (`annotations::connect_delete_shortcut`):
/// `win.undo`/`win.redo` also drive the toolbar's and Organize's Undo/Redo
/// buttons and the palette's Undo entry, all of which read the action's
/// enabled state and must keep following the document while the cursor
/// sits in a search field. With no accelerator bound, GTK has nothing to
/// dispatch and the key goes on to the field; the actions' enabled state is
/// never touched, so nothing has to recompute it on the way back. The
/// window's `notify::focus-widget` watch decides it, as for Delete and F2.
pub(crate) fn connect_search_field_handoff(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    search_fields: [gtk::Widget; 2],
) {
    sync_search_field_handoff(application, window, &search_fields);
    window.connect_focus_widget_notify({
        let application = application.clone();
        move |window| sync_search_field_handoff(&application, window, &search_fields)
    });
}

/// Whether the window's focus is in one of `fields`. A `GtkEntry` hands its
/// focus to an inner `GtkText`, so the focus widget is a descendant.
fn focus_is_in(window: &gtk::ApplicationWindow, fields: &[gtk::Widget]) -> bool {
    // `focus` is ambiguous: `RootExt` has one too.
    gtk::prelude::GtkWindowExt::focus(window).is_some_and(|focused| {
        fields
            .iter()
            .any(|field| focused == *field || focused.is_ancestor(field))
    })
}

fn sync_search_field_handoff(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    search_fields: &[gtk::Widget],
) {
    let in_search = focus_is_in(window, search_fields);
    // Only on a change: rebinding on every focus move would make GTK rebuild
    // the window's shortcut list for nothing.
    let bound = !application
        .accels_for_action(&UNDO.detailed_action())
        .is_empty();
    if bound != in_search {
        return;
    }
    for shortcut in TEXT_EDITING {
        if in_search {
            shortcut.uninstall(application);
        } else {
            shortcut.install(application);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Shortcut; 15] = [
        OPEN,
        SAVE,
        SAVE_AS,
        PRINT,
        FIND,
        NEW,
        UNDO,
        REDO,
        CUT,
        COPY,
        PASTE,
        COMMAND_PALETTE,
        BOLD,
        ITALIC,
        UNDERLINE,
    ];

    #[test]
    fn every_action_and_accelerator_is_used_once() {
        for (index, shortcut) in ALL.iter().enumerate() {
            for other in &ALL[index + 1..] {
                assert_ne!(shortcut.action, other.action);
                assert_ne!(shortcut.accel, other.accel);
            }
        }
    }

    #[gtk::test]
    fn gtk_ui_accel_label_is_what_the_user_reads() {
        assert_eq!(accel_label(OPEN.accel).as_deref(), Some("Ctrl+O"));
        assert_eq!(accel_label(SAVE_AS.accel).as_deref(), Some("Ctrl+Shift+S"));
        assert_eq!(
            accel_label(COMMAND_PALETTE.accel).as_deref(),
            Some("Ctrl+K")
        );
        assert_eq!(accel_label(TOGGLE_TOOLS.accel).as_deref(), Some("Shift+F9"));
        assert_eq!(
            accel_label("<Shift><Alt><Control>x").as_deref(),
            Some("Ctrl+Alt+Shift+X")
        );
        assert_eq!(accel_label("not an accelerator"), None);
    }

    /// Spanish names Shift as its keyboards print it.
    #[gtk::test]
    fn gtk_ui_the_modifier_names_speak_the_window_s_language() {
        use crate::app::i18n::{speak, Language};
        let _spanish = speak(Language::Spanish);

        assert_eq!(accel_label(SAVE_AS.accel).as_deref(), Some("Ctrl+Mayús+S"));
        assert_eq!(accel_label("<Alt>F4").as_deref(), Some("Alt+F4"));
        assert_eq!(
            tooltip(&tr("editor-save-as"), SAVE_AS),
            "Guardar como (Ctrl+Mayús+S)"
        );
    }

    #[test]
    fn the_modifiers_are_printed_ctrl_alt_shift_then_the_key() {
        assert_eq!(
            modifier_keys(true, true, true),
            [
                "shortcut-modifier-ctrl",
                "shortcut-modifier-alt",
                "shortcut-modifier-shift"
            ]
        );
        assert_eq!(
            modifier_keys(false, false, true),
            ["shortcut-modifier-shift"]
        );
        assert!(modifier_keys(false, false, false).is_empty());
        assert_eq!(
            compose(["Ctrl".to_owned(), "Shift".to_owned()], "S"),
            "Ctrl+Shift+S"
        );
        assert_eq!(compose(Vec::new(), "F9"), "F9");
    }

    /// Every tooltip that names a key: the words from the catalog, the key
    /// from the table, in the one spelling ([`tooltip`]). None of the
    /// catalog's tooltips types a key of its own.
    #[gtk::test]
    fn gtk_ui_every_tooltip_that_names_a_key_reads_it_from_the_table() {
        use crate::app::test_support::find_button;

        let built = crate::app::ui_tests::built_ui();
        let viewer = &built.viewer;
        let tooltip_of = |widget: &gtk::Widget| widget.tooltip_text().map(String::from);
        let find_button_widget = viewer
            .search_entry
            .ancestor(gtk::MenuButton::static_type())
            .expect("the Find entry sits behind a menu button");
        let home_open = find_button(built.window.upcast_ref(), &tr("home-open-file"))
            .expect("Home has an Open file button");
        let quick = &viewer.content_quick_actions;
        let cases: Vec<(gtk::Widget, &str, Shortcut, &str)> = vec![
            (
                viewer.save_button.clone().upcast(),
                "editor-save",
                SAVE,
                "Save (Ctrl+S)",
            ),
            (
                viewer.print_button.clone().upcast(),
                "editor-print",
                PRINT,
                "Print (Ctrl+P)",
            ),
            (
                find_button_widget,
                "editor-find",
                FIND,
                "Find in document (Ctrl+F)",
            ),
            (
                home_open.upcast(),
                "home-open-file-tooltip",
                OPEN,
                "Open a PDF (Ctrl+O)",
            ),
            (
                viewer.style_toggles.bold.clone().upcast(),
                "edit-style-bold-tooltip",
                BOLD,
                "Make the selected text, or the text being retyped, bold (Ctrl+B)",
            ),
            (
                viewer.style_toggles.italic.clone().upcast(),
                "edit-style-italic-tooltip",
                ITALIC,
                "Make the selected text, or the text being retyped, italic (Ctrl+I)",
            ),
            (
                viewer.style_toggles.underline.clone().upcast(),
                "edit-style-underline-tooltip",
                UNDERLINE,
                "Underline the selected text, or the text being retyped (Ctrl+U)",
            ),
            (
                quick.retype.clone().upcast(),
                "edit-quick-retype-tooltip",
                RETYPE,
                "Retype the selected text in place (F2)",
            ),
            (
                quick.cut.clone().upcast(),
                "edit-quick-cut-tooltip",
                CUT,
                "Cut the selection to the clipboard (Ctrl+X)",
            ),
            (
                quick.copy.clone().upcast(),
                "edit-quick-copy-tooltip",
                COPY,
                "Copy the selection to the clipboard (Ctrl+C)",
            ),
            (
                quick.delete.clone().upcast(),
                "edit-quick-delete-tooltip",
                DELETE,
                "Remove the selection from the page (Delete)",
            ),
        ];
        for (widget, label_key, shortcut, english) in cases {
            assert!(
                !tr(label_key).contains('('),
                "{label_key} types a key of its own: {:?}",
                tr(label_key)
            );
            assert_eq!(
                tooltip_of(&widget).as_deref(),
                Some(tooltip(&tr(label_key), shortcut).as_str()),
                "{label_key}"
            );
            assert_eq!(tooltip_of(&widget).as_deref(), Some(english), "{label_key}");
        }
        built.window.close();
    }

    #[test]
    fn the_detailed_name_is_in_the_window_group() {
        assert_eq!(OPEN.detailed_action(), "win.open");
    }

    mod search_field_handoff {
        use gtk::prelude::*;
        use pdf_render::PageRotation;

        use crate::app::annotations::text_input_has_focus;
        use crate::app::content_edit::clipboard;
        use crate::app::content_edit::test_support::Fixture;
        use crate::app::state::ContentSelection;
        use crate::app::update_content_edit_controls;

        const CUT: &str = "win.cut-content";
        const COPY: &str = "win.copy";
        const PASTE: &str = "win.paste";
        const UNDO: &str = "win.undo";
        const REDO: &str = "win.redo";
        const TEXT_EDITING: [&str; 5] = [CUT, COPY, PASTE, UNDO, REDO];

        fn pump() {
            let context = gtk::glib::MainContext::default();
            for _ in 0..50 {
                while context.iteration(false) {}
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        /// Whether a key press would reach `action`: GTK only dispatches a
        /// shortcut that one of the window's capture-phase shortcut
        /// controllers lists (that is where application accelerators land —
        /// see `palette::tests`' Ctrl+K measurement), to an action that is
        /// enabled. Either one missing and the key goes on to the focused
        /// widget.
        fn fires(fixture: &Fixture, detailed: &str) -> bool {
            let name = detailed.trim_start_matches("win.");
            let enabled = fixture
                .built
                .window
                .lookup_action(name)
                .is_some_and(|action| action.is_enabled());
            bound(fixture, detailed) && enabled
        }

        /// Whether the window's shortcut controllers carry `detailed`.
        fn bound(fixture: &Fixture, detailed: &str) -> bool {
            let controllers = fixture.built.window.observe_controllers();
            (0..controllers.n_items())
                .filter_map(|index| {
                    controllers
                        .item(index)
                        .and_downcast::<gtk::ShortcutController>()
                })
                .any(|controller| {
                    (0..controller.n_items())
                        .filter_map(|item| controller.item(item).and_downcast::<gtk::Shortcut>())
                        .any(|shortcut| {
                            shortcut
                                .action()
                                .and_downcast::<gtk::NamedAction>()
                                .is_some_and(|action| action.action_name() == detailed)
                        })
                })
        }

        /// Both lines held as a content selection, the way a marquee leaves
        /// them, with the window's controls refreshed from it.
        fn select_content(fixture: &Fixture) {
            {
                let mut state = fixture.viewer().state.borrow_mut();
                let session = state.session.as_mut().expect("a session");
                session.content_selection = Some(ContentSelection {
                    page_index: 0,
                    members: pdf_edit::Selection {
                        runs: fixture.runs.clone(),
                        ..pdf_edit::Selection::default()
                    },
                });
            }
            update_content_edit_controls(fixture.viewer());
        }

        fn selection_count(fixture: &Fixture) -> usize {
            fixture
                .viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.content_selection.as_ref())
                .map_or(0, |held| held.members.runs.len())
        }

        fn queued_edits(fixture: &Fixture) -> usize {
            fixture
                .viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.document_model.as_ref())
                .map_or(0, |model| model.pending_edits.entries().len())
        }

        /// Typing a query is not editing the page: with content selected,
        /// Ctrl+X in the palette's search field cuts the query, so the
        /// window's cut must not fire — and nothing is cut.
        #[gtk::test]
        fn gtk_ui_ctrl_x_in_the_palette_search_does_not_cut_content() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            assert!(fires(&fixture, CUT), "Ctrl+X cuts the selection");

            fixture.built.palette.open();
            pump();

            assert!(text_input_has_focus(fixture.viewer()));
            assert!(!fires(&fixture, CUT));
            assert_eq!(selection_count(&fixture), 2, "the content is still there");
            assert_eq!(queued_edits(&fixture), 0, "nothing was cut");
        }

        /// A held clip is pasted by Ctrl+V on the page, not from inside the
        /// palette's search field, where it pastes text.
        #[gtk::test]
        fn gtk_ui_ctrl_v_in_the_palette_search_does_not_paste_the_clip() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            assert!(clipboard::try_copy(fixture.viewer()), "the clip is held");
            assert!(fires(&fixture, PASTE));

            fixture.built.palette.open();
            pump();

            assert!(!fires(&fixture, PASTE));
            assert!(!fires(&fixture, COPY));
            assert_eq!(queued_edits(&fixture), 0, "nothing was pasted");
        }

        /// The Find field had the same gap before the palette existed.
        #[gtk::test]
        fn gtk_ui_ctrl_x_and_ctrl_v_stand_down_in_the_find_field() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            assert!(clipboard::try_copy(fixture.viewer()));
            assert!(fires(&fixture, CUT) && fires(&fixture, PASTE));

            let window = &fixture.built.window;
            gtk::prelude::GtkWindowExt::set_focus(window, Some(&fixture.viewer().search_entry));
            pump();

            assert!(text_input_has_focus(fixture.viewer()));
            for detailed in TEXT_EDITING {
                assert!(!fires(&fixture, detailed), "{detailed}");
            }
            assert_eq!(selection_count(&fixture), 2);
            assert_eq!(queued_edits(&fixture), 0);
        }

        /// Only the palette's search and the Find entry take the keys. Any
        /// other text field — a form's font-size spin button, a plain entry,
        /// the content editors — keeps them on the document: the cursor
        /// stays in such a field while the user goes on working on the page
        /// with the mouse, and Ctrl+Z must still undo there.
        #[gtk::test]
        fn gtk_ui_other_text_fields_leave_the_shortcuts_bound() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            assert!(clipboard::try_copy(fixture.viewer()));
            let spin = gtk::SpinButton::with_range(4.0, 400.0, 1.0);
            let entry = gtk::Entry::new();
            fixture.viewer().pages.append(&spin);
            fixture.viewer().pages.append(&entry);
            pump();

            for field in [spin.upcast_ref::<gtk::Widget>(), entry.upcast_ref()] {
                assert!(field.grab_focus(), "the test window takes focus");
                pump();
                assert!(text_input_has_focus(fixture.viewer()));
                for detailed in TEXT_EDITING {
                    assert!(bound(&fixture, detailed), "{detailed}");
                }
                assert!(fires(&fixture, CUT) && fires(&fixture, PASTE));
            }
        }

        /// Leaving the text field gives the keys back, to actions whose
        /// enabled state still follows the document, not a blanket "on".
        #[gtk::test]
        fn gtk_ui_closing_the_palette_gives_ctrl_x_back() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            fixture.built.palette.open();
            pump();
            assert!(!fires(&fixture, CUT));

            fixture.built.palette.close();
            pump();

            assert!(!text_input_has_focus(fixture.viewer()));
            assert!(fires(&fixture, CUT), "Ctrl+X cuts the selection again");
            assert!(fires(&fixture, COPY) && fires(&fixture, PASTE));
            assert!(bound(&fixture, UNDO) && bound(&fixture, REDO));
            // Nothing to undo: the action is still off, as the document says.
            assert!(!fires(&fixture, UNDO));
        }

        /// Ctrl+Z / Ctrl+Y in the search field never reach the document's
        /// history — and the actions themselves stay as the document has
        /// them, so the toolbar's Undo and the palette's Undo entry, which
        /// read `is_enabled()`, are not greyed out by the cursor's position.
        #[gtk::test]
        fn gtk_ui_undo_and_redo_stand_down_in_the_palette_search() {
            let fixture = Fixture::new(PageRotation::None);
            let viewer = fixture.viewer();
            select_content(&fixture);
            assert!(clipboard::try_cut(viewer), "a cut to undo");
            assert_eq!(queued_edits(&fixture), 1);
            assert!(fires(&fixture, UNDO));

            fixture.built.palette.open();
            pump();

            assert!(!fires(&fixture, UNDO));
            assert!(!fires(&fixture, REDO));
            assert!(viewer.undo_action.is_enabled(), "the document can undo");
            assert_eq!(queued_edits(&fixture), 1, "the cut is still there");
        }
    }

    #[gtk::test]
    fn gtk_ui_every_accelerator_parses() {
        for shortcut in ALL {
            assert!(
                gtk::accelerator_parse(shortcut.accel).is_some(),
                "{shortcut:?}"
            );
        }
    }
}
