//! The Organize screen's "Pages" view: one card per model page, the drag
//! that reorders them, and the page numbers that follow.
//!
//! Split out of `organize` alongside [`super::command`] — this half owns the
//! widgets and the gesture, that half owns the edits, and [`thumbnail`] owns
//! the pdfium render behind each card. The screen's own build/connect/show
//! stays in the parent module.

use gtk::prelude::*;
use gtk::{gdk, glib, Box as GtkBox, Button, DragSource, FlowBox, Label, Orientation, Picture};
use pdf_render::DocumentHandle;

use crate::app::icons::{build_icon, Icon, ACCENT_TINT};
use crate::app::state::{Card, Cards, Viewer};

use super::command::{delete_page, move_page};
pub(in crate::app::organize) use thumbnail::spawn_thumbnail;

pub(in crate::app::organize) mod thumbnail;

/// Logical card size. Larger than Home's recents preview (`THUMB_WIDTH_PX`
/// there is 108): this grid is the whole point of the screen, not one card
/// among several.
const CARD_WIDTH_PX: i32 = 140;
const CARD_HEIGHT_PX: i32 = 180;

/// Renders the thumbnails of cards that do not have one yet, against the
/// handle as it is now, and leaves every painted card alone.
///
/// Falls back to a full [`populate_grid`] when the grid and the model
/// disagree about how many pages there are: the two are kept in step card for
/// card, so a mismatch means an edit reached the model by a path that never
/// told the grid, and guessing which card belongs to which page from there
/// would paint the wrong page's picture onto a card.
pub(super) fn fill_missing_thumbnails(viewer: &Viewer) {
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
        spawn_thumbnail(
            viewer,
            handle,
            backend_index as u32,
            card.picture.clone(),
            (CARD_WIDTH_PX, CARD_HEIGHT_PX),
        );
    }
}

/// A grid child's position in `cards` — the key [`super::build_organize_panel`]'s
/// sort function orders by.
///
/// A child that is not in `cards` sorts last rather than panicking: GTK is
/// free to compare a child at any point, including one [`populate_grid`] has
/// appended but not registered yet, and an arbitrary-but-stable answer there
/// is corrected by the `invalidate_sort` that follows any real reorder.
pub(super) fn sort_position(cards: &Cards, child: &gtk::FlowBoxChild) -> usize {
    child
        .child()
        .and_then(|widget| widget.downcast::<GtkBox>().ok())
        .and_then(|root| cards.position(&root))
        .unwrap_or(usize::MAX)
}

/// Clears the grid and rebuilds one card per page of the current session's
/// model, in `Document.pages` order, then kicks off one thumbnail render per
/// card. Safe to call with no session (leaves the grid empty) — `show`'s own
/// refusal check keeps that from happening on the path a user actually takes.
pub(super) fn populate_grid(viewer: &Viewer) {
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
            spawn_thumbnail(
                viewer,
                handle,
                backend_index as u32,
                card.picture,
                (CARD_WIDTH_PX, CARD_HEIGHT_PX),
            );
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
pub(super) fn handle_drop(viewer: &Viewer, value: &glib::Value, x: f64, y: f64) -> bool {
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
/// did to `Document.pages` (see `state::Cards::move_card` for the mirroring
/// itself).
///
/// Nothing is re-rendered and no widget changes parent. `cards` is the sort
/// key (see [`super::build_organize_panel`]), so reordering the vector *is* the
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
