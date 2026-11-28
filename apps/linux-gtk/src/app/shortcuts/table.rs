//! Every key the window answers to, in one list: what it is called, which
//! group it reads under, and what triggers it.
//!
//! The constants in [`super`] say which accelerator an action has. This says
//! which shortcuts *exist*: the accelerators, the canvas keys that are not
//! actions (`canvas_keys`), and the one gesture that needs a key held. The
//! shortcuts window ([`super::help`]) prints it row for row, and the tests
//! here hold it against what GTK actually has bound — so a key added
//! without a row, or two rows on one key, fails the suite instead of going
//! unnoticed.

use super::Shortcut;
use crate::app::canvas_keys::Nav;

/// The heading a row is printed under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Group {
    File,
    Edit,
    Find,
    View,
    Navigate,
    Window,
}

impl Group {
    /// In the order the shortcuts window shows them.
    pub(crate) const ALL: [Group; 6] = [
        Group::File,
        Group::Edit,
        Group::Find,
        Group::View,
        Group::Navigate,
        Group::Window,
    ];

    /// Message id of the group's heading.
    pub(crate) fn title_key(self) -> &'static str {
        match self {
            Group::File => "shortcuts-group-file",
            Group::Edit => "shortcuts-group-edit",
            Group::Find => "shortcuts-group-find",
            Group::View => "shortcuts-group-view",
            Group::Navigate => "shortcuts-group-navigate",
            Group::Window => "shortcuts-group-window",
        }
    }
}

/// What triggers a row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Keys {
    /// A window action's accelerators.
    Action(Shortcut),
    /// The canvas keys that make this move.
    Canvas(Nav),
    /// A key held with a pointer gesture, which no accelerator can spell:
    /// the message id of how it reads, e.g. "Space + drag".
    Gesture(&'static str),
}

/// One shortcut as the user reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Row {
    pub(crate) group: Group,
    /// Message id of what the shortcut does.
    pub(crate) label_key: &'static str,
    pub(crate) keys: Keys,
}

const fn action(group: Group, label_key: &'static str, shortcut: Shortcut) -> Row {
    Row {
        group,
        label_key,
        keys: Keys::Action(shortcut),
    }
}

const fn canvas(label_key: &'static str, nav: Nav) -> Row {
    Row {
        group: Group::Navigate,
        label_key,
        keys: Keys::Canvas(nav),
    }
}

