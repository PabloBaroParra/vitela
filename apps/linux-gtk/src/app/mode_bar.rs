//! The mode bar across the top of the window: Home, Edit, Annotate,
//! Organize, Sign, Protect — one row of tabs that replaced the left app rail
//! and the tools panel's own tab strip.
//!
//! **The bar never stores which mode is current.** [`Mode::of_screen`]
//! derives it from the two stacks that actually decide what is on screen —
//! the window's `view_stack` and the tools panel's `tools_stack` — and
//! [`ModeBar::sync`] runs whenever either of them moves. Opening a file from
//! Home, Organize's back button, a preview refresh restoring its screen, a
//! tool armed before a document existed: none of them knows the bar exists,
//! and none of them has to. The rail kept a highlight of its own and had to
//! be told; that is how it came to mark Organize current after Organize had
//! refused to open.
//!
//! What each mode *does* is not defined here either: every document mode is
//! `home::tools::open_tool`, the same gesture as Home's tool tiles, so a mode
//! clicked with no document open still picks a file first and lands on its
//! page once that file is open.
//!
//! The icons are the shell's own (`icons`), never the desktop theme's — see
//! `shell`'s module header for why.

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, FlowBox, Label, Orientation, SelectionMode,
    ToggleButton,
};

use super::brand::build_brand_lockup;
use super::home::{self, EDITOR_PAGE, HOME_PAGE};
use super::i18n::tr;
use super::icons::{build_icon, Icon, NEUTRAL_TINT};
use super::organize::ORGANIZE_PAGE;
use super::state::{HomeTool, Viewer};
use super::tools_panel::{EDIT_PAGE, FILL_SIGN_PAGE};

/// Icon edge on a mode tab, sized against the label beside it.
const MODE_ICON_PX: i32 = 16;

pub(crate) const MODE_BAR_CSS: &str = r#"
.mode-bar {
  background: @vitela_surface;
  border-bottom: 1px solid @vitela_border;
  padding: 6px 12px;
}

.mode-bar .brand-lockup {
  padding: 2px;
  margin-right: 12px;
}

.mode-bar button {
  min-height: 26px;
  padding: 5px 14px;
  border: 1px solid transparent;
  border-radius: 999px;
  background: transparent;
  box-shadow: none;
  color: @vitela_text_secondary;
  transition: background-color 120ms ease, color 120ms ease;
}

.mode-bar button:hover,
.mode-bar button:focus-visible {
  background: @vitela_accent_soft;
}

/* The current mode: an accent pill, the same tokens the editor toolbar's
   `:checked` and the old rail's active item used. */
.mode-bar button:checked {
  background: @vitela_checked;
  border-color: @vitela_accent_border;
  color: @vitela_accent;
  font-weight: 700;
}

.mode-bar flowboxchild {
  padding: 0;
}
"#;

/// One section of the app, in bar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Home,
    Edit,
    Annotate,
    Organize,
    Sign,
    Protect,
}

impl Mode {
    pub(crate) const ALL: [Mode; 6] = [
        Mode::Home,
        Mode::Edit,
        Mode::Annotate,
        Mode::Organize,
        Mode::Sign,
        Mode::Protect,
    ];

    fn label_key(self) -> &'static str {
        match self {
            Mode::Home => "mode-home",
            Mode::Edit => "mode-edit",
            Mode::Annotate => "mode-annotate",
            Mode::Organize => "mode-organize",
            Mode::Sign => "mode-sign",
            Mode::Protect => "mode-protect",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Mode::Home => Icon::Home,
            Mode::Edit => Icon::Edit,
            Mode::Annotate => Icon::Annotate,
            Mode::Organize => Icon::Organize,
            Mode::Sign => Icon::Sign,
            Mode::Protect => Icon::Protect,
        }
    }

    /// The Home tool this mode opens, or `None` for Home itself, which is
    /// navigation only and never touches the open document.
    fn tool(self) -> Option<HomeTool> {
        match self {
            Mode::Home => None,
            Mode::Edit => Some(HomeTool::Edit),
            Mode::Annotate => Some(HomeTool::Annotate),
            Mode::Organize => Some(HomeTool::Organize),
            Mode::Sign => Some(HomeTool::Sign),
            Mode::Protect => Some(HomeTool::Protect),
        }
    }

    /// Whether this mode is a place the window can be in. Protect is not: it
    /// opens a dialog over whatever screen is up, so it is a plain button and
    /// never the checked tab.
    fn is_place(self) -> bool {
        self != Mode::Protect
    }

    /// The mode the window is in, read from the visible page of `view_stack`
    /// (`view`) and of the tools panel's stack (`tools`). The Annotate page
    /// is the tools panel's default, so any editor page that is not Edit or
    /// Fill & Sign reads as Annotate. `None` only while the window is still
    /// being built and `view_stack` has no page it knows.
    pub(crate) fn of_screen(view: Option<&str>, tools: Option<&str>) -> Option<Mode> {
        match view? {
            HOME_PAGE => Some(Mode::Home),
            ORGANIZE_PAGE => Some(Mode::Organize),
            EDITOR_PAGE => Some(match tools {
                Some(EDIT_PAGE) => Mode::Edit,
                Some(FILL_SIGN_PAGE) => Mode::Sign,
                _ => Mode::Annotate,
            }),
            _ => None,
        }
    }
}

