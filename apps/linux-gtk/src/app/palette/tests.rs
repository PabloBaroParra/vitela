//! The palette over a whole window built the way production builds it.
//!
//! No `gtk_ui_` test here opens a real document — CI's GTK job has no
//! pdfium — so a document is a model session, and the canvas is given a
//! page-shaped widget where a scroll position matters.

use gtk::{gdk, glib, Button, Overlay, Picture};
use pdf_document::{Document, Orientation, Page, PageId, PageSize, Rotation};

use super::entries::catalog;
use super::*;
use crate::app::home::{EDITOR_PAGE, HOME_PAGE};
use crate::app::i18n::{speak, Language};
use crate::app::organize::ORGANIZE_PAGE;
use crate::app::state::{AnnotationAccess, Tool};
use crate::app::test_fixtures::model_session;
use crate::app::tools_panel::ANNOTATE_PAGE;
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;

/// Closes the window when the test ends, panicking or not, with the session
/// cleared first so closing raises no unsaved-changes prompt.
struct Window(BuiltUi);

impl Drop for Window {
    fn drop(&mut self) {
        self.0.viewer.state.borrow_mut().session = None;
        self.0.window.close();
    }
}

/// A presented window at the size the canvas-jump bugs were seen in:
/// narrower, and the tools panel folds away.
fn window() -> Window {
    let built = built_ui();
    built.window.set_default_size(1_000, 720);
    built.window.present();
    let window = Window(built);
    pump();
    window
}

fn pump() {
    let context = glib::MainContext::default();
    for _ in 0..50 {
        while context.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Pumps the main loop until `widget` has been allocated.
fn settle(widget: &impl IsA<gtk::Widget>) {
    let context = glib::MainContext::default();
    for _ in 0..2_000 {
        while context.iteration(false) {}
        if widget.as_ref().width() > 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// A one-page model document: availability only reads the session.
fn open_document(viewer: &Viewer) {
    let session = model_session(Document::with_pages(vec![Page::base(
        PageId(0),
        0,
        PageSize::A4,
        Orientation::Portrait,
        Rotation::None,
    )]));
    viewer.state.borrow_mut().session = Some(session);
}

/// Ctrl+K, through the window action it is bound to.
fn press_ctrl_k(ui: &BuiltUi) {
    WidgetExt::activate_action(&ui.window, "win.command-palette", None)
        .expect("win.command-palette is registered");
    pump();
}

fn focused(ui: &BuiltUi) -> Option<gtk::Widget> {
    RootExt::focus(&ui.window)
}

/// Asked of the root: a `SearchEntry` hands the cursor to its inner text.
fn cursor_in_search(ui: &BuiltUi) -> bool {
    let search = &ui.palette.inner.view.search;
    focused(ui).is_some_and(|widget| widget == *search || widget.is_ancestor(search))
}

fn type_query(ui: &BuiltUi, query: &str) {
    ui.palette.inner.view.search.set_text(query);
    pump();
}

fn press_enter(ui: &BuiltUi) {
    ui.palette
        .inner
        .view
        .search
        .emit_by_name::<()>("activate", &[]);
    pump();
}

fn shown_ids(ui: &BuiltUi) -> Vec<&'static str> {
    ui.palette
        .inner
        .shown
        .borrow()
        .iter()
        .map(|(entry, _)| entry.id)
        .collect()
}

fn selected_id(ui: &BuiltUi) -> Option<&'static str> {
    let index = ui.palette.selected()?;
    shown_ids(ui).get(index).copied()
}

/// Opens the palette and runs `id` by clicking its row.
fn run_by_click(ui: &BuiltUi, id: &str) {
    press_ctrl_k(ui);
    let index = shown_ids(ui)
        .iter()
        .position(|shown| *shown == id)
        .unwrap_or_else(|| panic!("{id} is listed"));
    let row = ui
        .palette
        .inner
        .view
        .list
        .row_at_index(i32::try_from(index).unwrap())
        .expect("every shown entry has a row");
    ui.palette
        .inner
        .view
        .list
        .emit_by_name::<()>("row-activated", &[&row]);
    pump();
}

/// Every label drawn in `row`, in order.
fn row_texts(ui: &BuiltUi, index: usize) -> Vec<String> {
    let row = ui
        .palette
        .inner
        .view
        .list
        .row_at_index(i32::try_from(index).unwrap())
        .expect("the row exists");
    let mut texts = Vec::new();
    let mut stack = vec![row.upcast::<gtk::Widget>()];
    while let Some(widget) = stack.pop() {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            texts.push(label.text().to_string());
        }
        let mut child = widget.last_child();
        while let Some(current) = child {
            child = current.prev_sibling();
            stack.push(current);
        }
    }
    texts
}

fn screen(viewer: &Viewer) -> (Option<String>, Option<String>) {
    (
        viewer.view_stack.visible_child_name().map(Into::into),
        viewer.tools_stack.visible_child_name().map(Into::into),
    )
}

#[gtk::test]
fn gtk_ui_palette_ctrl_k_opens_with_the_cursor_in_the_search_field_on_every_screen() {
    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);
    let accels: Vec<String> = ui
        .window
        .application()
        .expect("the window has its application")
        .accels_for_action("win.command-palette")
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(accels, ["<Control>k"]);

    for page in [HOME_PAGE, EDITOR_PAGE, ORGANIZE_PAGE] {
        ui.viewer.view_stack.set_visible_child_name(page);
        pump();

        press_ctrl_k(ui);
        assert!(ui.palette.is_open(), "{page}");
        assert!(cursor_in_search(ui), "{page}: the cursor is in the field");
        assert_eq!(ui.palette.inner.view.search.text(), "", "{page}");
        assert_eq!(shown_ids(ui).len(), catalog().len(), "{page}");

        // Ctrl+K again closes it.
        press_ctrl_k(ui);
        assert!(!ui.palette.is_open(), "{page}");
    }
}

