//! The top editor toolbar: open/save/print, history, the page readout, zoom,
//! and find — everything that acts on the document as a whole, above the
//! three-column shell.
//!
//! Text-only like the rest of this shell (see `shell`'s module docs for why
//! no icon-theme lookup is allowed to be load-bearing here).
//!
//! ## Why a `FlowBox` of groups and not one horizontal `GtkBox`
//!
//! This is the same lesson `annotations::add_annotation_toolbar` already
//! carries, applied to the bar it never covered. A horizontal `GtkBox`
//! reports the *sum* of its children's widths as its own **minimum**, and a
//! minimum propagates all the way up: it becomes the window's minimum width.
//! Fifteen labelled controls in one box asked for roughly 1300px before the
//! canvas, the page rail, and the two side panels were counted at all, so on
//! any window narrower than that — a 1366px laptop, a tiled/maximized window
//! the compositor sizes for us, or simply the 1000px this shell opens at —
//! GTK had no choice but to clip, and controls at the end of the row silently
//! went off-screen.
//!
//! A `FlowBox` reports only its widest single child as a minimum and wraps
//! the rest onto more rows. The children here are *groups*, not individual
//! buttons, so what wraps is "the whole zoom cluster", never "Zoom in alone,
//! two rows away from Zoom out". The widest group is what now sets the floor,
//! which is a few hundred pixels rather than the whole bar.
//!
//! Do not flatten these groups back into one box, and do not "fix" a future
//! overflow by widening the window.

use gtk::prelude::*;
use gtk::{
    gio, AccessibleRole, Box as GtkBox, Button, Entry, FlowBox, Label, MenuButton, Orientation,
    SelectionMode, ToggleButton,
};

/// How many groups [`build_editor_toolbar`] appends. `FlowBox` defaults to a
/// cap of seven children per line, which would force a wrap even on a window
/// wide enough for all of them; stating the real count lets the bar use one
/// row whenever it fits and wrap only when it genuinely has to.
const GROUP_COUNT: u32 = 8;

/// How wide the find entry asks to be, in characters. Deliberately a modest
/// fixed request instead of the `hexpand(true)` this entry used to carry: an
/// entry that grows without bound is the single widest thing in the bar, and
/// under `FlowBox` that width would become the minimum every other group has
/// to fit beside.
const FIND_ENTRY_CHARS: i32 = 14;

/// The toolbar widgets `build_ui` still has to wire up after construction.
///
/// Undo and Redo are built and appended like the rest but are absent here on
/// purpose: they are bound to `win.undo`/`win.redo` inside this module, so no
/// caller downstream ever needs to address them again (the same reasoning as
/// `shell::AppRail`'s omitted rail items).
pub(crate) struct EditorToolbar {
    /// The bar itself, for `build_ui` to place.
    pub(crate) root: FlowBox,
    pub(crate) open: Button,
    /// Carries the `sample.*` actions the Open sample menu items name.
    pub(crate) sample_actions: gio::SimpleActionGroup,
    pub(crate) print: Button,
    pub(crate) save: Button,
    pub(crate) page_indicator: Label,
    pub(crate) zoom_out: Button,
    pub(crate) zoom_label: Label,
    pub(crate) zoom_in: Button,
    pub(crate) fit_width: Button,
    pub(crate) fit_page: Button,
    /// Whether the page-thumbnail column is on screen. The *state* of the
    /// panel lives here rather than in the panel itself, so a drag that
    /// collapses it and a click that hides it are the same fact — see
    /// `build_ui`'s `connect_panel_collapse`.
    pub(crate) show_pages: ToggleButton,
    /// Twin of [`Self::show_pages`] for the tools column.
    pub(crate) show_tools: ToggleButton,
    pub(crate) search_entry: Entry,
    pub(crate) find_previous: Button,
    pub(crate) find_next: Button,
}

