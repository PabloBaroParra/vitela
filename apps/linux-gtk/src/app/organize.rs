//! The "Organize pages" screen: a thumbnail grid the user can drag to
//! reorder, delete from, and save — the third page of the window's view
//! `Stack`, alongside `home::HOME_PAGE`/`home::EDITOR_PAGE`.
//!
//! ## Why a third stack page rather than a tools-panel tab
//!
//! Every other document-properties surface in this shell (`metadata`,
//! `sign`, forms) is a permanent tab inside the editor's tools panel — it
//! sits *beside* the page canvas. Organizing pages needs the opposite: the
//! canvas itself replaced by a grid, so there is room to lay out every page
//! at once. `home::show_home`/`home::show_editor` already establish the
//! pattern for a top-level view swap; this module is the third case of it.
//!
//! ## Why a drop re-renders the whole grid rather than reordering widgets
//!
//! A drag-and-drop move only changes which position a page's thumbnail sits
//! at, never what is drawn on it, so reordering the already-rendered card
//! widgets in place looks like the obvious approach — and is what this
//! module did originally. It doesn't work on this GTK4 build:
//! `gtk_flow_box_remove` doesn't release a widget's parent, not
//! synchronously and not even a full main-loop iteration later, so any
//! `insert`/`append` of that same widget back into the grid fails
//! `gtk_flow_box_child_set_child`'s assertion and silently drops the card
//! (see `handle_drop`'s own doc for the repro). `handle_drop` instead calls
//! [`populate_grid`] again after every move — the same rebuild the screen's
//! own opening already does — trading a redundant pdfium render per page on
//! every drag for correctness. `delete_page` only ever removes a card
//! (never reparents one back in), so it keeps the cheaper in-place
//! `grid.remove`.
//!
//! ## Why `Command::MovePage`/`RemovePage`, not a direct `pdf_manip` call
//!
//! `pdf_manip::reorder_pages`/`delete_pages` operate on the whole lopdf
//! object directly and know nothing about undo. Recording a `Command`
//! instead keeps this feature on the same undo/redo log as every other edit
//! (Ctrl+Z from the editor undoes the last move/delete here) — `pdf-save`'s
//! `replay_page_ops` already knows how to turn the resulting `Document.pages`
//! order into the right `pdf_manip` calls at save time, so this module never
//! calls `pdf_manip` itself.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gdk, gdk_pixbuf, gio, glib, ApplicationWindow, Box as GtkBox, Button, DragSource, DropTarget,
    FlowBox, Label, Orientation, Picture, PolicyType, ScrolledWindow, SelectionMode,
};
use pdf_document::{Command, Document};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderOptions};

use crate::app::document::show_save_chooser;
use crate::app::icons::{build_icon, Icon, ACCENT_TINT};
use crate::app::render::render_result;
use crate::app::state::{
    DocumentSession, OrganizePanel, RenderedPage, Viewer, CONTENT_MODEL_UNAVAILABLE,
};

use super::tools_panel::panel_heading;

pub(crate) const ORGANIZE_PAGE: &str = "organize";

const NO_DOCUMENT: &str = "Open a PDF before organizing its pages.";

/// Logical card size. Larger than Home's recents preview (`THUMB_WIDTH_PX`
/// there is 108): this grid is the whole point of the screen, not one card
/// among several.
const CARD_WIDTH_PX: i32 = 140;
const CARD_HEIGHT_PX: i32 = 180;
const CARDS_PER_ROW: u32 = 5;

const POINTS_PER_INCH: f64 = 72.0;

/// `grid` is homogeneous (see [`build_organize_panel`]) so a row with fewer
/// than [`CARDS_PER_ROW`] cards stretches each card well past
/// `CARD_WIDTH_PX`/`CARD_HEIGHT_PX` to fill the line — GTK4 CSS has no
/// `max-width`/`max-height` to cap that. Rendering at this multiple of the
/// logical card size instead of 1x gives the stretch somewhere to land
/// without going blocky; the DPI is still clamped in [`thumbnail_dpi`].
const RENDER_HEADROOM: i32 = 3;