/// Opens over a focused widget on the zoomed canvas, closes with Esc: the
/// cursor goes back to that widget and the canvas does not move.
#[gtk::test]
fn gtk_ui_palette_escape_hands_the_cursor_back_and_leaves_the_canvas_where_it_was() {
    const PARKED: (f64, f64) = (60.0, 40.0);
    let built = window();
    let ui = &built.0;
    ui.viewer.view_stack.set_visible_child_name(EDITOR_PAGE);
    let (overlay, target) = page_with_a_button(ui);
    park(ui, PARKED);
    target.grab_focus();
    pump();
    assert_eq!(focused(ui).as_ref(), Some(target.upcast_ref()));
    assert_eq!(scroll_position(ui), PARKED, "focusing it moved nothing");

    press_ctrl_k(ui);
    assert!(cursor_in_search(ui));
    ui.palette.handle_key(gdk::Key::Escape);
    for _ in 0..3 {
        pump();
    }

    assert!(!ui.palette.is_open());
    assert_eq!(
        focused(ui).as_ref(),
        Some(target.upcast_ref()),
        "the cursor goes back where it was"
    );
    assert_eq!(
        scroll_position(ui),
        PARKED,
        "Esc must not scroll the canvas"
    );
    drop(overlay);
}

/// The widget that had the cursor is gone by the time the palette closes:
/// the cursor goes nowhere rather than wherever GTK would pick.
#[gtk::test]
fn gtk_ui_palette_closing_after_the_previous_widget_left_parks_the_cursor_nowhere() {
    const PARKED: (f64, f64) = (60.0, 40.0);
    let built = window();
    let ui = &built.0;
    ui.viewer.view_stack.set_visible_child_name(EDITOR_PAGE);
    let (overlay, target) = page_with_a_button(ui);
    park(ui, PARKED);
    target.grab_focus();
    pump();

    press_ctrl_k(ui);
    overlay.remove_overlay(&target);
    drop(target);
    pump();
    ui.palette.handle_key(gdk::Key::Escape);
    for _ in 0..3 {
        pump();
    }

    assert!(!ui.palette.is_open());
    assert_eq!(focused(ui), None);
    assert_eq!(
        scroll_position(ui),
        PARKED,
        "closing must not scroll the canvas"
    );
}

