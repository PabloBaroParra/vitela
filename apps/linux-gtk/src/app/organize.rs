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
//! ## The two views
//!
//! The screen shows the same page order either as one card per *document
//! block* ([`documents`]) or as one card per *page* ([`grid`]), and
//! [`views`] owns which of the two is on show. They are deliberately unlike
//! each other: a block list is one pdfium render per document and is rebuilt
//! whole on every change, while the page grid is one render per page and is
//! never rebuilt for a move — see [`reorder_cards`] for the GTK4 bug that
//! rules out the obvious alternative there.
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
    ApplicationWindow, Box as GtkBox, FlowBox, Orientation, PolicyType, ScrolledWindow,
    SelectionMode, Stack,
};

use crate::app::state::{Cards, OrganizePanel, Viewer};

use documents::{DOCUMENTS_VIEW, PAGES_VIEW};
use grid::{fill_missing_thumbnails, populate_grid, sort_position};
use views::{populate_visible, showing_documents};

pub(crate) const ORGANIZE_PAGE: &str = "organize";

pub(crate) use documents::ORGANIZE_CSS;

const NO_DOCUMENT: &str = "Open a PDF before organizing its pages.";
mod autoscroll;
pub(in crate::app::organize) mod cache;
mod command;
mod documents;
mod grid;
mod header;
mod import;
mod motion;
mod views;

pub(crate) use cache::Thumbnails;

const CARDS_PER_ROW: u32 = 5;

/// Builds the screen's static chrome — no signal wiring beyond the grid's
/// own drop target, which needs no `&Viewer` any more than the drop target
/// in `home::hero::build_drop_zone` does. [`connect_organize_panel`] wires
/// the rest once `Viewer` exists, mirroring
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

    let (header, controls) = header::build();
    root.append(&header);

    let (switch, documents_toggle, pages_toggle) = views::build_switch();
    root.append(&switch);

    let hint = views::build_hint();
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

    let pages_scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(PolicyType::Never)
        .child(&grid)
        .build();
    autoscroll::connect(&pages_scroll);

    let documents_list = GtkBox::new(Orientation::Vertical, 0);
    documents_list.set_valign(gtk::Align::Start);
    let documents_scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(PolicyType::Never)
        .child(&documents_list)
        .build();
    autoscroll::connect(&documents_scroll);

    let views = Stack::new();
    views.add_named(&documents_scroll, Some(DOCUMENTS_VIEW));
    views.add_named(&pages_scroll, Some(PAGES_VIEW));
    views.set_visible_child_name(DOCUMENTS_VIEW);
    motion::configure(&views);
    root.append(&views);

    (
        OrganizePanel {
            views,
            documents_toggle,
            pages_toggle,
            hint,
            documents_list,
            grid,
            cards,
            thumbnails_stale: Rc::new(Cell::new(false)),
            thumbnails: Thumbnails::new(),
            add_pdfs_button: controls.add_pdfs,
            import_progress: controls.import_progress,
            cancel_import_button: controls.cancel_import,
            extract_button: controls.extract,
            split_button: controls.split,
            save_button: controls.save,
        },
        root,
    )
}

/// Wires the screen's signals. Called once from `build_ui`, right after the
/// `Viewer` struct (and so `viewer.organize`) exists — the organize twin of
/// `metadata::connect_metadata_panel`. Needs `window`, unlike that one,
/// because the header's buttons open choosers that are transient for it.
pub(crate) fn connect_organize_panel(window: &ApplicationWindow, viewer: &Viewer) {
    views::connect(viewer);
    header::connect(window, viewer);
    grid::connect_drop(viewer);
}

/// Switches to the Organize screen and populates it for the current session.
/// Called from `home::tools::apply`'s `HomeTool::Organize` arm — which, like
/// every other tool, only runs once a document is open.
pub(crate) fn show(viewer: &Viewer) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    // Opens on Documents every time, not on whichever view was left behind:
    // the blocks are the shape of the assembled document, and the per-page
    // grid is the detail a user descends into from there.
    viewer.organize.documents_toggle.set_active(true);
    views::show(viewer, DOCUMENTS_VIEW);
    viewer.view_stack.set_visible_child_name(ORGANIZE_PAGE);
}