/// Every shortcut, grouped. Labels reuse the message of the control the
/// shortcut stands for wherever one exists.
pub(crate) const TABLE: &[Row] = &[
    action(Group::File, "home-shortcut-open", super::OPEN),
    action(Group::File, "home-quick-new-blank", super::NEW),
    action(Group::File, "editor-save", super::SAVE),
    action(Group::File, "editor-save-as", super::SAVE_AS),
    action(Group::File, "editor-print", super::PRINT),
    action(Group::File, "tabs-close", super::CLOSE_TAB),
    action(Group::File, "shortcuts-quit", super::QUIT),
    action(Group::Edit, "editor-undo", super::UNDO),
    action(Group::Edit, "editor-redo", super::REDO),
    action(Group::Edit, "shortcuts-cut", super::CUT),
    action(Group::Edit, "shortcuts-copy", super::COPY),
    action(Group::Edit, "shortcuts-paste", super::PASTE),
    action(Group::Edit, "shortcuts-delete", super::DELETE),
    action(Group::Edit, "shortcuts-retype", super::RETYPE),
    action(Group::Edit, "edit-style-bold", super::BOLD),
    action(Group::Edit, "edit-style-italic", super::ITALIC),
    action(Group::Edit, "edit-style-underline", super::UNDERLINE),
    action(Group::Edit, "shortcuts-escape", super::ESCAPE),
    action(Group::Find, "editor-find", super::FIND),
    action(Group::Find, "editor-find-next", super::FIND_NEXT),
    action(Group::Find, "editor-find-previous", super::FIND_PREVIOUS),
    action(Group::View, "editor-zoom-in", super::ZOOM_IN),
    action(Group::View, "editor-zoom-out", super::ZOOM_OUT),
    action(Group::View, "shortcuts-zoom-reset", super::ZOOM_RESET),
    action(Group::View, "editor-fit-width", super::FIT_WIDTH),
    action(Group::View, "editor-fit-page", super::FIT_PAGE),
    action(Group::View, "shortcuts-toggle-pages", super::TOGGLE_PAGES),
    action(Group::View, "shortcuts-toggle-tools", super::TOGGLE_TOOLS),
    action(Group::View, "shortcuts-fullscreen", super::FULLSCREEN),
    canvas("shortcuts-page-down", Nav::PageDown),
    canvas("shortcuts-page-up", Nav::PageUp),
    canvas("shortcuts-first-page", Nav::FirstPage),
    canvas("shortcuts-last-page", Nav::LastPage),
    canvas("shortcuts-scroll-up", Nav::LineUp),
    canvas("shortcuts-scroll-down", Nav::LineDown),
    canvas("shortcuts-scroll-left", Nav::Left),
    canvas("shortcuts-scroll-right", Nav::Right),
    Row {
        group: Group::Navigate,
        label_key: "shortcuts-pan",
        keys: Keys::Gesture("shortcuts-pan-keys"),
    },
    action(Group::Navigate, "shortcuts-next-tab", super::NEXT_TAB),
    action(
        Group::Navigate,
        "shortcuts-previous-tab",
        super::PREVIOUS_TAB,
    ),
    action(Group::Window, "palette-title", super::COMMAND_PALETTE),
    action(Group::Window, "home-shortcuts-title", super::SHORTCUTS_HELP),
];

#[cfg(test)]
mod tests {
    use gtk::prelude::*;

    use gtk::gdk;

    use super::*;
    use crate::app::canvas_keys;
    use crate::app::i18n::{has_message, Language};
    use crate::app::shortcuts;
    use crate::app::ui_tests::built_ui;

    /// A key as GTK names it: the key value and the modifiers held with it.
    type Chord = (gdk::Key, gdk::ModifierType);

    /// One accelerator in GTK syntax as a [`Chord`], letters folded to lower
    /// case so `<Control>Z` and `<Control>z` are one key.
    fn chord(accel: &str) -> Option<Chord> {
        let (key, modifiers) = gtk::accelerator_parse(accel)?;
        Some((key.to_lower(), modifiers))
    }