pub(crate) fn build_editor_toolbar() -> EditorToolbar {
    let root = FlowBox::new();
    root.add_css_class("editor-toolbar");
    root.set_selection_mode(SelectionMode::None);
    root.set_homogeneous(false);
    // Wider between groups than within one, so the grouping reads as grouping
    // without needing separator lines that would land in the wrong place the
    // moment a row wraps.
    root.set_column_spacing(12);
    root.set_row_spacing(6);
    root.set_max_children_per_line(GROUP_COUNT);

    // --- documents in ------------------------------------------------------
    let documents = group(&root, "Document");
    let open = Button::with_label("Open PDF");
    documents.append(&open);
    // A `gio::Menu` bound through `menu-model` (rather than a hand-built
    // `Popover` of `Button`s) so GTK owns the popup/dismiss/keyboard-nav
    // state machine — a manually-toggled Popover left the button needing a
    // second click to reopen after a selection.
    let sample_button = MenuButton::builder().label("Open sample").build();
    let sample_actions = gio::SimpleActionGroup::new();
    let sample_menu = gio::Menu::new();
    sample_menu.append(Some("Vitela sample"), Some("sample.plain"));
    sample_menu.append(
        Some("AES-128 sample (user-aes-pass)"),
        Some("sample.aes128"),
    );
    sample_menu.append(
        Some("RC4-128 sample (user-rc4-pass)"),
        Some("sample.rc4128"),
    );
    sample_button.set_menu_model(Some(&sample_menu));
    sample_button.insert_action_group("sample", Some(&sample_actions));
    documents.append(&sample_button);

    // --- documents out -----------------------------------------------------
    let output = group(&root, "Output");
    let save = Button::with_label("Save as");
    save.set_sensitive(false);
    let print = Button::with_label("Print");
    print.set_sensitive(false);
    output.append(&save);
    output.append(&print);

    // --- history -----------------------------------------------------------
    // Bound to the actions rather than wired to a handler, so GTK greys them
    // out whenever `win.undo`/`win.redo` are disabled. Offering Redo when the
    // history has nothing to redo is a promise the toolbar cannot keep: the
    // click is accepted, nothing changes, and the status line has to explain
    // it after the fact. The accelerators were already gated on `can_undo`/
    // `can_redo`; this is the same gate reaching the buttons, from the same
    // source, instead of a second copy of the rule kept in step by hand.
    let history = group(&root, "History");
    let undo = Button::with_label("Undo");
    undo.set_action_name(Some("win.undo"));
    let redo = Button::with_label("Redo");
    redo.set_action_name(Some("win.redo"));
    history.append(&undo);
    history.append(&redo);

    // --- where you are -----------------------------------------------------
    // "3 / 12": a compact readout of `last_visible`, kept up here rather than
    // duplicating the descriptive "Showing pages X-Y of N." status line — see
    // `render::update_viewport`, the one place both are set.
    let position = group(&root, "Position");
    let page_indicator = Label::new(Some("\u{2013}"));
    page_indicator.add_css_class("page-indicator");
    position.append(&page_indicator);

    // --- zoom --------------------------------------------------------------
    // Its own group, separate from the fit commands below: stepping the zoom
    // and choosing a fit mode are different gestures, and keeping them apart
    // halves the width of the widest group the bar has to guarantee room for.
    let zoom = group(&root, "Zoom");
    let zoom_out = Button::with_label("Zoom out");
    // The effective zoom at the current scroll position — see
    // `layout::current_zoom_factor`, which this and `step_zoom`'s ladder both
    // read from so the two can never disagree about "current".
    let zoom_label = Label::new(Some("100%"));
    zoom_label.add_css_class("zoom-indicator");
    let zoom_in = Button::with_label("Zoom in");
    zoom.append(&zoom_out);
    zoom.append(&zoom_label);
    zoom.append(&zoom_in);

    let fit = group(&root, "Fit");
    let fit_width = Button::with_label("Fit width");
    let fit_page = Button::with_label("Fit page");
    fit.append(&fit_width);
    fit.append(&fit_page);

    // --- the side columns --------------------------------------------------
    // Toggles, not commands: a side column is either on screen or it is not,
    // and that is exactly what a `ToggleButton` says. They start pressed
    // because both columns start open. `build_ui` connects them to the
    // panels and to the divider drags that can also collapse a column.
    let panels = group(&root, "Panels");
    let show_pages = ToggleButton::with_label("Pages");
    show_pages.set_active(true);
    let show_tools = ToggleButton::with_label("Tools");
    show_tools.set_active(true);
    panels.append(&show_pages);
    panels.append(&show_tools);

    // --- find --------------------------------------------------------------
    let find = group(&root, "Find");
    // Exact, case-sensitive search: the same matcher `pdf-ffi` uses, so
    // this shell and the other platforms agree on what a match is.
    let search_entry = Entry::builder()
        .placeholder_text("Find in document")
        .width_chars(FIND_ENTRY_CHARS)
        .max_width_chars(FIND_ENTRY_CHARS)
        .build();
    search_entry.update_property(&[gtk::accessible::Property::Label("Search document")]);
    let find_previous = Button::with_label("Previous");
    let find_next = Button::with_label("Next");
    find_previous.set_sensitive(false);
    find_next.set_sensitive(false);
    find.append(&search_entry);
    find.append(&find_previous);
    find.append(&find_next);

    EditorToolbar {
        root,
        open,
        sample_actions,
        print,
        save,
        page_indicator,
        zoom_out,
        zoom_label,
        zoom_in,
        fit_width,
        fit_page,
        show_pages,
        show_tools,
        search_entry,
        find_previous,
        find_next,
    }
}

/// Appends one labelled group to `bar` and returns it for its controls to be
/// added to.
///
/// The `Group` accessible role plus a label is what turns the visual
/// clustering into something a screen reader can also hear: a plain `GtkBox`
/// has a generic role, and labelling a generic container announces a name
/// with no structure attached to it.
fn group(bar: &FlowBox, label: &str) -> GtkBox {
    let group = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .accessible_role(AccessibleRole::Group)
        .build();
    group.update_property(&[gtk::accessible::Property::Label(label)]);
    bar.append(&group);
    group
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole module exists for: the bar's *minimum* width is
    /// one group, not the sum of every control, so a window narrower than the
    /// full bar wraps it instead of clipping controls off the right edge.
    ///
    /// Asserted as a ratio rather than a pixel count on purpose — the actual
    /// numbers move with the theme's font, and what has to hold is the shape
    /// of the relationship, not a measurement taken on one machine.
    #[gtk::test]
    fn the_toolbar_minimum_width_is_one_group_not_the_whole_bar() {
        let toolbar = build_editor_toolbar();

        let (minimum, natural, _, _) = toolbar.root.measure(Orientation::Horizontal, -1);

        assert!(
            minimum * 2 <= natural,
            "toolbar minimum {minimum} is not meaningfully below its natural width {natural}; \
             it is behaving like a plain GtkBox and will clip instead of wrapping"
        );
    }

    /// A wrap must move whole groups. `FlowBox`'s default cap of seven
    /// children per line happens to equal the group count today, so a group
    /// added without raising [`GROUP_COUNT`] would start the bar off already
    /// wrapped on a window with room to spare.
    #[gtk::test]
    fn every_group_can_share_one_row_when_the_window_is_wide_enough() {
        let toolbar = build_editor_toolbar();

        let groups = std::iter::successors(toolbar.root.first_child(), |child| child.next_sibling())
            .count() as u32;

        assert_eq!(groups, GROUP_COUNT);
        assert!(toolbar.root.max_children_per_line() >= groups);
    }
}
