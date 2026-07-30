//! GTK4 shell application: window bootstrap, signal wiring, and the shared
//! constants the feature modules render against.
//!
//! The feature modules form a small dependency graph rather than a strict
//! layering: `document` drives `layout`/`render`/`search` on open, `render`
//! reads geometry from `layout`, and `print` reuses `render`'s rasterizer.
//! Cyclic references between sibling modules are fine — it is all one crate.

mod brand;
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
    gio, glib, Application, ApplicationWindow, Box as GtkBox, Button, Entry, FileChooserNative,
    Label, MenuButton, Orientation, Overlay, ScrolledWindow,
};

use brand::build_app_mark;
use document::{open_sample, show_file_chooser, SampleKind};
use layout::{refresh_layout, set_zoom, Zoom};
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
    let status = Label::new(Some(
        "Choose a PDF file to view, or open the built-in sample.",
    ));
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
    let zoom_out = Button::with_label("Zoom out");
    let fit_width = Button::with_label("Fit width");
    let fit_page = Button::with_label("Fit page");
    let zoom_in = Button::with_label("Zoom in");

    let toolbar = GtkBox::new(Orientation::Horizontal, 8);
    toolbar.append(&open_button);
    toolbar.append(&sample_button);
    toolbar.append(&search_entry);
    toolbar.append(&find_previous);
    toolbar.append(&find_next);
    toolbar.append(&zoom_out);
    toolbar.append(&fit_width);
    toolbar.append(&fit_page);
    toolbar.append(&zoom_in);
    toolbar.append(&print_button);

    let pages = GtkBox::new(Orientation::Vertical, PAGE_GAP);
    pages.set_halign(gtk::Align::Center);
    let scroll = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&pages)
        .build();

    // The mark rides above the scroller instead of replacing it, so the view
    // keeps its allocation while the empty state is up: the fit width the
    // layout module measures from `scroll` is right on the first paint of a
    // document rather than one resize behind it.
    let app_mark = build_app_mark();
    let page_area = Overlay::new();
    page_area.set_child(Some(&scroll));
    page_area.add_overlay(&app_mark);

    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&toolbar);
    content.append(&status);
    content.append(&page_area);
    window.set_child(Some(&content));

    let viewer = Viewer {
        scroll,
        pages,
        app_mark,
        status,
        search_entry,
        find_previous,
        find_next,
        print_button,
        state: Rc::new(RefCell::new(ViewerState {
            generation: 0,
            session: None,
            password_dialog: None,
        })),
    };
    connect_viewport_updates(&viewer);
    connect_search(&viewer);
    zoom_out.connect_clicked({
        let viewer = viewer.clone();
        move |_| step_zoom(&viewer, false)
    });
    zoom_in.connect_clicked({
        let viewer = viewer.clone();
        move |_| step_zoom(&viewer, true)
    });
    fit_width.connect_clicked({
        let viewer = viewer.clone();
        move |_| set_zoom(&viewer, Zoom::FitWidth)
    });
    fit_page.connect_clicked({
        let viewer = viewer.clone();
        move |_| set_zoom(&viewer, Zoom::FitPage)
    });
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
    let action_sample_plain = gio::SimpleAction::new("plain", None);
    action_sample_plain.connect_activate({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_, _| open_sample(&window, &viewer, SampleKind::Plain)
    });
    sample_actions.add_action(&action_sample_plain);
    let action_sample_aes128 = gio::SimpleAction::new("aes128", None);
    action_sample_aes128.connect_activate({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_, _| open_sample(&window, &viewer, SampleKind::Aes128)
    });
    sample_actions.add_action(&action_sample_aes128);
    let action_sample_rc4128 = gio::SimpleAction::new("rc4128", None);
    action_sample_rc4128.connect_activate({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_, _| open_sample(&window, &viewer, SampleKind::Rc4128)
    });
    sample_actions.add_action(&action_sample_rc4128);

    window.present();
}

fn step_zoom(viewer: &Viewer, increase: bool) {
    const LADDER: [f64; 12] = [
        0.10, 0.25, 0.50, 0.75, 1.00, 1.25, 1.50, 2.00, 3.00, 4.00, 6.00, 8.00,
    ];
    let current = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| {
            // Under FitWidth every page carries a factor derived from its own
            // width, so the ladder has to step from the page on screen rather
            // than from page 0, which may be a different size entirely.
            let anchor = session.last_visible.map_or(0, |(first, _)| first);
            session
                .pages
                .get(anchor)
                .or_else(|| session.pages.first())
                .map(|page| page.budget.factor)
        })
        .unwrap_or(1.0);
    let factor = if increase {
        LADDER
            .into_iter()
            .find(|rung| *rung > current + 1e-9)
            .unwrap_or(8.0)
    } else {
        LADDER
            .into_iter()
            .rev()
            .find(|rung| *rung < current - 1e-9)
            .unwrap_or(0.10)
    };
    set_zoom(viewer, Zoom::Custom(factor));
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
    adjustment.connect_page_size_notify({
        let viewer = viewer.clone();
        move |_| refresh_layout(&viewer)
    });
    // GtkWidget has no "width" GObject property, so `notify::width` would
    // never fire. The horizontal adjustment's page size tracks the
    // viewport width and changes on every horizontal resize. The vertical
    // adjustment similarly signals height changes; `refresh_layout` reads the
    // ScrolledWindow allocation rather than either adjustment's page size.
    let hadjustment = viewer.scroll.hadjustment();
    hadjustment.connect_page_size_notify({
        let viewer = viewer.clone();
        move |_| refresh_layout(&viewer)
    });
}