    /// Every pair of rows in `rows` that answer to the same chord, by label.
    /// Empty for a table that is sound: one key, one meaning.
    fn conflicts(rows: &[Row]) -> Vec<(&'static str, &'static str, Chord)> {
        let keyed: Vec<(&'static str, Chord)> = rows
            .iter()
            .flat_map(|row| chords(row).into_iter().map(|chord| (row.label_key, chord)))
            .collect();
        let mut found = Vec::new();
        for (index, (label, chord)) in keyed.iter().enumerate() {
            for (other, other_chord) in &keyed[index + 1..] {
                if chord == other_chord {
                    found.push((*label, *other, *chord));
                }
            }
        }
        found
    }

    /// Every chord `row` answers to, and nothing for a gesture.
    fn chords(row: &Row) -> Vec<Chord> {
        match row.keys {
            Keys::Action(shortcut) => shortcut.accels().into_iter().filter_map(chord).collect(),
            Keys::Canvas(nav) => canvas_keys::BINDINGS
                .iter()
                .filter(|binding| binding.nav == nav)
                .map(|binding| {
                    let modifiers = if binding.control {
                        gdk::ModifierType::CONTROL_MASK
                    } else {
                        gdk::ModifierType::empty()
                    };
                    (binding.key, modifiers)
                })
                .collect(),
            Keys::Gesture(_) => Vec::new(),
        }
    }

    #[test]
    fn every_row_and_every_group_reads_in_english_and_spanish() {
        for language in [Language::English, Language::Spanish] {
            for group in Group::ALL {
                assert!(has_message(language, group.title_key()), "{group:?}");
            }
            for row in TABLE {
                assert!(
                    has_message(language, row.label_key),
                    "{language:?} has no {:?}",
                    row.label_key
                );
                if let Keys::Gesture(keys) = row.keys {
                    assert!(has_message(language, keys), "{language:?} has no {keys:?}");
                }
            }
        }
    }

    #[test]
    fn every_group_has_a_row_and_every_row_a_listed_group() {
        for group in Group::ALL {
            assert!(TABLE.iter().any(|row| row.group == group), "{group:?}");
        }
        for row in TABLE {
            assert!(Group::ALL.contains(&row.group), "{row:?}");
        }
    }

    #[test]
    fn an_action_is_listed_once() {
        let actions: Vec<&str> = TABLE
            .iter()
            .filter_map(|row| match row.keys {
                Keys::Action(shortcut) => Some(shortcut.action),
                _ => None,
            })
            .collect();
        for (index, action) in actions.iter().enumerate() {
            assert!(!actions[index + 1..].contains(action), "{action}");
        }
    }

    #[gtk::test]
    fn gtk_ui_every_accelerator_in_the_table_parses() {
        for row in TABLE {
            if let Keys::Action(shortcut) = row.keys {
                for accel in shortcut.accels() {
                    assert!(chord(accel).is_some(), "{accel} of {}", shortcut.action);
                }
            }
        }
    }

    /// The conflict check: one key, one meaning — across the accelerators,
    /// their alternates and the canvas keys together.
    #[gtk::test]
    fn gtk_ui_no_two_shortcuts_answer_to_the_same_key() {
        assert_eq!(conflicts(TABLE), Vec::new());
    }

    /// The check has teeth: the same key on two rows is reported.
    #[gtk::test]
    fn gtk_ui_a_key_given_to_two_rows_is_reported() {
        let clash = Shortcut {
            action: "clash",
            accel: "<Control><Shift>Z",
            also: &["Page_Down"],
        };
        let rows = [
            action(Group::Edit, "editor-redo", shortcuts::REDO),
            canvas("shortcuts-page-down", Nav::PageDown),
            action(Group::Edit, "shortcuts-cut", clash),
        ];

        let found: Vec<(&str, &str)> = conflicts(&rows)
            .into_iter()
            .map(|(first, second, _)| (first, second))
            .collect();

        assert_eq!(
            found,
            [
                ("editor-redo", "shortcuts-cut"),
                ("shortcuts-page-down", "shortcuts-cut")
            ]
        );
    }

    fn sorted(mut chords: Vec<Chord>) -> Vec<String> {
        chords.dedup();
        let mut names: Vec<String> = chords
            .into_iter()
            .map(|(key, modifiers)| gtk::accelerator_name(key, modifiers).to_string())
            .collect();
        names.sort();
        names
    }

    /// The table cannot drift from the window: every accelerator GTK has
    /// bound is a row here with exactly those keys, and every action row is
    /// bound. A shortcut installed without a row, or a row whose key was
    /// never installed, fails here.
    #[gtk::test]
    fn gtk_ui_the_table_is_exactly_what_the_window_has_bound() {
        let built = built_ui();
        let application = built
            .window
            .application()
            .expect("the window's application");

        let mut bound = 0;
        for detailed in application.list_action_descriptions() {
            let accels = application.accels_for_action(&detailed);
            if accels.is_empty() {
                continue;
            }
            bound += 1;
            let row = TABLE
                .iter()
                .find(|row| {
                    matches!(row.keys, Keys::Action(shortcut)
                        if shortcut.detailed_action() == detailed.as_str())
                })
                .unwrap_or_else(|| panic!("{detailed} is bound to {accels:?} and has no row"));
            let installed: Vec<Chord> = accels.iter().filter_map(|accel| chord(accel)).collect();
            assert_eq!(sorted(installed), sorted(chords(row)), "{detailed}");
        }

        let rows = TABLE
            .iter()
            .filter(|row| matches!(row.keys, Keys::Action(_)))
            .count();
        assert_eq!(bound, rows, "every action row is bound in the window");
        for row in TABLE {
            if let Keys::Action(shortcut) = row.keys {
                assert!(
                    built.window.lookup_action(shortcut.action).is_some(),
                    "win.{} has a row and no action",
                    shortcut.action
                );
            }
        }
        built.window.close();
    }

    /// The keys a `GtkText` or a `GtkTextView` binds for itself. A window
    /// accelerator on one of them is taken before the field sees it.
    const TEXT_FIELD_KEYS: [&str; 40] = [
        "<Control>a",
        "<Control>c",
        "<Control>v",
        "<Control>x",
        "<Control>z",
        "<Control><Shift>z",
        "<Control>y",
        "<Control>slash",
        "<Control>backslash",
        "<Control><Shift>a",
        "BackSpace",
        "<Control>BackSpace",
        "<Shift>BackSpace",
        "Delete",
        "<Control>Delete",
        "<Shift>Delete",
        "Insert",
        "<Control>Insert",
        "<Shift>Insert",
        "Left",
        "Right",
        "Up",
        "Down",
        "<Control>Left",
        "<Control>Right",
        "<Shift>Left",
        "<Shift>Right",
        "Home",
        "End",
        "<Control>Home",
        "<Control>End",
        "Page_Up",
        "Page_Down",
        "Return",
        "KP_Enter",
        "Tab",
        "<Control>Tab",
        "<Control>period",
        "<Control>semicolon",
        "Menu",
    ];

    /// The accelerators that land on a text field's own key, each on
    /// purpose:
    ///
    /// * cut, copy, paste, undo and redo are handed back to the two search
    ///   fields while one has the cursor
    ///   (`shortcuts::connect_search_field_handoff`), and act on the page
    ///   from every other field by design;
    /// * Delete's action is off while any text field has the focus
    ///   (`annotations::update_annotation_controls`);
    /// * Ctrl+Tab leaves a text view in GTK; here it leaves the document for
    ///   the next tab, after committing the open editor.
    ///
    /// Anything else showing up here is a new accelerator that takes a key
    /// out of every text field in the window.
    const TAKEN_ON_PURPOSE: [&str; 8] = [
        "<Control>c",
        "<Control>v",
        "<Control>x",
        "<Control>z",
        "<Control><Shift>z",
        "<Control>y",
        "Delete",
        "<Control>Tab",
    ];

    #[gtk::test]
    fn gtk_ui_only_the_listed_accelerators_take_a_key_from_text_fields() {
        let names = |accels: &[&str]| -> Vec<String> {
            sorted(accels.iter().filter_map(|accel| chord(accel)).collect())
        };
        let field_keys: Vec<Chord> = TEXT_FIELD_KEYS
            .iter()
            .filter_map(|accel| chord(accel))
            .collect();
        assert_eq!(field_keys.len(), TEXT_FIELD_KEYS.len(), "every key parses");

        let taken: Vec<Chord> = TABLE
            .iter()
            .filter(|row| matches!(row.keys, Keys::Action(_)))
            .flat_map(chords)
            .filter(|chord| field_keys.contains(chord))
            .collect();

        assert_eq!(sorted(taken), names(&TAKEN_ON_PURPOSE));
    }

    /// The canvas keys are all a text field's too, which is why they are
    /// not accelerators: with the focus in a field, none is taken.
    #[test]
    fn a_text_field_keeps_every_canvas_key() {
        let in_a_field = canvas_keys::Context {
            document_on_screen: true,
            focus: canvas_keys::Focus::Keyed,
            ..canvas_keys::Context::default()
        };
        for binding in canvas_keys::BINDINGS {
            assert!(!canvas_keys::takes(binding.nav, in_a_field), "{binding:?}");
        }
    }
}