/// Builds the screen's static chrome — no signal wiring beyond the grid's
/// own drop target, which needs no `&Viewer` any more than the drop target
/// in `home::hero::build_drop_zone` does. [`connect_organize_panel`] wires
/// the Save button once `Viewer` exists, mirroring
/// `metadata::build_metadata_panel`/`connect_metadata_panel`'s own split.
///
/// Returns the panel (for `Viewer::organize`) and its container (for
/// `build_ui` to add to `view_stack`).
pub(crate) fn build_organize_panel() -> (OrganizePanel, GtkBox) {
    let root = GtkBox::new(Orientation::Vertical, 12);
    root.add_css_class("organize-page");
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    let header = GtkBox::new(Orientation::Horizontal, 12);
    let heading = panel_heading("Organize pages");
    heading.set_hexpand(true);
    header.append(&heading);
    let save = Button::with_label("Save");
    save.add_css_class("home-primary");
    header.append(&save);
    root.append(&header);

    let hint = Label::new(Some("Drag a page to reorder it."));
    hint.set_xalign(0.0);
    hint.add_css_class("recent-meta");
    root.append(&hint);

    let grid = FlowBox::new();
    grid.set_selection_mode(SelectionMode::None);
    grid.set_homogeneous(true);
    grid.set_row_spacing(16);
    grid.set_column_spacing(16);
    grid.set_max_children_per_line(CARDS_PER_ROW);
    grid.set_min_children_per_line(2);
    grid.set_valign(gtk::Align::Start);

    let cards = Rc::new(RefCell::new(Vec::new()));

    let scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(PolicyType::Never)
        .child(&grid)
        .build();
    root.append(&scroll);

    (
        OrganizePanel {
            grid,
            cards,
            save_button: save,
        },
        root,
    )
}

/// Wires the Save button. Called once from `build_ui`, right after the
/// `Viewer` struct (and so `viewer.organize`) exists — the organize twin of
/// `metadata::connect_metadata_panel`. Needs `window`, unlike that one,
/// because saving opens the same file chooser Ctrl+S does.
pub(crate) fn connect_organize_panel(window: &ApplicationWindow, viewer: &Viewer) {
    viewer.organize.save_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| show_save_chooser(&window, &viewer)
    });

    // One drop target for the whole grid rather than one per card: the
    // target position is wherever the pointer lands, which `FlowBox::
    // child_at_pos` already answers without needing a controller per cell.
    let drop_target = DropTarget::new(i32::static_type(), gdk::DragAction::MOVE);
    drop_target.connect_drop({
        let viewer = viewer.clone();
        move |_, value, x, y| handle_drop(&viewer, value, x, y)
    });
    viewer.organize.grid.add_controller(drop_target);
}

/// Switches to the Organize screen and (re)populates its grid for the
/// current session. Called from `home::tools::apply`'s `HomeTool::Organize`
/// arm — which, like every other tool, only runs once a document is open.
pub(crate) fn show(viewer: &Viewer) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    populate_grid(viewer);
    viewer.view_stack.set_visible_child_name(ORGANIZE_PAGE);
}

/// Clears the grid and rebuilds one card per page of the current session's
/// model, in `Document.pages` order, then kicks off one thumbnail render per
/// card. Safe to call with no session (leaves the grid empty) — `show`'s own
/// refusal check keeps that from happening on the path a user actually takes.
fn populate_grid(viewer: &Viewer) {
    let grid = viewer.organize.grid.clone();
    for (card, _) in viewer.organize.cards.borrow_mut().drain(..) {
        grid.remove(&card);
    }

    let (page_ids, handle) = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return;
        };
        let Some(model) = session.document_model.as_ref() else {
            return;
        };
        let page_ids: Vec<u32> = model.pages.iter().map(|page| page.id.0).collect();
        (page_ids, session.document)
    };

    for (position, pdfium_page_index) in page_ids.into_iter().enumerate() {
        let (card, picture, number_label) = build_card(viewer, &grid, &viewer.organize.cards);
        number_label.set_text(&(position + 1).to_string());
        grid.append(&card);
        viewer
            .organize
            .cards
            .borrow_mut()
            .push((card, number_label));
        spawn_thumbnail(viewer, handle, pdfium_page_index, picture);
    }
}

