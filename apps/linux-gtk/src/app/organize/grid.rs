//! The Organize screen's "Pages" view: one card per model page, in
//! `Document.pages` order.
//!
//! Split out of `organize` alongside [`super::command`] — this half owns the
//! widgets and the gestures, that half owns the edits. Within it, [`card`]
//! owns what one card is made of, [`drop`] owns where a dragged page lands,
//! and [`thumbnail`] owns the pdfium render behind each card, the same
//! three-way split [`super::documents`] makes for the block view. The
//! screen's own build/connect/show stays in the parent module.

use gtk::prelude::*;
use gtk::Box as GtkBox;
use pdf_document::PageId;
use pdf_render::DocumentHandle;

use crate::app::state::{Cards, Viewer};

use card::{build_card, relabel_sources};
/// Re-exported for the screen's tests, which need the size a page card asks
/// for to look its own thumbnail up in the cache.
pub(in crate::app::organize) use card::{CARD_HEIGHT_PX, CARD_WIDTH_PX};
pub(in crate::app::organize) use drop::connect_drop;
pub(in crate::app::organize) use thumbnail::spawn_thumbnail;

mod card;
/// Visible to the screen's tests, which drive `drop::handle_drop` directly —
/// a synthetic `GtkDropTarget` drag is not something a `#[gtk::test]` can
/// stage.
pub(in crate::app::organize) mod drop;
pub(in crate::app::organize) mod thumbnail;

/// Renders the thumbnails of cards that do not have one yet, against the
/// handle as it is now, and leaves every painted card alone.
///
/// Falls back to a full [`populate_grid`] when the grid and the model
/// disagree about how many pages there are: the two are kept in step card for
/// card, so a mismatch means an edit reached the model by a path that never
/// told the grid, and guessing which card belongs to which page from there
/// would paint the wrong page's picture onto a card.
pub(super) fn fill_missing_thumbnails(viewer: &Viewer) {
    let Some((rows, handle)) = page_rows(viewer) else {
        return;
    };
    let cards = viewer.organize.cards.snapshot();
    if cards.len() != rows.len() {
        populate_grid(viewer);
        return;
    }
    for (card, row) in cards.iter().zip(rows) {
        let Some(backend_index) = row.backend else {
            continue;
        };
        if card.picture.paintable().is_some() {
            continue;
        }
        spawn_thumbnail(
            viewer,
            handle,
            card.id,
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

    let Some((rows, handle)) = page_rows(viewer) else {
        return;
    };

    for (position, row) in rows.into_iter().enumerate() {
        let card = build_card(viewer, &grid, &viewer.organize.cards, row.id);
        card.number.set_text(&(position + 1).to_string());
        // Registered before it is appended, not after: `grid.append` sorts the
        // new child by `sort_position`, which can only answer for a card
        // `cards` already holds.
        viewer.organize.cards.push(card.clone());
        grid.append(&card.root);
        if let Some(backend_index) = row.backend {
            spawn_thumbnail(
                viewer,
                handle,
                card.id,
                backend_index as u32,
                card.picture,
                (CARD_WIDTH_PX, CARD_HEIGHT_PX),
            );
        }
    }
    relabel_sources(viewer);
}

/// One model page, flattened into what a card needs — the twin of
/// [`super::documents::Row`], one per page instead of one per block.
struct PageRow {
    /// The page's stable identity: the card's id, and its drag payload.
    id: PageId,
    /// That page's index in the open pdfium handle, when it holds it yet.
    ///
    /// The model order and the handle's are not the same number while a page
    /// op is waiting for its preview refresh, and `id.0` is neither of them
    /// once pages can be imported, so every thumbnail is asked for through
    /// this rather than off a grid position. `None` is a page the handle does
    /// not hold yet (an insert whose refresh has not landed): its card shows
    /// the placeholder until it does.
    backend: Option<usize>,
}

/// Every page of the current model, in `Document.pages` order — or `None`
/// when there is no session or no model to read, which leaves every caller's
/// grid as it found it.
fn page_rows(viewer: &Viewer) -> Option<(Vec<PageRow>, DocumentHandle)> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    let model = session.document_model.as_ref()?;
    let rows = model
        .pages
        .iter()
        .map(|page| PageRow {
            id: page.id,
            backend: session.backend_index(page.id),
        })
        .collect();
    Some((rows, session.document))
}
