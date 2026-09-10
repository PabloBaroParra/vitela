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

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gdk, glib, ApplicationWindow, Box as GtkBox, Button, DropTarget, FlowBox, Label, Orientation,
    PolicyType, ProgressBar, ScrolledWindow, SelectionMode,
};

use crate::app::document::show_save_chooser;
use crate::app::state::{Cards, OrganizePanel, Viewer};

use super::tools_panel::panel_heading;
use command::move_page;
use grid::{fill_missing_thumbnails, populate_grid, renumber, sort_position};

pub(crate) const ORGANIZE_PAGE: &str = "organize";

const NO_DOCUMENT: &str = "Open a PDF before organizing its pages.";
mod command;
mod grid;
mod import;

const CARDS_PER_ROW: u32 = 5;

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

    let cards = Cards::new();

    // The grid's order is a *sort*, not an insertion order: `cards` is the
    // running truth for "which page is at which position", and this reads it.
    // Reordering that way is what lets [`reorder_cards`] move a page without
    // touching a single widget — see its own doc for why the obvious
    // remove/insert is not available here.
    grid.set_sort_func({
        let cards = cards.clone();
        move |a, b| {
            sort_position(&cards, a)
                .cmp(&sort_position(&cards, b))
                .into()
        }
    });

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
            thumbnails_stale: Rc::new(Cell::new(false)),
            add_pdfs_button: add_pdfs,
            import_progress,
            cancel_import_button: cancel_import,
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
    viewer.organize.add_pdfs_button.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| import::show_chooser(&window, &viewer)
    });
    viewer.organize.cancel_import_button.connect_clicked({
        let viewer = viewer.clone();
        move |_| import::cancel(&viewer)
    });
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

/// Rebuilds the grid when this screen is the one on show, and does nothing
/// otherwise. Called from `annotations::command::history` after an undo or
/// redo that moved a `MovePage`/`RemovePage`/`InsertPage` command: those
/// change `Document.pages` behind the grid's back, so the cards left on
/// screen would keep the order the log has just reversed.
///
/// Gated on visibility rather than run unconditionally because a rebuild
/// costs one pdfium render per page (see [`populate_grid`]), and [`show`]
/// populates the grid on the way in — a hidden grid has nothing to keep
/// current.
pub(crate) fn refresh_if_visible(viewer: &Viewer) {
    if viewer.view_stack.visible_child_name().as_deref() == Some(ORGANIZE_PAGE) {
        populate_grid(viewer);
    }
}

/// Marks every card's thumbnail as no longer trustworthy, so the next
/// `document::refresh_preview` rebuilds the grid instead of keeping the
/// pixels already on it.
///
/// Called from `annotations::command::history` when the step it replayed was
/// a *content* edit. That is the one thing that can repaint a page while this
/// screen holds cards for it: the Undo/Redo buttons sit in this screen's own
/// header, so a user standing in Organize can undo the text edit they made on
/// the editor and change a page's pixels without ever leaving the grid.
/// Page-structure steps do not need it — they shuffle, add or drop whole
/// pages, and never touch what a surviving page looks like.
pub(crate) fn invalidate_thumbnails(viewer: &Viewer) {
    viewer.organize.thumbnails_stale.set(true);
}

/// What the grid needs after `document::refresh_preview`'s in-memory
/// save-and-reopen lands, which is usually nothing.
///
/// The reopen swaps the pdfium handle every thumbnail was rendered against,
/// but a rendered thumbnail is a `Pixbuf` the card already owns — the new
/// handle does not invalidate it, only [`invalidate_thumbnails`] does. So the
/// work here is limited to cards that never got a thumbnail at all: an
/// inserted or imported page renders as a placeholder until the handle that
/// finally holds it exists, and a render still in flight when the handle was
/// swapped is dropped by `grid::spawn_thumbnail`'s own guard.
///
/// This used to be [`refresh_if_visible`], which meant every page move paid
/// for a second full re-render of the whole grid on top of the one the drop
/// had already done — on a fifty-page document, a hundred pdfium renders to
/// show a page in a different place.
pub(crate) fn refresh_after_reopen(viewer: &Viewer) {
    if viewer.view_stack.visible_child_name().as_deref() != Some(ORGANIZE_PAGE) {
        return;
    }
    if viewer.organize.thumbnails_stale.get() {
        populate_grid(viewer);
        return;
    }
    fill_missing_thumbnails(viewer);
}

pub(crate) fn document_changed(viewer: &Viewer) {
    import::document_changed(viewer);
}

/// The grid's single drop handler: finds which card the pointer landed on,
/// records the move in `Document.pages` via `Command::MovePage`, then moves
/// the card to match.
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
    let Some(target_card) = target
        .child()
        .and_then(|widget| widget.downcast::<GtkBox>().ok())
    else {
        return false;
    };
    let Some(to) = cards.position(&target_card) else {
        return false;
    };
    if from == to || from >= cards.len() {
        return false;
    }

    if !move_page(viewer, from, to) {
        return false;
    }
    reorder_cards(viewer, from, to);
    true
}

/// Moves the card at `from` to `to`, mirroring what `Command::MovePage` just
/// did to `Document.pages` (see `Cards::move_card` for the mirroring itself).
///
/// Nothing is re-rendered and no widget changes parent. `cards` is the sort
/// key (see [`build_organize_panel`]), so reordering the vector *is* the
/// reorder, and `invalidate_sort` is what tells the grid to read it again.
/// Each card keeps the `Picture` it was built with, which matters twice over:
/// the thumbnail it already painted is still that page's thumbnail, and a
/// render still in flight is still aimed at the card it was started for.
///
/// The obvious alternative — pulling the card out of the grid and inserting
/// it at the new index — is not available. `gtk_flow_box_remove` on this GTK4
/// build does not release a widget's parent: not synchronously inside this
/// callback, and not a full main-loop iteration later via
/// `glib::idle_add_local_once` either (both tried, reproduced with 2 cards,
/// from=0 to=1 — moving a card to become the *last* one). Any `insert` or
/// `append` of that widget back into the same grid then fails
/// `gtk_flow_box_child_set_child`'s assertion and silently drops the card.
/// Sorting never asks the grid to move anything, so it never meets that bug.
fn reorder_cards(viewer: &Viewer, from: usize, to: usize) {
    viewer.organize.cards.move_card(from, to);
    viewer.organize.grid.invalidate_sort();
    renumber(&viewer.organize.cards);
}

#[cfg(test)]
mod tests;
