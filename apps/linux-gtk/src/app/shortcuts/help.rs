//! The keyboard shortcuts window (F1, Ctrl+?): [`super::table`] printed row
//! for row, so what it lists is what the window answers to.
//!
//! A plain window of its own rather than `GtkShortcutsWindow`: that widget
//! is filled from a `.ui` description, and the calls that build one in code
//! arrived in GTK 4.14, past the 4.10 this crate builds against. Nothing in
//! it is interactive, so a list of labels is the whole job.
//!
//! Not modal — it is something to read beside the document, and a modal
//! window would hold the tabs still (`tabs::busy_refusal`).

use gtk::prelude::*;
use gtk::{
    gdk, gio, glib, Box as GtkBox, EventControllerKey, Label, Orientation, PolicyType,
    ScrolledWindow,
};

use super::table::{Group, Keys, Row, TABLE};
use super::Shortcut;
use crate::app::canvas_keys;
use crate::app::i18n::tr;

/// What marks the window among the main window's transients, so a second
/// F1 raises the one that is open instead of stacking another.
const WINDOW_NAME: &str = "vitela-shortcuts";

/// The look, from the shared palette (`theme`).
pub(crate) const SHORTCUTS_CSS: &str = r#"
.shortcuts-list {
  padding: 16px 20px;
}

.shortcuts-group-title {
  font-weight: 700;
  margin-top: 6px;
  color: @vitela_text;
}

.shortcuts-row {
  padding: 3px 0;
}

.shortcuts-keys {
  font-family: monospace;
  color: @vitela_text_muted;
}
"#;

/// One printed row: what the shortcut does, and its keys as the user reads
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) label: String,
    pub(crate) keys: Vec<String>,
}

/// Whether an accelerator is printed. The keypad's twin of a key is bound
/// and not listed: it is the same key to the person pressing it.
fn printed(accel: &str) -> bool {
    !accel.contains("KP_")
}

fn action_keys(shortcut: Shortcut) -> Vec<String> {
    shortcut
        .accels()
        .into_iter()
        .filter(|accel| printed(accel))
        .map(|accel| super::accel_label(accel).unwrap_or_else(|| accel.to_owned()))
        .collect()
}

fn canvas_key_labels(nav: canvas_keys::Nav) -> Vec<String> {
    canvas_keys::BINDINGS
        .iter()
        .filter(|binding| binding.nav == nav && binding.listed)
        .map(|binding| {
            let modifiers = if binding.control {
                gdk::ModifierType::CONTROL_MASK
            } else {
                gdk::ModifierType::empty()
            };
            super::key_label(binding.key, modifiers)
        })
        .collect()
}

fn line(row: &Row) -> Line {
    Line {
        label: tr(row.label_key),
        keys: match row.keys {
            Keys::Action(shortcut) => action_keys(shortcut),
            Keys::Canvas(nav) => canvas_key_labels(nav),
            Keys::Gesture(keys) => vec![tr(keys)],
        },
    }
}

/// The table as it is printed: each group's heading and its lines, in the
/// table's order.
pub(crate) fn sections() -> Vec<(String, Vec<Line>)> {
    Group::ALL
        .into_iter()
        .map(|group| {
            let lines = TABLE
                .iter()
                .filter(|row| row.group == group)
                .map(line)
                .collect();
            (tr(group.title_key()), lines)
        })
        .collect()
}

/// Alternatives for one command, as they read on a row.
const KEY_SEPARATOR: &str = "   /   ";

fn build_list() -> GtkBox {
    let list = GtkBox::new(Orientation::Vertical, 2);
    list.add_css_class("shortcuts-list");
    for (title, lines) in sections() {
        let heading = Label::new(Some(&title));
        heading.set_xalign(0.0);
        heading.add_css_class("shortcuts-group-title");
        heading.set_accessible_role(gtk::AccessibleRole::Heading);
        list.append(&heading);
        for line in lines {
            let row = GtkBox::new(Orientation::Horizontal, 24);
            row.add_css_class("shortcuts-row");
            let label = Label::new(Some(&line.label));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.set_wrap(true);
            let keys = Label::new(Some(&line.keys.join(KEY_SEPARATOR)));
            keys.set_xalign(1.0);
            keys.add_css_class("shortcuts-keys");
            row.append(&label);
            row.append(&keys);
            list.append(&row);
        }
    }
    list
}

/// The shortcuts window that is open over `window`, if one is.
pub(crate) fn open_window(window: &gtk::ApplicationWindow) -> Option<gtk::Window> {
    let owner: &gtk::Window = window.upcast_ref();
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
        .find(|candidate| {
            candidate.widget_name() == WINDOW_NAME
                && candidate.transient_for().as_ref() == Some(owner)
        })
}