/// Builds one card: a thumbnail placeholder, its page-number label, and a
/// delete button — plus the drag source that lets the whole card be picked
/// up and dropped elsewhere in the grid. Returns the card, its `Picture` (for
/// `spawn_thumbnail` to fill in once the render lands), and its number label
/// (for `populate_grid`/`renumber` to keep current).
fn build_card(
    viewer: &Viewer,
    grid: &FlowBox,
    cards: &Rc<RefCell<Vec<(GtkBox, Label)>>>,
) -> (GtkBox, Picture, Label) {
    let card = GtkBox::new(Orientation::Vertical, 6);
    card.add_css_class("organize-card");

    let picture = Picture::new();
    picture.add_css_class("organize-thumb");
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_size_request(CARD_WIDTH_PX, CARD_HEIGHT_PX);
    card.append(&picture);

    let footer = GtkBox::new(Orientation::Horizontal, 6);
    let number_label = Label::new(None);
    number_label.set_hexpand(true);
    number_label.set_xalign(0.0);
    footer.append(&number_label);

    let delete_button = Button::new();
    delete_button.set_child(Some(&build_icon(Icon::Delete, 16, ACCENT_TINT)));
    delete_button.add_css_class("flat");
    delete_button.update_property(&[gtk::accessible::Property::Label("Delete page")]);
    delete_button.set_tooltip_text(Some("Delete page"));
    footer.append(&delete_button);
    card.append(&footer);

    let drag_source = DragSource::new();
    drag_source.set_actions(gdk::DragAction::MOVE);
    drag_source.connect_prepare({
        let card = card.clone();
        let cards = cards.clone();
        move |_, _, _| {
            let index = card_position(&cards, &card)?;
            Some(gdk::ContentProvider::for_value(&glib::Value::from(
                index as i32,
            )))
        }
    });
    card.add_controller(drag_source);

    delete_button.connect_clicked({
        let viewer = viewer.clone();
        let grid = grid.clone();
        let cards = cards.clone();
        let card = card.clone();
        move |_| {
            let Some(index) = card_position(&cards, &card) else {
                return;
            };
            delete_page(&viewer, index);
            grid.remove(&card);
            cards.borrow_mut().remove(index);
            renumber(&cards);
        }
    });

    (card, picture, number_label)
}

/// `card`'s position in `cards` — the single source of truth for "where is
/// this card right now", read fresh on every drag or delete rather than
/// cached on the card itself, so it can never disagree with what is on
/// screen.
fn card_position(cards: &Rc<RefCell<Vec<(GtkBox, Label)>>>, card: &GtkBox) -> Option<usize> {
    cards.borrow().iter().position(|(root, _)| root == card)
}

/// Relabels every card's page-number to its current position — cheap text
/// updates, never a re-render, called after any move or delete.
fn renumber(cards: &Rc<RefCell<Vec<(GtkBox, Label)>>>) {
    for (position, (_, label)) in cards.borrow().iter().enumerate() {
        label.set_text(&(position + 1).to_string());
    }
}

/// The grid's single drop handler: finds which card the pointer landed on,
/// records the move in `Document.pages` via `Command::MovePage`, then
/// rebuilds the grid from the model's new order.
///
/// This does not try to reorder the existing card widgets in place.
/// `gtk_flow_box_remove` on this GTK4 build does not release a widget's
/// parent — not synchronously inside this callback, and not even a full
/// main-loop iteration later via `glib::idle_add_local_once` (both tried and
/// reproduced with 2 cards, from=0 to=1: moving a card to become the *last*
/// one). Any `insert`/`append` of that widget back into the same grid then
/// fails `gtk_flow_box_child_set_child`'s assertion and silently drops the
/// card. [`populate_grid`] sidesteps it entirely by only ever building fresh
/// cards and appending them into an emptied grid — the same path the screen
/// already uses on open — at the cost of re-rendering every thumbnail on
/// every move rather than just reparenting one card.
fn handle_drop(viewer: &Viewer, value: &glib::Value, x: f64, y: f64) -> bool {
    let grid = &viewer.organize.grid;
    let cards = &viewer.organize.cards;

    let Ok(from) = value.get::<i32>() else {
        return false;
    };
    let from = from as usize;
    let Some(target) = grid.child_at_pos(x as i32, y as i32) else {
        return false;
    };
    let Some(target_widget) = target.child() else {
        return false;
    };
    let Some(to) = cards
        .borrow()
        .iter()
        .position(|(root, _)| root.upcast_ref::<gtk::Widget>().eq(&target_widget))
    else {
        return false;
    };
    if from == to || from >= cards.borrow().len() {
        return false;
    }

    move_page(viewer, from, to);
    populate_grid(viewer);
    true
}

