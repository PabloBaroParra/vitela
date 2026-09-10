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
    gdk, gdk_pixbuf, gio, glib, ApplicationWindow, Box as GtkBox, Button, DragSource, DropTarget,
    FlowBox, Label, Orientation, Picture, PolicyType, ProgressBar, ScrolledWindow, SelectionMode,
};
use pdf_document::{Command, Document};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderOptions};

use crate::app::document::show_save_chooser;
use crate::app::icons::{build_icon, Icon, ACCENT_TINT};
use crate::app::render::render_result;
use crate::app::state::{
    Card, Cards, DocumentSession, OrganizePanel, RenderedPage, Viewer, CONTENT_MODEL_UNAVAILABLE,
};

use super::tools_panel::panel_heading;

pub(crate) const ORGANIZE_PAGE: &str = "organize";

const NO_DOCUMENT: &str = "Open a PDF before organizing its pages.";
mod import;

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
/// swapped is dropped by [`spawn_thumbnail`]'s own guard.
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

/// Renders the thumbnails of cards that do not have one yet, against the
/// handle as it is now, and leaves every painted card alone.
///
/// Falls back to a full [`populate_grid`] when the grid and the model
/// disagree about how many pages there are: the two are kept in step card for
/// card, so a mismatch means an edit reached the model by a path that never
/// told the grid, and guessing which card belongs to which page from there
/// would paint the wrong page's picture onto a card.
fn fill_missing_thumbnails(viewer: &Viewer) {
    let Some((backend_indexes, handle)) = backend_indexes(viewer) else {
        return;
    };
    let cards = viewer.organize.cards.snapshot();
    if cards.len() != backend_indexes.len() {
        populate_grid(viewer);
        return;
    }
    for (card, backend_index) in cards.iter().zip(backend_indexes) {
        let Some(backend_index) = backend_index else {
            continue;
        };
        if card.picture.paintable().is_some() {
            continue;
        }
        spawn_thumbnail(viewer, handle, backend_index as u32, card.picture.clone());
    }
}

/// A grid child's position in `cards` — the key [`build_organize_panel`]'s
/// sort function orders by.
///
/// A child that is not in `cards` sorts last rather than panicking: GTK is
/// free to compare a child at any point, including one [`populate_grid`] has
/// appended but not registered yet, and an arbitrary-but-stable answer there
/// is corrected by the `invalidate_sort` that follows any real reorder.
fn sort_position(cards: &Cards, child: &gtk::FlowBoxChild) -> usize {
    child
        .child()
        .and_then(|widget| widget.downcast::<GtkBox>().ok())
        .and_then(|root| cards.position(&root))
        .unwrap_or(usize::MAX)
}

pub(crate) fn document_changed(viewer: &Viewer) {
    import::document_changed(viewer);
}

/// Clears the grid and rebuilds one card per page of the current session's
/// model, in `Document.pages` order, then kicks off one thumbnail render per
/// card. Safe to call with no session (leaves the grid empty) — `show`'s own
/// refusal check keeps that from happening on the path a user actually takes.
fn populate_grid(viewer: &Viewer) {
    let grid = viewer.organize.grid.clone();
    for card in viewer.organize.cards.take_all() {
        grid.remove(&card.root);
    }
    // A rebuild renders every card from the handle as it is now, which is
    // exactly what `thumbnails_stale` was asking for.
    viewer.organize.thumbnails_stale.set(false);

    let Some((backend_indexes, handle)) = backend_indexes(viewer) else {
        return;
    };

    for (position, backend_index) in backend_indexes.into_iter().enumerate() {
        let card = build_card(viewer, &grid, &viewer.organize.cards);
        card.number.set_text(&(position + 1).to_string());
        // Registered before it is appended, not after: `grid.append` sorts the
        // new child by `sort_position`, which can only answer for a card
        // `cards` already holds.
        viewer.organize.cards.push(card.clone());
        grid.append(&card.root);
        if let Some(backend_index) = backend_index {
            spawn_thumbnail(viewer, handle, backend_index as u32, card.picture);
        }
    }
}

/// One entry per *model* page, in model order, holding that page's index in
/// the open pdfium handle — plus the handle itself.
///
/// The two orders are not the same number while a page op is waiting for its
/// preview refresh, and `page.id.0` is neither of them once pages can be
/// imported, so every thumbnail is asked for through this rather than off a
/// grid position. `None` is a page the handle does not hold yet (an insert
/// whose refresh has not landed): its card shows the placeholder until it
/// does.
///
/// `None` for the whole thing means there is no session or no model to read —
/// every caller leaves the grid as it found it.
fn backend_indexes(viewer: &Viewer) -> Option<(Vec<Option<usize>>, DocumentHandle)> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    let model = session.document_model.as_ref()?;
    let backend_indexes = model
        .pages
        .iter()
        .map(|page| session.backend_index(page.id))
        .collect();
    Some((backend_indexes, session.document))
}

