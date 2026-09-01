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
mod editor_toolbar;
mod forms;
mod input;
mod layout;
mod metadata;
mod print;
mod render;
mod search;
mod selection;
mod shell;
mod side_panel;
mod sign;
mod state;
mod tools_panel;

#[cfg(test)]
mod ui_tests;

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, glib, Application, ApplicationWindow, Box as GtkBox, Button, FlowBox, Label, Orientation,
    Overlay, Paned, PolicyType, ScrolledWindow,
};

use annotations::add_annotation_toolbar;
use brand::build_app_mark;
use document::{
    confirm_closing_edits, new_blank_document, open_file, open_sample, show_file_chooser,
    show_save_chooser, SampleKind,
};
use editor_toolbar::build_editor_toolbar;
use layout::{current_zoom_factor, refresh_layout, set_zoom, Zoom};
use print::print_document;
use render::update_viewport;
use search::{run_search, step_match};
use shell::{build_app_rail, install_shell_css};
use side_panel::{collapsible, Column};
use sign::{build_sign_content, connect_sign_toolbar};
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
    install_shell_css();
    let window = ApplicationWindow::builder()
        .application(application)
        .default_width(1000)
        .default_height(800)
        .title("Vitela")
        .build();

    // Built as one unit by its own module — grouped, and wrapping onto more
    // rows instead of clipping when the window is narrow. See
    // `editor_toolbar` for why that is not optional. Destructured here so the
    // wiring below reads against the controls themselves rather than through
    // a struct that exists only to carry them across the module boundary.
    let editor_toolbar::EditorToolbar {
        root: toolbar,
        open: open_button,
        sample_actions,
        print: print_button,
        save: save_button,
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
    } = build_editor_toolbar();

    let status = Label::new(Some(
        "Choose a PDF file to view, or open the built-in sample.",
    ));
    status.set_xalign(0.0);
    // A status line is the least important thing on screen and the widest
    // string in the shell, and an un-ellipsized `Label` reports its full text
    // width as a *minimum* — which becomes the window's. Ellipsizing lets a
    // long message shorten itself rather than force the window wider than the
    // screen; `status.text()` still returns the message in full.
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    // Ellipsizing costs the user the tail of every long message, and this
    // label is the only place an open or save failure is ever reported —
    // "Could not open PDF: …" truncated at the width of the window says
    // nothing about what went wrong. Mirroring the text into the tooltip
    // keeps it readable without letting it drive the window's width again.
    // Wired to the property rather than to the ~20 `set_text` call sites, so
    // a message added later cannot forget to bring its tooltip along.
    status.connect_label_notify(|label| label.set_tooltip_text(Some(&label.text())));

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
    // Siblings of the mode toggle, not variants of it (T-163): they decide
    // what a content-edit-mode click *creates* rather than whether one can
    // happen at all — see `content_edit::set_insert_mode`.
    let (insert_text_button, insert_image_button) = content_edit::build_insert_toggles();
    // Beside the mode toggle rather than in the main toolbar: it only ever
    // acts on content-edit mode's own selection (T-162 Slice 1), the same
    // reason the annotation toolbar's own Delete lives in its own row.
    let delete_image_button = Button::with_label("Delete image");
    delete_image_button.set_sensitive(false);
    // Same row, same gate (T-162 Slice 2): a file-picker swap needs exactly
    // the selection Delete does.
    let replace_image_button = Button::with_label("Replace image");
    replace_image_button.set_sensitive(false);
    // A `FlowBox`, not a plain `GtkBox`, for the same reason
    // `annotations::add_annotation_toolbar` uses one: it wraps onto more rows
    // as the resizable tools panel narrows instead of reporting the sum of
    // five buttons' widths as this panel's minimum.
    let content_edit_row = FlowBox::new();
    content_edit_row.set_selection_mode(gtk::SelectionMode::None);
    content_edit_row.set_row_spacing(4);
    content_edit_row.set_column_spacing(4);
    content_edit_row.set_homogeneous(false);
    content_edit_row.append(&content_edit_button);
    content_edit_row.append(&insert_text_button);
    content_edit_row.append(&insert_image_button);
    content_edit_row.append(&delete_image_button);
    content_edit_row.append(&replace_image_button);

    // The forms-edit toolbar and style inspector (T-141), destined for the
    // tools panel's "Fill & Sign" page rather than this row — see
    // `tools_panel::build_tools_panel`'s `forms_content` parameter.
    let (form_field_toolbar, forms_content) = forms::build_forms_content();
    // The signing section of the same "Fill & Sign" page (Batch B23 Fase 2),
    // destined for `tools_panel::build_tools_panel`'s `sign_content`
    // parameter alongside `forms_content` — see that function for how the
    // two are stacked on one page.
    let (
        choose_signing_certificate,
        choose_pkcs11_certificate,
        choose_nss_certificate,
        sign_content,
    ) = build_sign_content();

    let pages = GtkBox::new(Orientation::Vertical, PAGE_GAP);
    pages.set_halign(gtk::Align::Center);
    let scroll = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&pages)
        .build();
    scroll.add_css_class("canvas-frame");

    // The mark rides above the scroller instead of replacing it, so the view
    // keeps its allocation while the empty state is up: the fit width the
    // layout module measures from `scroll` is right on the first paint of a
    // document rather than one resize behind it.
    let app_mark = build_app_mark();
    let page_area = Overlay::new();
    // Explicit rather than left to propagate from `scroll`: this is the one
    // pane that must claim the space the two `Paned`s below leave over, and
    // that has to be a stated fact about `page_area` itself, not an inference
    // GTK draws from its child — see `tools_panel`'s `set_hexpand(false)` for
    // why an inferred answer is not to be trusted here.
    page_area.set_hexpand(true);
    page_area.set_child(Some(&scroll));
    page_area.add_overlay(&app_mark);

    let page_navigation = GtkBox::new(Orientation::Vertical, 4);
    page_navigation.add_css_class("page-navigation");
    page_navigation.update_property(&[gtk::accessible::Property::Label("Pages")]);
    let page_navigation_scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(PolicyType::Never)
        .child(&page_navigation)
        .build();
    let navigation_panel = GtkBox::new(Orientation::Vertical, 10);
    navigation_panel.add_css_class("navigation-panel");
    navigation_panel.set_hexpand(false);
    navigation_panel.set_width_request(144);
    let navigation_heading = Label::new(Some("Pages"));
    navigation_heading.set_xalign(0.0);
    navigation_heading.add_css_class("panel-heading");
    navigation_panel.append(&navigation_heading);
    navigation_panel.append(&page_navigation_scroll);

    let (metadata_panel, metadata_content) = metadata::build_metadata_panel();
    let (tools_content, tools_stack) = tools_panel::build_tools_panel(
        &annotation_row,
        &content_edit_row,
        &forms_content,
        &sign_content,
        &metadata_content,
    );
    let tools_panel = GtkBox::new(Orientation::Vertical, 10);
    tools_panel.add_css_class("tools-panel");
    tools_panel.update_property(&[gtk::accessible::Property::Label("Tools and properties")]);
    tools_panel.set_hexpand(false);
    // No `width_request`. The 220 that used to be here was never the real
    // floor anyway — the tab strip's own 400px was — and now that the strip
    // wraps (`tools_panel::build_tab_switcher`), the honest minimum is
    // whatever the controls inside actually need. A hard request on top of
    // that would only make the divider collapse the column earlier than it
    // has to. The *initial* width is the `Paned` position below, not a
    // minimum.
    tools_panel.append(&tools_content);
    let tools_scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(PolicyType::Never)
        .child(&tools_panel)
        .build();

    let (app_rail, app_rail_box) = build_app_rail();

    // Each column goes into the `Paned` through a slot that can fold it away
    // while staying visible itself — which is what keeps the divider's own
    // separator on screen to drag back. See `side_panel::collapsible`.
    let navigation_slot = collapsible(&navigation_panel);
    let tools_slot = collapsible(&tools_scroll);

    // Nav | (canvas | tools), both boundaries user-draggable — a plain
    // `GtkBox` has no drag handle of its own, so the three-column layout
    // needs two nested `Paned`s rather than one flat row. The rail sits
    // outside both: it is icon-rail width always, never something a document
    // window is short on room for.
    let canvas_tools_paned = Paned::new(Orientation::Horizontal);
    canvas_tools_paned.set_wide_handle(true);
    canvas_tools_paned.set_hexpand(true);
    canvas_tools_paned.set_start_child(Some(&page_area));
    canvas_tools_paned.set_end_child(Some(&tools_slot));
    // The canvas absorbs a window resize; the tools panel keeps the width the
    // user last dragged it to, the same way a code editor's side panel does.
    // `resize_*_child` (window resize) and `shrink_*_child` (how far a drag
    // may push a boundary) are different axes; only the first is pinned here —
    // `side_panel::connect` explains why `shrink_*_child` must stay `true`.
    canvas_tools_paned.set_resize_start_child(true);
    canvas_tools_paned.set_resize_end_child(false);
    canvas_tools_paned.set_position(500);

    let nav_paned = Paned::new(Orientation::Horizontal);
    nav_paned.set_wide_handle(true);
    nav_paned.set_hexpand(true);
    nav_paned.set_start_child(Some(&navigation_slot));
    nav_paned.set_end_child(Some(&canvas_tools_paned));
    nav_paned.set_resize_start_child(false);
    nav_paned.set_resize_end_child(true);
    nav_paned.set_position(144);

    // Both columns, both directions: the toggle folds a column away and
    // brings it back, and so does dragging the divider across the width the
    // column needs to draw in.
    side_panel::connect(&nav_paned, Column::Start, &navigation_slot, &show_pages);
    side_panel::connect(&canvas_tools_paned, Column::End, &tools_slot, &show_tools);

    let main = GtkBox::new(Orientation::Horizontal, 0);
    main.add_css_class("editor-main");
    main.set_vexpand(true);
    main.append(&app_rail_box);
    main.append(&nav_paned);

    status.add_css_class("status-bar");
    let content = GtkBox::new(Orientation::Vertical, 0);
    content.add_css_class("vitela-shell");
    content.append(&toolbar);
    content.append(&main);
    content.append(&status);
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
        page_navigation,
        metadata: metadata_panel,
        app_mark,
        status,
        page_indicator,
        zoom_label,
        search_entry,
        find_previous,
        find_next,
        print_button,
        save_button,
        undo_action,
        redo_action,
        annotation_buttons: annotation_toolbar,
        content_edit_button,
        insert_text_button,
        insert_image_button,
        delete_image_button,
        replace_image_button,
        forms: form_field_toolbar,
        choose_signing_certificate,
        choose_pkcs11_certificate,
        choose_nss_certificate,
        state: Rc::new(RefCell::new(ViewerState {
            generation: 0,
            session_id: 0,
            session: None,
            active_tool: None,
            content_edit_mode: false,
            content_insert_mode: None,
            content_refresh_in_flight: false,
            content_refresh_pending: None,
            form_edit_mode: false,
            form_field_kind: None,
            password_dialog: None,
            pfx_dialog: None,
            pkcs11_dialog: None,
            nss_dialog: None,
            sign_picker_dialog: None,
        })),
    };
    connect_viewport_updates(&viewer);
    connect_search(&viewer);
    annotations::connect_annotation_toolbar(&viewer);
    content_edit::connect_toggle(&viewer);
    content_edit::connect_insert_toggles(&viewer);
    forms::connect_forms_toolbar(&viewer);
    connect_sign_toolbar(&window, &viewer);
    metadata::connect_metadata_panel(&viewer);
    viewer.delete_image_button.connect_clicked({
        let viewer = viewer.clone();
        move |_| content_edit::image::delete_selected(&viewer)
    });
    viewer.replace_image_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| content_edit::image::replace_selected(&window, &viewer)
    });
    // Same command as the toolbar's Open PDF button, just reachable from the
    // rail — `win.open` is installed once, below, by
    // `connect_standard_shortcuts`.
    app_rail.files.set_action_name(Some("win.open"));
    // Focuses the first annotation tool rather than arming it: a rail click
    // is a navigation gesture, not a promise to start drawing a highlight the
    // moment the page loads. Focusing (rather than `annotation_row.grab_focus`,
    // which has no button of its own to delegate to) is also what scrolls the
    // tools panel to reveal the section, via GTK's usual focus-follows-scroll.
    app_rail.annotate.connect_clicked({
        let viewer = viewer.clone();
        move |_| {
            if let Some((_, button)) = viewer.annotation_buttons.create.first() {
                button.grab_focus();
            }
        }
    });
    // Unlike `annotate`, this one *is* the real toggle already on the tools
    // panel (`content_edit_button`) — flips the same switch, just reachable
    // from the rail. Guarded on sensitivity because `ToggleButton::set_active`
    // takes effect even on an insensitive widget, and this document may not
    // permit content edits yet.
    app_rail.edit_pdf.connect_clicked({
        let viewer = viewer.clone();
        move |_| {
            if viewer.content_edit_button.is_sensitive() {
                viewer.content_edit_button.set_active(true);
            }
        }
    });
    // T-186: switches to the "Fill & Sign" tab and, like `annotate` above,
    // focuses the first live control there — `update_sign_controls` (called
    // from `document::show_document`) is what keeps
    // `choose_signing_certificate` insensitive when the open document
    // refuses signing, so an insensitive button here simply does not take
    // focus rather than needing a separate check.
    app_rail.sign.connect_clicked({
        let tools_stack = tools_stack.clone();
        let viewer = viewer.clone();
        move |_| {
            tools_stack.set_visible_child_name(tools_panel::FILL_SIGN_PAGE);
            viewer.choose_signing_certificate.grab_focus();
        }
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
    window.connect_close_request({
        let viewer = viewer.clone();
        move |window| confirm_closing_edits(window, &viewer)
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

pub(crate) fn navigate_to_page(viewer: &Viewer, page_index: usize) {
    let offset = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return;
        };
        if page_index >= session.page_heights.len() {
            return;
        }
        layout::page_top(&session.page_heights, page_index)
    };
    viewer.scroll.vadjustment().set_value(offset);
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
    let current = current_zoom_factor(viewer);
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