/// The bar and its buttons. Cheap to clone: the button list is shared.
#[derive(Clone)]
pub(crate) struct ModeBar {
    /// The row `build_ui` puts above the view stack.
    pub(crate) root: GtkBox,
    /// One button per [`Mode::ALL`] entry, in order. Place modes are
    /// `ToggleButton`s (a `Button` subclass), Protect a plain `Button`.
    buttons: Rc<Vec<(Mode, Button)>>,
}

impl ModeBar {
    /// The button for `mode`.
    #[cfg(test)]
    pub(crate) fn button(&self, mode: Mode) -> &Button {
        self.buttons
            .iter()
            .find(|(candidate, _)| *candidate == mode)
            .map(|(_, button)| button)
            .expect("the bar has a button for every mode")
    }

    /// The mode whose tab is checked, if exactly one is.
    #[cfg(test)]
    pub(crate) fn checked(&self) -> Option<Mode> {
        let checked: Vec<Mode> = self
            .buttons
            .iter()
            .filter(|(_, button)| {
                button
                    .downcast_ref::<ToggleButton>()
                    .is_some_and(ToggleButton::is_active)
            })
            .map(|(mode, _)| *mode)
            .collect();
        match checked.as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }

    /// Checks the tab for the screen `viewer` is showing and clears every
    /// other one. Never changes a page: it only reports what the stacks
    /// settled on, so the bar cannot come to disagree with the screen.
    pub(crate) fn sync(&self, viewer: &Viewer) {
        let current = Mode::of_screen(
            viewer.view_stack.visible_child_name().as_deref(),
            viewer.tools_stack.visible_child_name().as_deref(),
        );
        for (mode, button) in self.buttons.iter() {
            if let Some(toggle) = button.downcast_ref::<ToggleButton>() {
                toggle.set_active(current == Some(*mode));
            }
        }
    }
}

/// Builds the bar, unwired. [`connect`] wires it once a `Viewer` exists.
pub(crate) fn build_mode_bar() -> ModeBar {
    let root = GtkBox::new(Orientation::Horizontal, 0);
    root.add_css_class("mode-bar");
    root.update_property(&[gtk::accessible::Property::Label(&tr("mode-bar-sections"))]);
    // The real mark, and the only place the shell identifies itself — it
    // moved here from the rail, the one piece of chrome on screen for every
    // view.
    root.append(&build_brand_lockup());

    // A `FlowBox`, not a row: like the editor toolbar and the tab strip this
    // replaced, a row of labelled controls in this shell has to be able to
    // wrap, or it sets the window's minimum width.
    let tabs = FlowBox::new();
    tabs.set_selection_mode(SelectionMode::None);
    tabs.set_homogeneous(false);
    // Packed at the start at their natural width, like a tab row; left to
    // fill, a `FlowBox` spreads six pills across the whole window.
    tabs.set_hexpand(true);
    tabs.set_halign(gtk::Align::Start);
    tabs.set_valign(gtk::Align::Center);
    tabs.set_row_spacing(4);
    tabs.set_column_spacing(4);
    tabs.set_max_children_per_line(Mode::ALL.len() as u32);
    root.append(&tabs);

    let buttons = Mode::ALL
        .iter()
        .map(|mode| {
            let button = mode_button(*mode);
            tabs.append(&button);
            (*mode, button)
        })
        .collect();

    ModeBar {
        root,
        buttons: Rc::new(buttons),
    }
}

/// Wires every mode to what it opens, and keeps the checked tab following
/// both stacks from now on.
pub(crate) fn connect(bar: &ModeBar, window: &ApplicationWindow, viewer: &Viewer) {
    for (mode, button) in bar.buttons.iter() {
        button.connect_clicked({
            let bar = bar.clone();
            let window = window.clone();
            let viewer = viewer.clone();
            let mode = *mode;
            move |_| {
                match mode.tool() {
                    // Navigation only: the open document, if any, stays open
                    // behind Home exactly as it was.
                    None => home::show_home(&viewer),
                    Some(tool) => home::tools::open_tool(&window, &viewer, tool),
                }
                // Always, not only through the stack notifications below: a
                // click that moved no page (the mode already current, a file
                // chooser opened instead, Organize refusing the document,
                // Protect's dialog) has still toggled the button, and the
                // bar has to say again what the screen says.
                bar.sync(&viewer);
            }
        });
    }
    for stack in [&viewer.view_stack, &viewer.tools_stack] {
        stack.connect_visible_child_notify({
            let bar = bar.clone();
            let viewer = viewer.clone();
            move |_| bar.sync(&viewer)
        });
    }
    bar.sync(viewer);
}

fn mode_button(mode: Mode) -> Button {
    let label = tr(mode.label_key());
    let content = GtkBox::new(Orientation::Horizontal, 6);
    content.set_halign(gtk::Align::Center);
    content.append(&build_icon(mode.icon(), MODE_ICON_PX, NEUTRAL_TINT));
    content.append(&Label::new(Some(&label)));

    let button: Button = if mode.is_place() {
        ToggleButton::new().upcast()
    } else {
        Button::new()
    };
    button.set_child(Some(&content));
    button.set_tooltip_text(Some(&label));
    // A button given a custom child has no label of its own for the
    // accessibility layer to fall back on, so it is stated.
    button.update_property(&[gtk::accessible::Property::Label(&label)]);
    button
}

#[cfg(test)]
mod tests;
