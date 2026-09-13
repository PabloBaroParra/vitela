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

use card::{build_card, relabel_sources, renumber};
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

/// Shows the Pages view's content: a full [`populate_grid`], or — when the
/// cards already on the grid are the very cards a rebuild would produce —
/// only the cheap part of one.
///
/// Building a card is a `Box`, a `Picture`, three labels and two buttons, and
/// measurement put that at about 2.2 ms per card on this shell
/// (`organize::tests::measure`): on a four-hundred-page assembly a rebuild is
/// nearly a second of blocked main loop, and leaving a view and coming back
/// used to pay it in full for a grid that had not changed by one page. The
/// thumbnail cache (see [`super::cache`]) had already made the *renders*
/// free; this is the widget half of the same answer.
///
/// The test for "has not changed" is the card order itself rather than a
/// revision counter, because the cards *are* the record of what the grid
/// holds: `drop::reorder_cards` moves them on a drag, [`populate_grid`]
/// rebuilds them, and an import appends to them. A counter would be a second
/// truth to keep in step with the first, and the first is already exact.
pub(super) fn fill_grid(viewer: &Viewer) {
    if !grid_holds_the_model(viewer) || viewer.organize.thumbnails_stale.get() {
        populate_grid(viewer);
        return;
    }
    // What a rebuild would have changed about a card that survives it: its
    // position label, its provenance line, and a thumbnail it never got. Each
    // is already maintained by whichever path edits the grid, so in practice
    // these three find nothing to do — they are here so that reusing the
    // cards is equivalent to rebuilding them without that being a claim about
    // every caller.
    renumber(&viewer.organize.cards);
    relabel_sources(viewer);
    fill_missing_thumbnails(viewer);
}

/// Whether the grid holds exactly one card per model page, in the model's
/// order — the condition under which rebuilding it would put the same pages
/// back in the same places.
///
/// Compares page identities and not lengths: a move keeps the count and
/// changes the order, which is precisely the case a length check would wave
/// through.
fn grid_holds_the_model(viewer: &Viewer) -> bool {
    let cards = viewer.organize.cards.snapshot();
    let state = viewer.state.borrow();
    let Some(model) = state
        .session
        .as_ref()
        .and_then(|session| session.document_model.as_ref())
    else {
        // No model to compare against: let `populate_grid` decide what an
        // empty session leaves on the grid, rather than deciding it twice.
        return false;
    };
    cards.len() == model.pages.len()
        && cards
            .iter()
            .zip(&model.pages)
            .all(|(card, page)| card.id == page.id)
}

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