/// Builds one card: a thumbnail placeholder, its page-number label, and a
/// delete button — plus the drag source that lets the whole card be picked
/// up and dropped elsewhere in the grid.
fn build_card(viewer: &Viewer, grid: &FlowBox, cards: &Cards) -> Card {
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
            let index = cards.position(&card)?;
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
            let Some(index) = cards.position(&card) else {
                return;
            };
            if delete_page(&viewer, index) {
                grid.remove(&card);
                cards.remove(index);
                renumber(&cards);
            }
        }
    });

    Card {
        root: card,
        number: number_label,
        picture,
    }
}

/// Relabels every card's page-number to its current position — cheap text
/// updates, never a re-render, called after any move or delete.
fn renumber(cards: &Cards) {
    for (position, card) in cards.snapshot().iter().enumerate() {
        card.number.set_text(&(position + 1).to_string());
    }
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

/// Renders one card's thumbnail off the main thread and fills its `Picture`
/// in when the render lands.
///
/// The `cfg(test)` early return is the module's one test seam, and it is here
/// rather than in the tests because this is the only line where the pairing
/// under test — *which* pdfium page index a given card asked for — still
/// exists. `Document.pages` is reordered by `Command::MovePage`, so a card
/// that rendered by grid position instead of by page identity would still
/// look right in the model and wrong on screen; capturing the request is what
/// tells the two apart. The tests cannot let the real path run: their session
/// carries no pdfium document.
fn spawn_thumbnail(
    viewer: &Viewer,
    handle: DocumentHandle,
    pdfium_page_index: u32,
    picture: Picture,
) {
    #[cfg(test)]
    if tests::capture_thumbnail(pdfium_page_index, &picture) {
        return;
    }
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
) -> bool {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return false;
    }
    // Every operation that reaches this funnel changes the page list, which
    // is the PDF document-assembly permission (`/P` bit 11) and not the
    // modify-contents bit checked just above. A document can grant one and
    // withhold the other, so asking only the first would repaginate a file
    // that forbids exactly that — see `pdf_manip::document_assembly_is_allowed`.
    if let Some(refusal) = viewer.page_assembly_refusal() {
        viewer.status.set_text(refusal);
        return false;
    }
    // Permission granted is not the same as result writable. Moving or
    // deleting a page changes the page set, which puts the save on the
    // full-rewrite writer, and an encrypted document opened with only one of
    // its two passwords cannot be re-encrypted at all. Asked here rather than
    // at the save it would fail: the refresh below saves a snapshot the same
    // way, so an edit that can never be written would not merely wait to fail
    // — it would take the preview down with it on the very next operation.
    if let Some(refusal) = viewer.full_rewrite_refusal() {
        viewer.status.set_text(refusal);
        return false;
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
            super::annotations::update_annotation_controls(viewer);
            // A page op moves `Document.pages` out from under the open pdfium
            // handle, which still holds the pre-op order. Materializing it now
            // — the same in-memory save+reopen a content edit runs — is what
            // puts the two back in agreement; without it every page index the
            // canvas produces would keep naming the old page, and every id
            // resolved through `DocumentSession::backend_pages` would name the
            // new one. Bails out on its own (leaving the message above intact)
            // when there is no `save_backing` to replay against.
            super::document::refresh_preview(viewer, message);
            true
        }
        Err(error) => {
            viewer.status.set_text(&error);
            false
        }
    }
}

fn model(session: &mut DocumentSession) -> Result<&mut Document, String> {
    session
        .document_model
        .as_mut()
        .ok_or_else(|| CONTENT_MODEL_UNAVAILABLE.to_string())
}

/// Returns `false` when `EditLog::apply` rejected the command, leaving both
/// the document and the log untouched. Every caller turns that into an `Err`
/// rather than dropping it: a rejected command records nothing, so reporting
/// success would leave the status line — and the undo stack — describing an
/// edit that never happened.
fn apply_command(document: &mut Document, command: Command) -> bool {
    let mut log = std::mem::take(&mut document.pending_edits);
    let applied = log.apply(document, command);
    document.pending_edits = log;
    applied
}

fn move_page(viewer: &Viewer, from: usize, to: usize) -> bool {
    if from == to {
        return false;
    }
    command(viewer, |session| {
        let document = model(session)?;
        if from >= document.pages.len() || to >= document.pages.len() {
            return Err("Invalid page position.".to_string());
        }
        if !apply_command(document, Command::MovePage { from, to }) {
            return Err("Could not move the page.".to_string());
        }
        Ok(format!("Moved page {} to position {}.", from + 1, to + 1))
    })
}

fn delete_page(viewer: &Viewer, index: usize) -> bool {
    command(viewer, |session| {
        let document = model(session)?;
        // `Command::remove_page`, not a `RemovePage` literal: it captures the
        // annotations and form fields anchored to the page as well, so undo
        // brings the page back with what was drawn on it, and the removal
        // cannot strand an annotation on a page id `pdf-save` will refuse to
        // write (which would leave the document unsaveable, not just untidy).
        let removal = Command::remove_page(document, index)
            .ok_or_else(|| "Page no longer exists.".to_string())?;
        if !apply_command(document, removal) {
            return Err("Could not delete the page.".to_string());
        }
        Ok(format!("Deleted page {}.", index + 1))
    })
}

#[cfg(test)]
mod tests;
