//! One card of the Pages view: the widgets it is made of, the two labels
//! that are kept current without a re-render, and the delete button.
//!
//! Split out of [`super`] alongside [`super::drop`] the way the Documents
//! view splits into `documents::card` and `documents::gap`: this half owns
//! what a card *is*, that half owns where a dragged one lands, and the parent
//! owns the grid they live in.

use gtk::prelude::*;
use gtk::{gdk, glib, Box as GtkBox, Button, DragSource, FlowBox, Label, Orientation, Picture};
use pdf_document::{BlockSource, PageId};

use crate::app::icons::{build_icon, Icon, ACCENT_TINT};
use crate::app::state::{Card, Cards, Viewer};

use super::super::command::delete_page;
use super::super::documents::source_name;

/// Logical card size. Larger than Home's recents preview (`THUMB_WIDTH_PX`
/// there is 108): this grid is the whole point of the screen, not one card
/// among several.
pub(in crate::app::organize) const CARD_WIDTH_PX: i32 = 140;
pub(in crate::app::organize) const CARD_HEIGHT_PX: i32 = 180;

/// Builds one card: a thumbnail placeholder, the provenance line
/// [`relabel_sources`] fills in, its page-number label and a delete button —
/// plus the drag source that lets the whole card be picked up and dropped
/// elsewhere in the grid.
pub(super) fn build_card(viewer: &Viewer, grid: &FlowBox, cards: &Cards, id: PageId) -> Card {
    let card = GtkBox::new(Orientation::Vertical, 6);
    card.add_css_class("organize-card");

    let picture = Picture::new();
    picture.add_css_class("organize-thumb");
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_size_request(CARD_WIDTH_PX, CARD_HEIGHT_PX);
    card.append(&picture);

    // A line of its own under the thumbnail rather than a second column
    // beside the page number: a file name is as long as it is, and a card
    // 140px wide has no column to spare. Ellipsized with the full name on the
    // tooltip, so a long name costs the card its width and never its height.
    let source_label = Label::new(None);
    source_label.set_xalign(0.0);
    source_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    source_label.set_visible(false);
    source_label.add_css_class("organize-card-source");
    card.append(&source_label);

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
    // The page's own id, not the card's position: see [`super`]'s header.
    drag_source.connect_prepare(move |_, _, _| {
        Some(gdk::ContentProvider::for_value(&glib::Value::from(id.0)))
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
                // Deleting the last page of an imported PDF can leave a page
                // list with a single source in it, and every surviving card
                // still naming one.
                relabel_sources(&viewer);
            }
        }
    });

    Card {
        id,
        root: card,
        number: number_label,
        source: source_label,
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

/// Rewrites every card's provenance line from the model as it stands, and
/// hides the line on the cards that have nothing to say.
///
/// Called after a rebuild and after a delete — the two things that can change
/// *how many* sources the page list holds. A move cannot: it permutes pages
/// that keep the source they came in with, and each line travels with the
/// card it belongs to.
pub(super) fn relabel_sources(viewer: &Viewer) {
    let names = source_names(viewer);
    let cards = viewer.organize.cards.snapshot();
    if names.len() != cards.len() {
        return;
    }
    for (card, name) in cards.iter().zip(names) {
        match name {
            Some(name) => {
                card.source.set_text(&name);
                card.source.set_tooltip_text(Some(&name));
                card.source.set_visible(true);
            }
            None => {
                card.source.set_text("");
                card.source.set_tooltip_text(None);
                card.source.set_visible(false);
            }
        }
    }
}

/// The provenance line for each model page, in model order — `None` for a
/// page whose source needs no naming on its card.
///
/// Every page of a document assembled from a single PDF gets `None`: naming
/// the one source on all fifty cards is noise, and the checklist asks for
/// provenance "without overloading the card". The moment a second source is
/// in the page list, every card says which PDF its page came from, including
/// the pages of the document that was opened.
fn source_names(viewer: &Viewer) -> Vec<Option<String>> {
    let state = viewer.state.borrow();
    let Some(session) = state.session.as_ref() else {
        return Vec::new();
    };
    let Some(model) = session.document_model.as_ref() else {
        return Vec::new();
    };
    let sources: Vec<BlockSource> = model
        .pages
        .iter()
        .map(|page| BlockSource::from(page.origin))
        .collect();
    let mixed = sources.windows(2).any(|pair| pair[0] != pair[1]);
    if !mixed {
        return vec![None; sources.len()];
    }
    sources
        .into_iter()
        .map(|source| Some(source_name(session, source).unwrap_or_else(|| "Blank page".to_owned())))
        .collect()
}
