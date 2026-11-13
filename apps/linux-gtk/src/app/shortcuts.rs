//! The window shortcuts, written down once (B27 decision 7).
//!
//! Each [`Shortcut`] pairs a `win.*` action name with its accelerator. The
//! code that registers the action (`connect_standard_shortcuts`,
//! `annotations::connect_history_shortcuts`) and the command palette's
//! catalog both read these constants, so the key the palette shows is the
//! key GTK listens for. Home's shortcut card still writes its own text until
//! T-232 moves it onto this table.

use gtk::prelude::*;

/// One window action and the accelerator that triggers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
    /// The action's name inside the window's `win` group, e.g. `"open"`.
    pub(crate) action: &'static str,
    /// The accelerator in GTK's syntax, e.g. `"<Control>o"`.
    pub(crate) accel: &'static str,
}

pub(crate) const OPEN: Shortcut = Shortcut {
    action: "open",
    accel: "<Control>o",
};
pub(crate) const SAVE: Shortcut = Shortcut {
    action: "save",
    accel: "<Control>s",
};
pub(crate) const PRINT: Shortcut = Shortcut {
    action: "print",
    accel: "<Control>p",
};
pub(crate) const FIND: Shortcut = Shortcut {
    action: "find",
    accel: "<Control>f",
};
pub(crate) const NEW: Shortcut = Shortcut {
    action: "new",
    accel: "<Control>n",
};
pub(crate) const UNDO: Shortcut = Shortcut {
    action: "undo",
    accel: "<Control>z",
};
pub(crate) const REDO: Shortcut = Shortcut {
    action: "redo",
    accel: "<Control>y",
};
/// Opens the command palette (B27).
pub(crate) const COMMAND_PALETTE: Shortcut = Shortcut {
    action: "command-palette",
    accel: "<Control>k",
};

impl Shortcut {
    /// The action's detailed name, `win.<action>`.
    pub(crate) fn detailed_action(self) -> String {
        format!("win.{}", self.action)
    }

    /// Binds the accelerator to the window action on `application`.
    pub(crate) fn install(self, application: &gtk::Application) {
        application.set_accels_for_action(&self.detailed_action(), &[self.accel]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Shortcut; 8] = [OPEN, SAVE, PRINT, FIND, NEW, UNDO, REDO, COMMAND_PALETTE];

    #[test]
    fn every_action_and_accelerator_is_used_once() {
        for (index, shortcut) in ALL.iter().enumerate() {
            for other in &ALL[index + 1..] {
                assert_ne!(shortcut.action, other.action);
                assert_ne!(shortcut.accel, other.accel);
            }
        }
    }

    #[test]
    fn the_detailed_name_is_in_the_window_group() {
        assert_eq!(OPEN.detailed_action(), "win.open");
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
