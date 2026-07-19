//! GTK4 shell application: window bootstrap, signal wiring, and the shared
//! constants the feature modules render against.
//!
//! The feature modules form a small dependency graph rather than a strict
//! layering: `document` drives `layout`/`render`/`search` on open, `render`
//! reads geometry from `layout`, and `print` reuses `render`'s rasterizer.
//! Cyclic references between sibling modules are fine — it is all one crate.

mod document;
mod layout;
mod print;
mod render;
mod search;
mod state;

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    glib, Application, ApplicationWindow, Box as GtkBox, Button, Entry, FileChooserNative, Label,
    Orientation, ScrolledWindow,
};

use document::show_file_chooser;
use layout::refresh_layout;
use print::print_document;
use render::update_viewport;
use search::{run_search, step_match};
use state::{Viewer, ViewerState};

const APPLICATION_ID: &str = "org.vitela.Pdf";
/// Vertical gap between stacked page widgets. Shared by the page box in
/// [`build_ui`] and the geometry walks in `layout` that must mirror it.
pub(crate) const PAGE_GAP: i32 = 12;

pub fn run() -> glib::ExitCode {
    let application = Application::builder()
        .application_id(APPLICATION_ID)
        .build();
    application.connect_activate(build_ui);
    application.run()
}

fn build_ui(application: &Application) {
    let window = ApplicationWindow::builder()
        .application(application)
        .default_width(1000)
        .default_height(800)
        .title("Vitela")
        .build();

    let open_button = Button::with_label("Open PDF");
    let status = Label::new(Some("Choose a PDF file to view."));
    status.set_xalign(0.0);

    // Exact, case-sensitive search: the same matcher `pdf-ffi` uses, so
    // this shell and the other platforms agree on what a match is.
    let search_entry = Entry::builder()
        .placeholder_text("Find in document")
        .hexpand(true)
        .build();
    let find_previous = Button::with_label("Previous");
    let find_next = Button::with_label("Next");
    find_previous.set_sensitive(false);
    find_next.set_sensitive(false);

    let print_button = Button::with_label("Print");
    print_button.set_sensitive(false);

    let toolbar = GtkBox::new(Orientation::Horizontal, 8);
    toolbar.append(&open_button);
    toolbar.append(&search_entry);
    toolbar.append(&find_previous);
    toolbar.append(&find_next);
    toolbar.append(&print_button);

    let pages = GtkBox::new(Orientation::Vertical, PAGE_GAP);
    pages.set_halign(gtk::Align::Center);
    let scroll = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&pages)
        .build();

    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&toolbar);
    content.append(&status);
    content.append(&scroll);
    window.set_child(Some(&content));

    let viewer = Viewer {
        scroll,
        pages,
        status,
        search_entry,
        find_previous,
        find_next,
        print_button,
        state: Rc::new(RefCell::new(ViewerState {
            generation: 0,
            session: None,
        })),
    };
    connect_viewport_updates(&viewer);
    connect_search(&viewer);
    viewer.print_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| print_document(&window, &viewer)
    });

    // `FileChooserNative` is not a widget: GTK holds no reference to it
    // while it is shown, so the shell must keep it alive here until the
    // response arrives or the dialog is destroyed before it can be used.
    let active_chooser: Rc<RefCell<Option<FileChooserNative>>> = Rc::new(RefCell::new(None));
    open_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        let active_chooser = active_chooser.clone();
        move |_| show_file_chooser(&window, &viewer, &active_chooser)
    });

    window.present();
}

fn connect_search(viewer: &Viewer) {
    viewer.search_entry.connect_activate({
        let viewer = viewer.clone();
        move |_| run_search(&viewer)
    });
    viewer.find_next.connect_clicked({
        let viewer = viewer.clone();
        move |_| step_match(&viewer, 1)
    });
    viewer.find_previous.connect_clicked({
        let viewer = viewer.clone();
        move |_| step_match(&viewer, -1)
    });
}

fn connect_viewport_updates(viewer: &Viewer) {
    let adjustment = viewer.scroll.vadjustment();
    adjustment.connect_value_changed({
        let viewer = viewer.clone();
        move |_| update_viewport(&viewer)
    });
    adjustment.connect_page_size_notify({
        let viewer = viewer.clone();
        move |_| update_viewport(&viewer)
    });
    // GtkWidget has no "width" GObject property, so `notify::width` would
    // never fire. The horizontal adjustment's page size tracks the
    // viewport width and changes on every horizontal resize — the correct
    // signal to re-fit pages to the new available width.
    let hadjustment = viewer.scroll.hadjustment();
    hadjustment.connect_page_size_notify({
        let viewer = viewer.clone();
        move |_| refresh_layout(&viewer)
    });
}