/// Rebuilds the view on show when this screen is the one on show, and does
/// nothing otherwise. Called from `annotations::command::history` after an
/// undo or redo that moved a page-structure command: those change
/// `Document.pages` behind the cards' backs, so what is left on screen would
/// keep the order the log has just reversed.
///
/// Gated on visibility rather than run unconditionally because a rebuild
/// costs a pdfium render per card (see [`populate_grid`]), and [`show`]
/// populates on the way in — a hidden view has nothing to keep current.
pub(crate) fn refresh_if_visible(viewer: &Viewer) {
    if viewer.view_stack.visible_child_name().as_deref() == Some(ORGANIZE_PAGE) {
        populate_visible(viewer);
    }
}

/// Marks every card's thumbnail as no longer trustworthy, so the next
/// `write::refresh_preview` rebuilds the grid instead of keeping the
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
    // The cache is keyed on page identity, and a content edit changes what a
    // page *looks like* without changing which page it is — so every entry
    // for it is now a picture of a page that no longer exists, under a key
    // that still matches. Emptying the cache is the only honest answer, and
    // the generation bump that comes with it retires the renders already in
    // flight against the pixels being replaced.
    viewer.organize.thumbnails.invalidate();
}

/// Marks one page's thumbnails as no longer trustworthy — the narrow twin of
/// [`invalidate_thumbnails`], for a rotation.
///
/// A quarter-turn is the one edit that repaints exactly one page: every other
/// card on the grid is still a picture of the page it names, at the angle it
/// still has. So this empties that page's cache entries ([`cache::Thumbnails::
/// forget_page`] explains why the generation stays put) and clears the card's
/// own `Picture`, which is what puts it back in the set
/// [`fill_missing_thumbnails`] re-renders once the reopen behind the rotation
/// lands. Without the second half the card keeps painting the `Pixbuf` it
/// already owns and the grid shows the page at its old angle for ever.
///
/// Takes a `PageId` and touches every card carrying it, rather than a grid
/// position: the callers are a card's own button and an undo/redo step, and
/// the second has only the id the command recorded.
pub(crate) fn invalidate_page_thumbnail(viewer: &Viewer, page: pdf_document::PageId) {
    viewer.organize.thumbnails.forget_page(page);
    for card in viewer.organize.cards.snapshot() {
        if card.id == page {
            card.picture.set_paintable(gtk::gdk::Paintable::NONE);
        }
    }
}

/// What the view on show needs after `write::refresh_preview`'s in-memory
/// save-and-reopen lands, which for the Pages grid is usually nothing.
///
/// The reopen swaps the pdfium handle every thumbnail was rendered against,
/// but a rendered thumbnail is a `Pixbuf` the card already owns — the new
/// handle does not invalidate it, only [`invalidate_thumbnails`] does. So the
/// work here is limited to cards that never got a thumbnail at all: an
/// inserted or imported page renders as a placeholder until the handle that
/// finally holds it exists, and a render still in flight when the handle was
/// swapped is dropped by `grid::thumbnail::spawn_thumbnail`'s own guard.
///
/// This used to be [`refresh_if_visible`], which meant every page move paid
/// for a second full re-render of the whole grid on top of the one the drop
/// had already done — on a fifty-page document, a hundred pdfium renders to
/// show a page in a different place.
pub(crate) fn refresh_after_reopen(viewer: &Viewer) {
    if viewer.view_stack.visible_child_name().as_deref() != Some(ORGANIZE_PAGE) {
        return;
    }
    // The Documents view has no thumbnail-preserving path to take: a move or
    // a delete can merge, split or drop whole blocks, so its cards are
    // rebuilt from the new page order either way. It is one render per
    // document rather than per page, which is what makes that affordable.
    if showing_documents(viewer) {
        documents::populate(viewer);
        return;
    }
    if viewer.organize.thumbnails_stale.get() {
        populate_grid(viewer);
        return;
    }
    fill_missing_thumbnails(viewer);
}

pub(crate) fn document_changed(viewer: &Viewer) {
    // Before anything else: `PageId`s are unique within one model and start
    // over at 0 in the next, so a surviving entry would hand the new
    // document's first page the old document's first page's picture.
    viewer.organize.thumbnails.clear();
    // The cards on the grid are pictures of the document being left, and the
    // `PageId`s that start over are exactly what stops `grid::fill_grid` from
    // seeing that: two documents of the same length present the same ids in
    // the same order, so the cheap check would call the old grid current and
    // reuse it — thumbnails and all — for a document it has never rendered.
    // The flag is what a rebuild is *for*, and this is its most extreme case:
    // not a page whose pixels changed, but every page at once.
    viewer.organize.thumbnails_stale.set(true);
    import::document_changed(viewer);
}

#[cfg(test)]
mod tests;
