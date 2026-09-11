//! The Organize screen's "Documents" view (checklist §9): the selector, the
//! block cards, and the two ways of reordering or deleting a whole block.
//!
//! The per-page grid's tests are the parent module's; these never touch it.

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Label};
use pdf_document::{
    Document, ImportedDocumentId, Orientation as PageOrientation, Page, PageId, PageSize, Rotation,
};

use super::super::documents::{DOCUMENTS_HINT, DOCUMENTS_VIEW, PAGES_HINT, PAGES_VIEW};
use super::with_documents;
use crate::app::state::{ImportedSource, Viewer};

pub(super) fn base_page(id: u32) -> Page {
    Page::base(
        PageId(id),
        id,
        PageSize::A4,
        PageOrientation::Portrait,
        Rotation::None,
    )
}

pub(super) fn imported_page(id: u32, source: u64) -> Page {
    Page::imported(
        PageId(id),
        ImportedDocumentId(source),
        0,
        PageSize::A4,
        PageOrientation::Portrait,
        Rotation::None,
    )
}

pub(super) fn source(id: u64, name: &str) -> ImportedSource {
    ImportedSource {
        id: ImportedDocumentId(id),
        document: pdf_manip::LopdfDocument::from_lopdf(lopdf::Document::new()),
        name: name.to_owned(),
    }
}

/// Two base pages followed by three imported ones: two blocks, which is the
/// smallest model with a move that is not a no-op.
pub(super) fn two_blocks() -> Document {
    let mut document = Document::blank();
    document.pages = vec![
        base_page(0),
        base_page(1),
        imported_page(2, 7),
        imported_page(3, 7),
        imported_page(4, 7),
    ];
    document
}

/// The list's children in order: gap, card, gap, card, … gap.
pub(super) fn children(viewer: &Viewer) -> Vec<gtk::Widget> {
    let mut children = Vec::new();
    let mut child = viewer.organize.documents_list.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        children.push(widget);
    }
    children
}

pub(super) fn cards(viewer: &Viewer) -> Vec<GtkBox> {
    children(viewer)
        .into_iter()
        .filter(|widget| widget.has_css_class("organize-block"))
        .map(|widget| widget.downcast().expect("a block card is a GtkBox"))
        .collect()
}

/// A card's two lines of text — its title and its "N pages · first–last".
pub(super) fn card_text(card: &GtkBox) -> (String, String) {
    let details = card.first_child().unwrap().next_sibling().unwrap();
    let name: Label = details
        .first_child()
        .unwrap()
        .downcast()
        .expect("the name label");
    let meta: Label = name.next_sibling().unwrap().downcast().expect("the meta");
    (name.text().to_string(), meta.text().to_string())
}

/// A card's Move up, Move down and Delete buttons, in that order.
pub(super) fn card_buttons(card: &GtkBox) -> Vec<Button> {
    let controls = card.last_child().unwrap();
    let mut buttons = Vec::new();
    let mut child = controls.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        buttons.push(widget.downcast().expect("a control is a Button"));
    }
    buttons
}

#[gtk::test]
fn gtk_ui_organize_opens_on_the_documents_view() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        assert!(viewer.organize.documents_toggle.is_active());
        assert!(!viewer.organize.pages_toggle.is_active());
        assert_eq!(
            viewer.organize.views.visible_child_name().as_deref(),
            Some(DOCUMENTS_VIEW)
        );
        assert_eq!(viewer.organize.hint.text(), DOCUMENTS_HINT);
        // The Pages view is built but empty until it is asked for: opening
        // the screen must not pay for a render per page.
        assert!(viewer.organize.cards.snapshot().is_empty());
    });
}

#[gtk::test]
fn gtk_ui_the_selector_switches_views_and_the_hint_follows() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        viewer.organize.pages_toggle.set_active(true);

        assert!(!viewer.organize.documents_toggle.is_active());
        assert_eq!(
            viewer.organize.views.visible_child_name().as_deref(),
            Some(PAGES_VIEW)
        );
        assert_eq!(viewer.organize.hint.text(), PAGES_HINT);
        assert_eq!(viewer.organize.cards.snapshot().len(), 5);

        viewer.organize.documents_toggle.set_active(true);

        assert_eq!(
            viewer.organize.views.visible_child_name().as_deref(),
            Some(DOCUMENTS_VIEW)
        );
        assert_eq!(viewer.organize.hint.text(), DOCUMENTS_HINT);
    });
}

#[gtk::test]
fn gtk_ui_each_contiguous_run_is_one_card_naming_its_source_and_range() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        let cards = cards(viewer);
        assert_eq!(cards.len(), 2);
        assert_eq!(
            card_text(&cards[0]),
            ("base.pdf".to_owned(), "2 pages · 1–2".to_owned())
        );
        assert_eq!(
            card_text(&cards[1]),
            ("report.pdf".to_owned(), "3 pages · 3–5".to_owned())
        );
    });
}

/// One more drop position than there are cards: before the first block,
/// between every pair, and after the last one.
#[gtk::test]
fn gtk_ui_the_list_offers_a_drop_position_before_between_and_after_every_card() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        let children = children(viewer);
        let gaps = children
            .iter()
            .filter(|widget| widget.has_css_class("organize-gap"))
            .count();

        assert_eq!(gaps, 3);
        assert!(
            children
                .iter()
                .step_by(2)
                .all(|widget| widget.has_css_class("organize-gap")),
            "gaps and cards must alternate, starting and ending with a gap"
        );
        // Idle, every one of them: the highlight names the destination of a
        // drag in progress and nothing else.
        assert!(!children
            .iter()
            .any(|widget| widget.has_css_class("organize-gap-active")));
    });
}

/// A split source is labeled per the core's `Block::part`, and the card that
/// says "Part 2" is the one whose pages come second.
#[gtk::test]
fn gtk_ui_a_source_split_in_two_is_labeled_by_part() {
    let mut document = Document::blank();
    document.pages = vec![
        imported_page(0, 7),
        base_page(1),
        imported_page(2, 7),
        imported_page(3, 7),
    ];

    with_documents(document, vec![source(7, "report.pdf")], |viewer| {
        let cards = cards(viewer);
        assert_eq!(cards.len(), 3);
        assert_eq!(
            card_text(&cards[0]),
            ("report.pdf — Part 1".to_owned(), "1 page · 1".to_owned())
        );
        assert_eq!(
            card_text(&cards[2]),
            ("report.pdf — Part 2".to_owned(), "2 pages · 3–4".to_owned())
        );
    });
}

/// Looking is not editing: switching views rebuilds widgets and nothing
/// else — no command, no dirty flag, no undo step.
#[gtk::test]
fn gtk_ui_switching_views_records_no_command_and_leaves_the_document_clean() {
    with_documents(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        viewer.organize.pages_toggle.set_active(true);
        viewer.organize.documents_toggle.set_active(true);
        viewer.organize.pages_toggle.set_active(true);

        let session = super::session(viewer);
        assert_eq!(session.edit_revision, 0);
        assert!(!session.unsaved_to_disk);
        assert!(!session
            .document_model
            .as_ref()
            .unwrap()
            .pending_edits
            .can_undo());
    });
}