/// Opens the shortcuts window over `window`, or raises the one already open.
pub(crate) fn show(window: &gtk::ApplicationWindow) {
    if let Some(open) = open_window(window) {
        open.present();
        return;
    }
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .child(&build_list())
        .build();
    let help = gtk::Window::builder()
        .title(tr("home-shortcuts-title"))
        .transient_for(window)
        .destroy_with_parent(true)
        .default_width(460)
        .default_height(620)
        .child(&scroll)
        .build();
    help.set_widget_name(WINDOW_NAME);

    // Escape closes it, and so do the two keys that opened it: the window
    // has no `win.*` actions of its own for them to reach.
    let keys = EventControllerKey::new();
    keys.connect_key_pressed({
        let help = help.downgrade();
        move |_, key, _, modifiers| {
            let closes = key == gdk::Key::Escape
                || key == gdk::Key::F1
                || (key == gdk::Key::question
                    && modifiers.contains(gdk::ModifierType::CONTROL_MASK));
            if !closes {
                return glib::Propagation::Proceed;
            }
            if let Some(help) = help.upgrade() {
                help.close();
            }
            glib::Propagation::Stop
        }
    });
    help.add_controller(keys);
    help.present();
}

/// Installs `win.shortcuts` and binds F1 and Ctrl+? to it.
pub(crate) fn connect(application: &gtk::Application, window: &gtk::ApplicationWindow) {
    let action = gio::SimpleAction::new(super::SHORTCUTS_HELP.action, None);
    action.connect_activate({
        let window = window.clone();
        move |_, _| show(&window)
    });
    window.add_action(&action);
    super::SHORTCUTS_HELP.install(application);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::shortcuts;
    use crate::app::test_support::pump;
    use crate::app::ui_tests::built_ui;

    fn labels(root: &gtk::Widget, found: &mut Vec<String>) {
        if let Some(label) = root.downcast_ref::<Label>() {
            found.push(label.text().to_string());
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            labels(&widget, found);
            child = widget.next_sibling();
        }
    }

    /// Generated from the table, so it cannot list a key the window does
    /// not answer to or leave one out: one line per row, in order.
    #[gtk::test]
    fn gtk_ui_the_window_prints_one_line_for_every_row_of_the_table() {
        let printed: Vec<Line> = sections()
            .into_iter()
            .flat_map(|(_, lines)| lines)
            .collect();

        assert_eq!(printed.len(), TABLE.len());
        for line in &printed {
            assert!(!line.label.is_empty());
            assert!(
                !line.keys.is_empty() && line.keys.iter().all(|key| !key.is_empty()),
                "{line:?}"
            );
        }
        let mut drawn = Vec::new();
        labels(build_list().upcast_ref(), &mut drawn);
        // A heading per group, and a label and its keys per row.
        assert_eq!(drawn.len(), Group::ALL.len() + 2 * TABLE.len());
    }

    #[gtk::test]
    fn gtk_ui_keys_read_as_gtk_prints_them_with_every_alternative() {
        let find = |label_key: &str| -> Vec<String> {
            let label = tr(label_key);
            sections()
                .into_iter()
                .flat_map(|(_, lines)| lines)
                .find(|line| line.label == label)
                .unwrap_or_else(|| panic!("no line for {label_key}"))
                .keys
        };

        assert_eq!(find("editor-save"), ["Ctrl+S"]);
        assert_eq!(find("editor-redo"), ["Ctrl+Y", "Ctrl+Shift+Z"]);
        assert_eq!(find("editor-find-next"), ["F3", "Ctrl+G"]);
        assert_eq!(find("shortcuts-page-down"), ["Page Down"]);
        assert_eq!(find("shortcuts-first-page"), ["Home", "Ctrl+Home"]);
        assert_eq!(find("shortcuts-pan"), [tr("shortcuts-pan-keys")]);
    }

    /// The keypad's twins are bound but not printed.
    #[gtk::test]
    fn gtk_ui_keypad_twins_are_not_printed() {
        assert_eq!(action_keys(shortcuts::ZOOM_IN), ["Ctrl++", "Ctrl+="]);
        assert_eq!(
            action_keys(shortcuts::PREVIOUS_TAB),
            ["Ctrl+Shift+Tab", "Ctrl+Page Up"]
        );
    }

    #[gtk::test]
    fn gtk_ui_the_action_opens_one_window_and_a_second_press_does_not_stack_another() {
        let built = built_ui();
        assert!(open_window(&built.window).is_none());

        gtk::prelude::ActionGroupExt::activate_action(&built.window, "shortcuts", None);
        let help = open_window(&built.window).expect("the shortcuts window is up");
        assert_eq!(help.title().as_deref(), Some("Keyboard shortcuts"));

        gtk::prelude::ActionGroupExt::activate_action(&built.window, "shortcuts", None);
        let open = gtk::Window::list_toplevels()
            .into_iter()
            .filter(|widget| widget.widget_name() == WINDOW_NAME)
            .count();
        assert_eq!(open, 1);

        help.destroy();
        built.window.close();
        pump();
    }

    #[gtk::test]
    fn gtk_ui_the_window_speaks_spanish() {
        let _spanish = crate::app::i18n::speak(crate::app::i18n::Language::Spanish);

        let titles: Vec<String> = sections().into_iter().map(|(title, _)| title).collect();

        assert_eq!(
            titles,
            [
                "Archivo",
                "Edición",
                "Buscar",
                "Vista",
                "Navegación",
                "Ventana"
            ]
        );
    }
}
