//! The top editor toolbar: open/save/print, history, the page readout, zoom,
//! and find — everything that acts on the document as a whole, above the
//! three-column shell.
//!
//! Bundled outline icons keep the command bar compact without depending on
//! the desktop icon theme. Labels remain available to assistive technology.
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
    Popover, SelectionMode, ToggleButton,
};

use super::icons::{build_icon, Icon, NEUTRAL_TINT};

/// How many groups [`build_editor_toolbar`] appends. `FlowBox` defaults to a
/// cap of seven children per line, which would force a wrap even on a window
/// wide enough for all of them; stating the real count lets the bar use one
/// row whenever it fits and wrap only when it genuinely has to.
const GROUP_COUNT: u32 = 8;

/// A compact search field keeps the popover usable on narrow windows.
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
    root.set_column_spacing(0);
    root.set_row_spacing(4);
    root.set_valign(gtk::Align::Start);
    root.set_max_children_per_line(GROUP_COUNT);

    // --- documents in ------------------------------------------------------
    let documents = group(&root, "Document");
    let open = icon_button("Open PDF", Icon::Files);
    open.set_child(Some(&button_content(Icon::Files, "Open", true)));
    open.set_tooltip_text(Some("Open PDF (Ctrl+O)"));
    documents.append(&open);
    // A `gio::Menu` bound through `menu-model` (rather than a hand-built
    // `Popover` of `Button`s) so GTK owns the popup/dismiss/keyboard-nav
    // state machine — a manually-toggled Popover left the button needing a
    // second click to reopen after a selection.
    let sample_button = MenuButton::new();
    sample_button.set_child(Some(&button_content(Icon::Sample, "Open sample", false)));
    sample_button.set_tooltip_text(Some("Open sample"));
    sample_button.update_property(&[gtk::accessible::Property::Label("Open sample")]);
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
    let save = icon_button("Save as", Icon::Save);
    save.set_tooltip_text(Some("Save as (Ctrl+S)"));
    save.set_sensitive(false);
    let print = icon_button("Print", Icon::Print);
    print.set_tooltip_text(Some("Print (Ctrl+P)"));
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
    let undo = icon_button("Undo", Icon::Undo);
    undo.set_action_name(Some("win.undo"));
    let redo = icon_button("Redo", Icon::Redo);
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
    page_indicator.set_tooltip_text(Some("Current page / total pages"));
    position.append(&page_indicator);

    // --- zoom --------------------------------------------------------------
    // Its own group, separate from the fit commands below: stepping the zoom
    // and choosing a fit mode are different gestures, and keeping them apart
    // halves the width of the widest group the bar has to guarantee room for.
    let zoom = group(&root, "Zoom");
    let zoom_out = icon_button("Zoom out", Icon::ZoomOut);
    // The effective zoom at the current scroll position — see
    // `layout::current_zoom_factor`, which this and `step_zoom`'s ladder both
    // read from so the two can never disagree about "current".
    let zoom_label = Label::new(Some("100%"));
    zoom_label.add_css_class("zoom-indicator");
    let zoom_in = icon_button("Zoom in", Icon::ZoomIn);
    zoom.append(&zoom_out);
    zoom.append(&zoom_label);
    zoom.append(&zoom_in);

    let fit = group(&root, "Fit");
    let fit_width = icon_button("Fit width", Icon::FitWidth);
    let fit_page = icon_button("Fit page", Icon::FitPage);
    fit.append(&fit_width);
    fit.append(&fit_page);

    // --- the side columns --------------------------------------------------
    // Toggles, not commands: a side column is either on screen or it is not,
    // and that is exactly what a `ToggleButton` says. They start pressed
    // because both columns start open. `build_ui` connects them to the
    // panels and to the divider drags that can also collapse a column.
    let panels = group(&root, "Panels");
    let show_pages = ToggleButton::with_label("Pages");
    decorate_button(&show_pages, "Pages", Icon::PanelLeft);
    show_pages.set_active(true);
    let show_tools = ToggleButton::with_label("Tools");
    decorate_button(&show_tools, "Tools", Icon::PanelRight);
    show_tools.set_active(true);
    panels.append(&show_pages);
    panels.append(&show_tools);

    // --- find --------------------------------------------------------------
    let find = group(&root, "Find");
    find.add_css_class("toolbar-group-last");
    let search_button = MenuButton::new();
    search_button.set_child(Some(&button_content(
        Icon::Search,
        "Find in document",
        false,
    )));
    search_button.set_tooltip_text(Some("Find in document (Ctrl+F)"));
    search_button.update_property(&[gtk::accessible::Property::Label("Find in document")]);
    let search_popover = Popover::new();
    search_popover.add_css_class("toolbar-search");
    let search_content = GtkBox::new(Orientation::Vertical, 8);
    let search_title = Label::new(Some("Find in document"));
    search_title.set_xalign(0.0);
    search_title.add_css_class("panel-heading");
    search_content.append(&search_title);
    let search_row = GtkBox::new(Orientation::Horizontal, 4);
    // Exact, case-sensitive search: the same matcher `pdf-ffi` uses, so
    // this shell and the other platforms agree on what a match is.
    let search_entry = Entry::builder()
        .placeholder_text("Find in document")
        .width_chars(FIND_ENTRY_CHARS)
        .max_width_chars(FIND_ENTRY_CHARS)
        .build();
    search_entry.update_property(&[gtk::accessible::Property::Label("Search document")]);
    let find_previous = icon_button("Previous match", Icon::Previous);
    let find_next = icon_button("Next match", Icon::Next);
    find_previous.set_sensitive(false);
    find_next.set_sensitive(false);
    search_row.append(&search_entry);
    search_row.append(&find_previous);
    search_row.append(&find_next);
    search_content.append(&search_row);
    let search_hint = Label::new(Some("Case-sensitive. Press Enter to search."));
    search_hint.set_xalign(0.0);
    search_hint.add_css_class("dim-label");
    search_content.append(&search_hint);
    search_popover.set_child(Some(&search_content));
    search_popover.connect_show({
        let entry = search_entry.clone();
        move |_| {
            entry.grab_focus();
        }
    });
    search_button.set_popover(Some(&search_popover));
    find.append(&search_button);

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

