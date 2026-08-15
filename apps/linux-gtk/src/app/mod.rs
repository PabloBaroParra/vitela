//! GTK4 shell application: window bootstrap, signal wiring, and the shared
//! constants the feature modules render against.
//!
//! The feature modules form a small dependency graph rather than a strict
//! layering: `document` drives `layout`/`render`/`search` on open, `render`
//! reads geometry from `layout`, and `print` reuses `render`'s rasterizer.
//! Cyclic references between sibling modules are fine — it is all one crate.

mod annotations;
mod brand;
mod content_edit;
mod document;
mod input;
mod layout;
mod print;
mod render;
mod search;
mod selection;
mod state;

#[cfg(test)]
mod ui_tests;

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, glib, Application, ApplicationWindow, Box as GtkBox, Button, Entry, Label, MenuButton,
    Orientation, Overlay, ScrolledWindow,
};

use annotations::add_annotation_toolbar;
use brand::build_app_mark;
use document::{
    new_blank_document, open_file, open_sample, show_file_chooser, show_save_chooser, SampleKind,
};
use layout::{refresh_layout, set_zoom, Zoom};
use print::print_document;
use render::update_viewport;
use search::{run_search, step_match};
use state::{Viewer, ViewerState};

const APPLICATION_ID: &str = "org.vitela.Pdf";
/// Vertical gap between stacked page widgets. Shared by the page box in
/// [`build_ui`] and the geometry walks in `layout` that must mirror it.
pub(crate) const PAGE_GAP: i32 = 12;

/// Private construction result shared by production startup and crate-local GTK tests.
#[derive(Clone)]
struct BuiltUi {
    window: ApplicationWindow,
    viewer: Viewer,
}

pub fn run() -> glib::ExitCode {
    let application = Application::builder()
        .application_id(APPLICATION_ID)
        // The shipped .desktop entry launches with `Exec=vitela %F`; without
        // this flag GApplication rejects the file argument outright instead
        // of emitting `open` below.
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let built_ui: Rc<RefCell<Option<BuiltUi>>> = Rc::new(RefCell::new(None));

    application.connect_activate({
        let built_ui = built_ui.clone();
        move |application| {
            ensure_built(&built_ui, application).window.present();
        }
    });
    application.connect_open({
        let built_ui = built_ui.clone();
        move |application, files, _hint| {
            let ui = ensure_built(&built_ui, application);
            ui.window.present();
            // A file-manager "Open With" always means one document; extra
            // entries in `files` (multi-select) are silently ignored rather
            // than half-supporting multi-window.
            if let Some(path) = files.first().and_then(gio::File::path) {
                open_file(&ui.viewer, path);
            }
        }
    });
    application.run()
}

/// Builds the window on the first `activate`/`open` and reuses it for every
/// later one, so a second file-manager launch loads into the already-open
/// window (through [`open_file`]'s usual replace-the-document flow) instead
/// of spawning a second window.
fn ensure_built(built_ui: &Rc<RefCell<Option<BuiltUi>>>, application: &Application) -> BuiltUi {
    if let Some(built) = built_ui.borrow().as_ref() {
        return built.clone();
    }
    let built = build_ui(application);
    *built_ui.borrow_mut() = Some(built.clone());
    built
}

