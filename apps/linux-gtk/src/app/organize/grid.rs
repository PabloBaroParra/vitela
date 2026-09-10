//! The Organize screen's thumbnail grid: building one card per model page,
//! rendering its thumbnail, and keeping the page numbers in step.
//!
//! Split out of `organize` alongside [`super::command`] — this half owns the
//! widgets and the pixels, that half owns the edits. The screen's own
//! build/connect/show lives in the parent module, which is also where the
//! drop handler that drives [`renumber`] sits.

use gtk::prelude::*;
use gtk::{
    gdk, gdk_pixbuf, gio, glib, Box as GtkBox, Button, DragSource, FlowBox, Label, Orientation,
    Picture,
};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderOptions};

use crate::app::icons::{build_icon, Icon, ACCENT_TINT};
use crate::app::render::render_result;
use crate::app::state::{Card, Cards, RenderedPage, Viewer};

use super::command::delete_page;

/// Logical card size. Larger than Home's recents preview (`THUMB_WIDTH_PX`
/// there is 108): this grid is the whole point of the screen, not one card
/// among several.
const CARD_WIDTH_PX: i32 = 140;
const CARD_HEIGHT_PX: i32 = 180;

const POINTS_PER_INCH: f64 = 72.0;

/// `grid` is homogeneous (see [`super::build_organize_panel`]) so a row with fewer
/// than [`super::CARDS_PER_ROW`] cards stretches each card well past
/// `CARD_WIDTH_PX`/`CARD_HEIGHT_PX` to fill the line — GTK4 CSS has no
/// `max-width`/`max-height` to cap that. Rendering at this multiple of the
/// logical card size instead of 1x gives the stretch somewhere to land
/// without going blocky; the DPI is still clamped in [`thumbnail_dpi`].
const RENDER_HEADROOM: i32 = 3;

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
        spawn_thumbnail(viewer, handle, backend_index as u32, card.picture.clone());
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
pub(super) fn renumber(cards: &Cards) {
    for (position, card) in cards.snapshot().iter().enumerate() {
        card.number.set_text(&(position + 1).to_string());
    }
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
    if super::tests::capture_thumbnail(pdfium_page_index, &picture) {
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