fn icon_button(label: &str, icon: Icon) -> Button {
    let button = Button::new();
    decorate_button(&button, label, icon);
    button
}

fn decorate_button(button: &impl IsA<Button>, label: &str, icon: Icon) {
    let button = button.as_ref();
    button.set_child(Some(&button_content(icon, label, false)));
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
}

fn button_content(icon: Icon, label: &str, show_label: bool) -> GtkBox {
    let content = GtkBox::new(Orientation::Horizontal, 6);
    content.set_halign(gtk::Align::Center);
    let image = build_icon(icon, 18, NEUTRAL_TINT);
    let caption = Label::new(Some(label));
    // Missing SVG loaders must leave usable text controls, not blank buttons.
    caption.set_visible(show_label || image.paintable().is_none());
    image.set_visible(image.paintable().is_some());
    image.connect_notify_local(Some("paintable"), {
        let caption = caption.downgrade();
        move |image, _| {
            image.set_visible(image.paintable().is_some());
            if let Some(caption) = caption.upgrade() {
                caption.set_visible(show_label || image.paintable().is_none());
            }
        }
    });
    content.append(&image);
    content.append(&caption);
    content
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
        .spacing(2)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Start)
        .accessible_role(AccessibleRole::Group)
        .build();
    group.update_property(&[gtk::accessible::Property::Label(label)]);
    group.add_css_class("toolbar-group");
    bar.append(&group);
    group
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gtk::test]
    fn gtk_ui_toolbar_fits_one_row_at_desktop_width_and_wraps_when_narrow() {
        super::super::shell::install_shell_css();
        let toolbar = build_editor_toolbar();
        toolbar.page_indicator.set_text("1 / 12");
        let (_, desktop_height, _, _) = toolbar.root.measure(Orientation::Vertical, 860);
        let (_, narrow_height, _, _) = toolbar.root.measure(Orientation::Vertical, 560);
        assert!(
            desktop_height <= 64,
            "desktop toolbar is {desktop_height}px tall"
        );
        assert!(narrow_height > desktop_height, "narrow toolbar must reflow");
        assert!(
            narrow_height <= 112,
            "narrow toolbar is {narrow_height}px tall"
        );
    }

    #[gtk::test]
    fn gtk_ui_toolbar_icons_keep_accessible_names_and_tooltips() {
        use gtk::glib::translate::{from_glib_full, ToGlibPtr};

        let toolbar = build_editor_toolbar();
        for (button, label) in [
            (&toolbar.open, "Open PDF"),
            (&toolbar.save, "Save as"),
            (&toolbar.print, "Print"),
            (&toolbar.zoom_out, "Zoom out"),
            (&toolbar.zoom_in, "Zoom in"),
            (&toolbar.fit_width, "Fit width"),
            (&toolbar.fit_page, "Fit page"),
            (toolbar.show_pages.upcast_ref(), "Pages"),
            (toolbar.show_tools.upcast_ref(), "Tools"),
            (&toolbar.find_previous, "Previous match"),
            (&toolbar.find_next, "Next match"),
        ] {
            let accessible: &gtk::Accessible = button.as_ref();
            let expected = <str as ToGlibPtr<'_, *const std::ffi::c_char>>::to_glib_none(label);
            let mismatch: Option<gtk::glib::GString> = unsafe {
                from_glib_full(gtk::ffi::gtk_test_accessible_check_property(
                    accessible.to_glib_none().0,
                    gtk::ffi::GTK_ACCESSIBLE_PROPERTY_LABEL,
                    expected.0,
                ))
            };
            assert!(mismatch.is_none(), "{label}: {mismatch:?}");
            assert!(button
                .tooltip_text()
                .is_some_and(|text| text.starts_with(label)));
        }
    }

    #[gtk::test]
    fn gtk_ui_toolbar_falls_back_to_text_if_an_icon_cannot_render() {
        let content = button_content(Icon::Print, "Print", false);
        let image = content
            .first_child()
            .unwrap()
            .downcast::<gtk::Image>()
            .unwrap();
        let caption = content.last_child().unwrap().downcast::<Label>().unwrap();
        let paintable = image.paintable().expect("bundled Print icon must render");
        assert!(!caption.is_visible());
        image.set_paintable(None::<&gtk::gdk::Paintable>);
        assert!(caption.is_visible());
        assert_eq!(caption.text(), "Print");
        assert!(!image.is_visible());
        image.set_paintable(Some(&paintable));
        assert!(!caption.is_visible());
        assert!(image.is_visible());
    }

    /// The property the whole module exists for: the bar's *minimum* width is
    /// one group, not the sum of every control, so a window narrower than the
    /// full bar wraps it instead of clipping controls off the right edge.
    ///
    /// Measured against the sum of the groups rather than against the
    /// `FlowBox`'s own natural width, and as a ratio rather than a pixel
    /// count. The sum is exactly what the old flat `GtkBox` demanded, so it
    /// is the number this has to beat — whereas `FlowBox`'s reported natural
    /// width turns out to be environment-dependent (it came back equal to the
    /// minimum under CI's Xvfb while reporting 1366px against the same 301px
    /// minimum locally), which made the original form of this assertion fail
    /// for a reason that had nothing to do with the property under test.
    #[gtk::test]
    fn gtk_ui_toolbar_minimum_width_is_one_group_not_the_whole_bar() {
        let toolbar = build_editor_toolbar();

        let (minimum, _, _, _) = toolbar.root.measure(Orientation::Horizontal, -1);
        let sum_of_groups = sum_of_child_widths(&toolbar.root);

        assert!(
            minimum * 2 <= sum_of_groups,
            "toolbar minimum {minimum} is not meaningfully below the {sum_of_groups} its groups \
             add up to; it is behaving like the plain GtkBox it replaced and will clip instead \
             of wrapping"
        );
    }

    /// What a horizontal `GtkBox` of the same children would have reported as
    /// its own minimum: the sum of their natural widths.
    fn sum_of_child_widths(container: &impl IsA<gtk::Widget>) -> i32 {
        std::iter::successors(container.as_ref().first_child(), |child| {
            child.next_sibling()
        })
        .map(|child| child.measure(Orientation::Horizontal, -1).1)
        .sum()
    }

    /// A wrap must move whole groups. `FlowBox`'s default cap of seven
    /// children per line happens to equal the group count today, so a group
    /// added without raising [`GROUP_COUNT`] would start the bar off already
    /// wrapped on a window with room to spare.
    #[gtk::test]
    fn gtk_ui_every_toolbar_group_can_share_one_row_when_wide_enough() {
        let toolbar = build_editor_toolbar();

        let groups = std::iter::successors(toolbar.root.first_child(), |child| child.next_sibling())
            .count() as u32;

        assert_eq!(groups, GROUP_COUNT);
        assert!(toolbar.root.max_children_per_line() >= groups);
    }
}
