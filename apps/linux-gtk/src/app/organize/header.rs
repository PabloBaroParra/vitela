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

use gtk::prelude::*;
use gtk::{ApplicationWindow, Box as GtkBox, Button, Orientation, ProgressBar};

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
/// The order the buttons are appended in is the order they appear, and
/// `organize::tests` asserts the neighbours of each: it is the only thing
/// distinguishing "Extract" from "Split" from "Save" for someone reading the
/// screen left to right, so it is pinned rather than left to whoever edits
/// this next.
pub(super) fn build() -> (GtkBox, Controls) {
    let header = GtkBox::new(Orientation::Horizontal, 12);
    let heading = panel_heading("Organize pages");
    heading.set_hexpand(true);
    header.append(&heading);
    for (label, action) in [("Undo", "win.undo"), ("Redo", "win.redo")] {
        let button = Button::with_label(label);
        button.set_action_name(Some(action));
        header.append(&button);
    }

    let add_pdfs = Button::with_label("Add PDFs");
    header.append(&add_pdfs);
    let import_progress = ProgressBar::new();
    import_progress.set_hexpand(true);
    import_progress.set_visible(false);
    import_progress.update_property(&[gtk::accessible::Property::Label("PDF import progress")]);
    header.append(&import_progress);
    let cancel_import = Button::with_label("Cancel");
    cancel_import.set_visible(false);
    header.append(&cancel_import);

    let extract = Button::with_label("Extract");
    extract.set_tooltip_text(Some("Save chosen pages as a new PDF"));
    header.append(&extract);
    let split = Button::with_label("Split");
    split.set_tooltip_text(Some("Cut this PDF into several new PDFs"));
    header.append(&split);
    let save = Button::with_label("Save");
    save.add_css_class("home-primary");
    header.append(&save);

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