fn spawn_thumbnail(
    viewer: &Viewer,
    handle: DocumentHandle,
    pdfium_page_index: u32,
    picture: Picture,
) {
    let scale_factor = picture.scale_factor().max(1) * RENDER_HEADROOM;
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let job = move || -> Result<RenderedPage, pdf_render::RenderError> {
                let renderer = PdfiumRenderer::new();
                let (width_pt, height_pt) = renderer
                    .page_size(handle, pdfium_page_index, Priority::Thumbnail)
                    .wait()?;
                let dpi = thumbnail_dpi(width_pt, height_pt, scale_factor);
                render_result(renderer.render_page(
                    handle,
                    pdfium_page_index,
                    dpi,
                    None,
                    RenderOptions::new(),
                    Priority::Thumbnail,
                ))
            };
            let Ok(Ok(page)) = gio::spawn_blocking(job).await else {
                return;
            };
            let still_current = viewer
                .state
                .borrow()
                .session
                .as_ref()
                .is_some_and(|session| session.document == handle);
            if !still_current {
                return;
            }
            let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
                &glib::Bytes::from_owned(page.pixels),
                gdk_pixbuf::Colorspace::Rgb,
                true,
                8,
                page.width as i32,
                page.height as i32,
                page.stride as i32,
            );
            picture.set_pixbuf(Some(&pixbuf));
        }
    });
}

/// The DPI that fits a `width_pt` x `height_pt` page inside [`CARD_WIDTH_PX`]
/// x [`CARD_HEIGHT_PX`] at `scale_factor` — the organize-grid twin of
/// `home::recents::thumbnail_dpi`. Kept as its own copy rather than shared:
/// two call sites and a ten-line pure function is not worth an abstraction.
fn thumbnail_dpi(width_pt: f32, height_pt: f32, scale_factor: i32) -> u32 {
    let scale = f64::from(scale_factor.max(1));
    let fit = |pixels: i32, points: f32| {
        f64::from(pixels) * scale * POINTS_PER_INCH / f64::from(points.max(1.0))
    };
    fit(CARD_WIDTH_PX, width_pt)
        .min(fit(CARD_HEIGHT_PX, height_pt))
        .floor()
        .clamp(8.0, 300.0) as u32
}

fn command(
    viewer: &Viewer,
    operation: impl FnOnce(&mut DocumentSession) -> Result<String, String>,
) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    let result = {
        let mut state = viewer.state.borrow_mut();
        match state.session.as_mut() {
            Some(session) => operation(session),
            None => Err(NO_DOCUMENT.to_string()),
        }
    };
    match result {
        Ok(message) => {
            if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
                session.edit_revision += 1;
                session.unsaved_to_disk = true;
            }
            viewer.status.set_text(&message);
        }
        Err(error) => viewer.status.set_text(&error),
    }
}

fn model(session: &mut DocumentSession) -> Result<&mut Document, String> {
    session
        .document_model
        .as_mut()
        .ok_or_else(|| CONTENT_MODEL_UNAVAILABLE.to_string())
}

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

fn move_page(viewer: &Viewer, from: usize, to: usize) {
    command(viewer, |session| {
        let document = model(session)?;
        if from >= document.pages.len() || to >= document.pages.len() {
            return Err("Invalid page position.".to_string());
        }
        apply_command(document, Command::MovePage { from, to });
        Ok(format!("Moved page {} to position {}.", from + 1, to + 1))
    });
}

fn delete_page(viewer: &Viewer, index: usize) {
    command(viewer, |session| {
        let document = model(session)?;
        let page = document
            .pages
            .get(index)
            .cloned()
            .ok_or_else(|| "Page no longer exists.".to_string())?;
        apply_command(document, Command::RemovePage { index, page });
        Ok(format!("Deleted page {}.", index + 1))
    });
}