fn build_ui(application: &Application) -> BuiltUi {
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
    search_entry.update_property(&[gtk::accessible::Property::Label("Search document")]);
    let find_previous = Button::with_label("Previous");
    let find_next = Button::with_label("Next");
    find_previous.set_sensitive(false);
    find_next.set_sensitive(false);

    let print_button = Button::with_label("Print");
    print_button.set_sensitive(false);
    let save_button = Button::with_label("Save as");
    save_button.set_sensitive(false);
    // Bound to the actions rather than wired to a handler, so GTK greys them
    // out whenever `win.undo`/`win.redo` are disabled. Offering Redo when the
    // history has nothing to redo is a promise the toolbar cannot keep: the
    // click is accepted, nothing changes, and the status line has to explain
    // it after the fact. The accelerators were already gated on `can_undo`/
    // `can_redo`; this is the same gate reaching the buttons, from the same
    // source, instead of a second copy of the rule kept in step by hand.
    let undo_button = Button::with_label("Undo");
    undo_button.set_action_name(Some("win.undo"));
    let redo_button = Button::with_label("Redo");
    redo_button.set_action_name(Some("win.redo"));
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
    toolbar.append(&save_button);
    toolbar.append(&undo_button);
    toolbar.append(&redo_button);
    // Its own row rather than more widgets on this one: twelve annotation
    // controls do not belong in the same horizontal budget as open/zoom/
    // search/print, and stacking them there pushed the window's minimum width
    // past the screen (see `annotations::add_annotation_toolbar`).
    let (annotation_toolbar, annotation_row) = add_annotation_toolbar();
    // Next to the annotation row, not inside it: arming this and arming an
    // annotation tool are mutually exclusive (`content_edit::set_mode`,
    // `annotations::toolbar::arm_tool`), so it reads as a sibling mode rather
    // than an eighth annotation type.
    let content_edit_button = content_edit::build_toggle();
    // Beside the mode toggle rather than in the main toolbar: it only ever
    // acts on content-edit mode's own selection (T-162 Slice 1), the same
    // reason the annotation toolbar's own Delete lives in its own row.
    let delete_image_button = Button::with_label("Delete image");
    delete_image_button.set_sensitive(false);
    // Same row, same gate (T-162 Slice 2): a file-picker swap needs exactly
    // the selection Delete does.
    let replace_image_button = Button::with_label("Replace image");
    replace_image_button.set_sensitive(false);
    let content_edit_row = GtkBox::new(Orientation::Horizontal, 8);
    content_edit_row.append(&content_edit_button);
    content_edit_row.append(&delete_image_button);
    content_edit_row.append(&replace_image_button);

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
    content.append(&annotation_row);
    content.append(&content_edit_row);
    content.append(&status);
    content.append(&page_area);
    window.set_child(Some(&content));

    // A `SimpleAction` starts enabled, and `update_annotation_controls` — the
    // one place that maintains these — returns early when no document is open.
    // Without this the toolbar would offer Undo and Redo on an empty window,
    // before any history exists to act on. `print_button` and `save_button`
    // start closed for the same reason, a few lines above.
    let undo_action = gio::SimpleAction::new("undo", None);
    let redo_action = gio::SimpleAction::new("redo", None);
    undo_action.set_enabled(false);
    redo_action.set_enabled(false);

    let viewer = Viewer {
        scroll,
        pages,
        app_mark,
        status,
        search_entry,
        find_previous,
        find_next,
        print_button,
        save_button,
        undo_action,
        redo_action,
        annotation_buttons: annotation_toolbar,
        content_edit_button,
        delete_image_button,
        replace_image_button,
        state: Rc::new(RefCell::new(ViewerState {
            generation: 0,
            session_id: 0,
            session: None,
            active_tool: None,
            content_edit_mode: false,
            password_dialog: None,
        })),
    };
    connect_viewport_updates(&viewer);
    connect_search(&viewer);
    annotations::connect_annotation_toolbar(&viewer);
    content_edit::connect_toggle(&viewer);
    viewer.delete_image_button.connect_clicked({
        let viewer = viewer.clone();
        move |_| content_edit::image::delete_selected(&viewer)
    });
    viewer.replace_image_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| content_edit::image::replace_selected(&window, &viewer)
    });
    // Window-level, not page-level: the pointer is rarely over the page that
    // holds the selection by the time the user reaches for Ctrl+C.
    selection::connect_copy(application, &window, &viewer);
    input::connect_paste(application, &window, &viewer);
    input::connect_window_file_drop(&page_area, &viewer);
    annotations::connect_delete_shortcut(application, &window, &viewer);
    annotations::connect_history_shortcuts(application, &window, &viewer);
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
    viewer.save_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| show_save_chooser(&window, &viewer)
    });
    connect_standard_shortcuts(application, &window, &viewer);

    open_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| show_file_chooser(&window, &viewer)
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

    BuiltUi { window, viewer }
}

/// Adds the standard window commands that already have shell handlers. Copy,
/// undo, and redo are installed by their feature modules because their enabled
/// state is owned there.
fn connect_standard_shortcuts(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    let open = gio::SimpleAction::new("open", None);
    open.connect_activate({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_, _| show_file_chooser(&window, &viewer)
    });
    window.add_action(&open);
    application.set_accels_for_action("win.open", &["<Control>o"]);

    let save = gio::SimpleAction::new("save", None);
    save.connect_activate({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_, _| show_save_chooser(&window, &viewer)
    });
    window.add_action(&save);
    application.set_accels_for_action("win.save", &["<Control>s"]);

    let print = gio::SimpleAction::new("print", None);
    print.connect_activate({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_, _| print_document(&window, &viewer)
    });
    window.add_action(&print);
    application.set_accels_for_action("win.print", &["<Control>p"]);

    let find = gio::SimpleAction::new("find", None);
    find.connect_activate({
        let entry = viewer.search_entry.clone();
        move |_, _| {
            entry.grab_focus();
        }
    });
    window.add_action(&find);
    application.set_accels_for_action("win.find", &["<Control>f"]);

    let new = gio::SimpleAction::new("new", None);
    new.connect_activate({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_, _| new_blank_document(&window, &viewer)
    });
    window.add_action(&new);
    application.set_accels_for_action("win.new", &["<Control>n"]);
}

/// Whether the "Delete image" and "Replace image" controls are usable, and
/// applies it — the content-edit twin of
/// `annotations::toolbar::update_annotation_controls`, scoped to T-162's two
/// selection-gated buttons.
///
/// Called wherever `update_annotation_controls` already is (document
/// open/close, content-edit mode toggle) plus after every image
/// select/deselect/delete/replace inside `content_edit::image`.
pub(crate) fn update_content_edit_controls(viewer: &Viewer) {
    let state = viewer.state.borrow();
    let enabled = state.session.as_ref().is_some_and(|session| {
        session.content_edit_access.refusal().is_none() && session.selected_image.is_some()
    });
    viewer.delete_image_button.set_sensitive(enabled);
    viewer.replace_image_button.set_sensitive(enabled);
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