/// A page-sized widget on the canvas with a focusable button on it, both
/// allocated.
fn page_with_a_button(ui: &BuiltUi) -> (Overlay, Button) {
    let page = Picture::new();
    page.set_width_request(2_000);
    page.set_height_request(1_500);
    let overlay = Overlay::new();
    overlay.set_child(Some(&page));
    overlay.set_halign(gtk::Align::Center);
    let target = Button::with_label("On the page");
    target.set_halign(gtk::Align::Start);
    target.set_valign(gtk::Align::Start);
    target.set_margin_start(160);
    target.set_margin_top(120);
    overlay.add_overlay(&target);
    ui.viewer.pages.append(&overlay);
    settle(&target);
    (overlay, target)
}

fn park(ui: &BuiltUi, at: (f64, f64)) {
    ui.viewer.scroll.hadjustment().set_value(at.0);
    ui.viewer.scroll.vadjustment().set_value(at.1);
    pump();
    assert_eq!(
        scroll_position(ui),
        at,
        "the canvas has to start away from the origin, \
         or a jump cannot be told from a no-op"
    );
}

fn scroll_position(ui: &BuiltUi) -> (f64, f64) {
    (
        ui.viewer.scroll.hadjustment().value(),
        ui.viewer.scroll.vadjustment().value(),
    )
}

/// Spanish, typed without its accent rules mattering: "anotar" selects the
/// Annotate mode, Enter runs it — the screen its button lands on — and the
/// palette is gone.
#[gtk::test]
fn gtk_ui_palette_typing_a_command_and_pressing_enter_runs_it_and_closes() {
    let _spanish = speak(Language::Spanish);
    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);

    press_ctrl_k(ui);
    type_query(ui, "anotar");
    assert_eq!(selected_id(ui), Some("mode.annotate"));
    press_enter(ui);

    assert!(!ui.palette.is_open());
    assert_eq!(
        screen(&ui.viewer),
        (Some(EDITOR_PAGE.into()), Some(ANNOTATE_PAGE.into()))
    );
}

/// A refused command stays listed, selectable, dimmed, with its reason; Enter
/// on it runs nothing and leaves the palette open.
#[gtk::test]
fn gtk_ui_palette_a_refused_command_shows_why_and_enter_leaves_it_open() {
    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);
    if let Some(session) = ui.viewer.state.borrow_mut().session.as_mut() {
        session.annotation_access = AnnotationAccess::Forbidden;
    }
    crate::app::annotations::update_annotation_controls(&ui.viewer);
    let refusal = ui
        .viewer
        .annotation_editing_refusal()
        .expect("the fixture forbids annotation");

    press_ctrl_k(ui);
    type_query(ui, "highlight");
    assert_eq!(selected_id(ui), Some("annotate.highlight"));
    assert!(
        row_texts(ui, 0).contains(&refusal),
        "the reason is on the row: {:?}",
        row_texts(ui, 0)
    );
    let row = ui.palette.inner.view.list.row_at_index(0).unwrap();
    assert!(row.has_css_class(view::REFUSED_CLASS));

    press_enter(ui);

    assert!(
        ui.palette.is_open(),
        "a refused command keeps the palette up"
    );
    assert_eq!(ui.viewer.state.borrow().active_tool, None::<Tool>);
}

/// With an empty query, what ran this session comes first, most recent
/// first; a refused command that did not run is not one of them.
#[gtk::test]
fn gtk_ui_palette_commands_run_this_session_lead_an_empty_query() {
    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);
    ui.viewer.undo_action.set_enabled(false);

    run_by_click(ui, "mode.annotate");
    assert!(!ui.palette.is_open());
    run_by_click(ui, "mode.home");
    assert!(!ui.palette.is_open());
    run_by_click(ui, "edit.undo");
    assert!(ui.palette.is_open(), "nothing to undo: refused, still open");
    ui.palette.close();

    press_ctrl_k(ui);
    let mut expected = vec!["mode.home", "mode.annotate"];
    expected.extend(
        catalog()
            .iter()
            .map(|entry| entry.id)
            .filter(|id| !["mode.home", "mode.annotate"].contains(id)),
    );
    assert_eq!(shown_ids(ui), expected);
}

