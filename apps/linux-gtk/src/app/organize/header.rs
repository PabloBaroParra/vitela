//! The Organize screen's header row: the heading, and every control that acts
//! on the document as a whole.
//!
//! ## Why the header is a module and the cards are not
//!
//! A card's buttons belong to the page under them and are built where that
//! page's widget is built ([`super::grid`], [`super::documents`]). The header's
//! do not belong to anything on screen — Undo, Add PDFs, Extract, Split and
//! Save each act on the whole document — so the only thing holding them
//! together is the row itself, and that is a responsibility rather than a
//! leftover.
//!
//! Both halves of it live here: [`build`] makes the widgets and [`connect`]
//! wires them. They are separate functions for the reason
//! `metadata::build_metadata_panel`/`connect_metadata_panel` are — the widgets
//! have to exist before the `Viewer` that owns them does — but they are not
//! separate *files*, because a button whose label promises one thing and whose
//! handler does another is exactly the bug a reader should be able to see in
//! one screen.
//!
//! ## Why the actions are a `FlowBox` and not a row
//!
//! They were a plain horizontal `GtkBox`, and a `GtkBox` has no way to give up
//! width: its minimum is the *sum* of its children's. Six buttons and a
//! heading demanded 619px, and the screen around them 651px. Below that GTK
//! has nothing to do but under-allocate, and whatever falls past the edge is
//! simply not drawn — with Save, the primary action, first out of the window
//! because it is last in the row. A user on a narrow display, or on a
//! compositor that forces a size onto the window (WSLg does), lost the ability
//! to save and got no hint that anything was missing.
//!
//! A `FlowBox` wraps instead. Its minimum is its *widest child* rather than
//! their sum, so the row folds onto a second line and the header grows taller
//! rather than hiding controls. At any width that already fit, it lays out as
//! the single row it always was — which is what made this the cheap fix rather
//! than an overflow menu.
//!
//! The heading ellipsizes for the same reason, and yields first: it is the one
//! thing in the row still readable at half its width.

use gtk::prelude::*;
use gtk::{
    Align, ApplicationWindow, Box as GtkBox, Button, FlowBox, Orientation, ProgressBar,
    SelectionMode,
};

use super::import;
use crate::app::state::Viewer;
use crate::app::tools_panel::panel_heading;
use crate::app::write::{begin_extract, begin_split, show_save_chooser};

/// The header's controls, handed back to [`super::build_organize_panel`] so
/// they can be stored on `OrganizePanel` — the row itself keeps no state, and
/// every one of these is read or toggled from elsewhere later.
pub(super) struct Controls {
    pub(super) add_pdfs: Button,
    pub(super) import_progress: ProgressBar,
    pub(super) cancel_import: Button,
    pub(super) extract: Button,
    pub(super) split: Button,
    pub(super) save: Button,
}

/// Builds the header row and the controls in it.
///
/// The order the actions are appended in is the order they appear, and
/// `organize::tests` asserts the neighbours of each: it is the only thing
/// distinguishing "Extract" from "Split" from "Save" for someone reading the
/// screen left to right, so it is pinned rather than left to whoever edits
/// this next. No sort function is set on the `FlowBox` — unlike the page
/// grid — so wrapping changes which *line* a button is on, never which button
/// follows which.
pub(super) fn build() -> (GtkBox, Controls) {
    let header = GtkBox::new(Orientation::Horizontal, 12);
    let heading = panel_heading("Organize pages");
    heading.set_hexpand(true);
    // The first thing to give up width when there is not enough — see the
    // module doc. Without it the heading holds 100px it does not need while
    // the actions wrap beside it.
    heading.set_ellipsize(gtk::pango::EllipsizeMode::End);
    header.append(&heading);

    let import_progress = ProgressBar::new();
    import_progress.set_hexpand(true);
    import_progress.set_visible(false);
    import_progress.update_property(&[gtk::accessible::Property::Label("PDF import progress")]);
    header.append(&import_progress);
    let cancel_import = Button::with_label("Cancel");
    cancel_import.set_visible(false);
    header.append(&cancel_import);

    let actions = build_actions();
    header.append(&actions);
    for (label, action) in [("Undo", "win.undo"), ("Redo", "win.redo")] {
        let button = Button::with_label(label);
        button.set_action_name(Some(action));
        actions.append(&button);
    }
    let add_pdfs = Button::with_label("Add PDFs");
    actions.append(&add_pdfs);
    let extract = Button::with_label("Extract");
    extract.set_tooltip_text(Some("Save chosen pages as a new PDF"));
    actions.append(&extract);
    let split = Button::with_label("Split");
    split.set_tooltip_text(Some("Cut this PDF into several new PDFs"));
    actions.append(&split);
    let save = Button::with_label("Save");
    save.add_css_class("home-primary");
    actions.append(&save);

    (
        header,
        Controls {
            add_pdfs,
            import_progress,
            cancel_import,
            extract,
            split,
            save,
        },
    )
}

/// The container the actions wrap inside.
///
/// `SelectionMode::None` because these are commands, not a list to pick from —
/// the same reason the page grid sets it, and what keeps a click reaching the
/// button rather than selecting the slot around it. Not homogeneous, because a
/// `FlowBox` otherwise allocates every child the width of the widest and
/// "Undo" would be as wide as "Add PDFs".
fn build_actions() -> FlowBox {
    let actions = FlowBox::new();
    actions.set_selection_mode(SelectionMode::None);
    actions.set_homogeneous(false);
    actions.set_min_children_per_line(1);
    // The six buttons appended above: one line whenever they fit, which is
    // every width the window can be dragged to on an ordinary display.
    actions.set_max_children_per_line(6);
    actions.set_column_spacing(12);
    actions.set_row_spacing(8);
    // End rather than Fill: the actions stay against the right edge as they
    // always have, and the slack the heading is not using does not stretch
    // the row across the screen.
    actions.set_halign(Align::End);
    actions.set_valign(Align::Center);
    actions
}

/// Wires every header button. Called from
/// [`super::connect_organize_panel`], once the `Viewer` that owns the widgets
/// exists.
///
/// `window` is what the four choosers are transient for — the import chooser,
/// Extract's save dialog, Split's folder dialog and Save's own — which is why
/// this needs it where `views::connect` does not.
pub(super) fn connect(window: &ApplicationWindow, viewer: &Viewer) {
    viewer.organize.add_pdfs_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| import::show_chooser(&window, &viewer)
    });
    viewer.organize.cancel_import_button.connect_clicked({
        let viewer = viewer.clone();
        move |_| import::cancel(&viewer)
    });
    viewer.organize.extract_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| begin_extract(&window, &viewer)
    });
    viewer.organize.split_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| begin_split(&window, &viewer)
    });
    viewer.organize.save_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| show_save_chooser(&window, &viewer)
    });
}