/// Decision 4 of T-229's note, settled here: Find from another screen takes
/// the user to the editor and opens its search.
#[gtk::test]
fn gtk_ui_palette_find_from_home_goes_to_the_editor_and_opens_search() {
    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);
    assert_eq!(
        ui.viewer.view_stack.visible_child_name().as_deref(),
        Some(HOME_PAGE)
    );

    run_by_click(ui, "edit.find");

    assert_eq!(
        ui.viewer.view_stack.visible_child_name().as_deref(),
        Some(EDITOR_PAGE)
    );
    let popover = ui
        .viewer
        .search_entry
        .ancestor(gtk::Popover::static_type())
        .and_downcast::<gtk::Popover>()
        .expect("the search field lives in a popover");
    assert!(popover.is_visible());
    popover.popdown();
}

/// Decision 6's measurement. GDK has no public way to build a key event,
/// so the delivery is asserted structurally: GTK puts application
/// accelerators on one shortcut controller on the window, and the phase
/// that controller runs in decides whether it sees Ctrl+K before a focused
/// text field does. Capture runs from the window down, ahead of the
/// focused `GtkText`'s own key bindings.
#[gtk::test]
fn gtk_ui_palette_ctrl_k_is_resolved_before_a_focused_text_field() {
    let built = window();
    let ui = &built.0;
    // A text field holds the cursor, as an annotation or content-edit box
    // would; the controller below is the window's, whoever has focus.
    let field = gtk::Entry::new();
    ui.viewer.pages.append(&field);
    settle(&field);
    field.grab_focus();
    pump();
    assert!(focused(ui).is_some_and(|widget| widget.is_ancestor(&field)));

    let controllers = ui.window.observe_controllers();
    let mut found = Vec::new();
    for index in 0..controllers.n_items() {
        let Some(controller) = controllers
            .item(index)
            .and_downcast::<gtk::ShortcutController>()
        else {
            continue;
        };
        for item in 0..controller.n_items() {
            let Some(shortcut) = controller.item(item).and_downcast::<gtk::Shortcut>() else {
                continue;
            };
            let named = shortcut
                .action()
                .and_downcast::<gtk::NamedAction>()
                .map(|action| action.action_name().to_string());
            if named.as_deref() == Some("win.command-palette") {
                found.push((
                    controller.propagation_phase(),
                    shortcut
                        .trigger()
                        .map(|trigger| trigger.to_str().to_string()),
                ));
            }
        }
    }

    // GTK 4 lists the application's accelerators on more than one of the
    // window's shortcut controllers (two, measured); every one of them must
    // run in the capture phase.
    assert!(
        !found.is_empty(),
        "Ctrl+K is on a window shortcut controller"
    );
    for (phase, trigger) in found {
        assert_eq!(phase, gtk::PropagationPhase::Capture);
        assert_eq!(trigger.as_deref(), Some("<Control>k"));
    }
}

/// Esc is also `win.clear-content-selection`'s accelerator, and application
/// accelerators run in the capture phase ahead of the palette. While the
/// palette is open that action must be off, so the first Esc reaches the
/// palette and closes it with the selection untouched — and it must come
/// back on however the palette closes.
#[gtk::test]
fn gtk_ui_palette_escape_closes_it_first_and_keeps_a_content_selection() {
    use crate::app::content_edit::multi::{has_selection, CLEAR_ACTION};
    use crate::app::state::ContentSelection;

    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);
    if let Some(session) = ui.viewer.state.borrow_mut().session.as_mut() {
        session.content_selection = Some(ContentSelection {
            page_index: 0,
            members: Default::default(),
        });
    }
    crate::app::update_content_edit_controls(&ui.viewer);
    let clear = ui
        .window
        .lookup_action(CLEAR_ACTION)
        .expect("build_ui installs the clear action");
    assert!(clear.is_enabled(), "a selection arms Esc's clear action");

    for how in ["Esc", "Ctrl+K", "a click outside"] {
        press_ctrl_k(ui);
        assert!(ui.palette.is_open(), "{how}");
        assert!(
            !clear.is_enabled(),
            "{how}: while the palette is open Esc must not be the clear action's"
        );

        match how {
            "Esc" => {
                ui.palette.handle_key(gdk::Key::Escape);
            }
            "Ctrl+K" => press_ctrl_k(ui),
            _ => ui.palette.close(),
        }
        pump();

        assert!(!ui.palette.is_open(), "{how}: one press closes it");
        assert!(
            has_selection(&ui.viewer),
            "{how}: the selection is untouched"
        );
        assert!(clear.is_enabled(), "{how}: Esc clears the selection again");
    }
}
