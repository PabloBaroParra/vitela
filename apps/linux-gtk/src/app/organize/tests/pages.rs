//! The Organize screen's "Pages" view as checklist §10 asks for it: pages
//! from different PDFs in one grid, a drag that carries a page identity
//! rather than a captured position, drops that land in the gaps and past the
//! last card, and the provenance line under each thumbnail.
//!
//! The parent module's tests cover the grid's older ground — history, the
//! thumbnails a move must not re-render, the cards a move must carry intact.
//! These cover what the two views have to agree about.

use super::super::documents::PAGES_VIEW;
use super::documents::{base_page, imported_page, source, two_blocks};
use super::*;

/// Every card's provenance line, or `None` where the card shows none.
fn source_lines(viewer: &Viewer) -> Vec<Option<String>> {
    viewer
        .organize
        .cards
        .snapshot()
        .iter()
        .map(|card| card.source.get_visible().then(|| card.source.text().into()))
        .collect()
}

/// Opens the screen on a document with `sources` imported into it and
/// switches to the Pages view, the way a user reaches it — `show` always
/// opens on Documents (checklist §9).
fn with_pages(
    document: Document,
    sources: Vec<crate::app::state::ImportedSource>,
    test: impl FnOnce(&Viewer),
) {
    with_documents(document, sources, |viewer| {
        viewer.organize.pages_toggle.set_active(true);
        assert_eq!(
            viewer.organize.views.visible_child_name().as_deref(),
            Some(PAGES_VIEW)
        );
        test(viewer);
    });
}

/// The slot past the final card is a real destination, not a miss. The grid
/// used to resolve a drop by asking which *card* the pointer was over, so the
/// empty space under a part-filled last row answered "none" and the drop was
/// silently dropped — leaving no way to drag a page to the end of the
/// document at all.
#[gtk::test]
fn gtk_ui_a_page_dropped_past_the_last_card_lands_at_the_end() {
    with_organize(|viewer| {
        assert!(drop_at_slot(viewer, 0, 3));
        assert_grid(viewer, &[1, 2, 0]);
    });
}

/// The drag carries the page's id, and the drop resolves it against the page
/// order as it is at that moment. A payload that was a position captured
/// when the grid was built would still name index 0 here — and move the wrong
/// page.
#[gtk::test]
fn gtk_ui_a_page_dragged_again_is_found_where_it_now_sits() {
    with_organize(|viewer| {
        assert!(drop_at_slot(viewer, 0, 3));
        assert_grid(viewer, &[1, 2, 0]);

        // Page 0 is now the last card. Dropping *it* on the front slot has to
        // move it from index 2, not from the index it held when built.
        assert!(drop_at_slot(viewer, 0, 0));
        assert_grid(viewer, &[0, 1, 2]);
    });
}

#[gtk::test]
fn gtk_ui_a_drop_naming_a_page_the_document_does_not_have_is_refused() {
    with_organize(|viewer| {
        assert!(!drop_at_slot(viewer, 99, 0));
        assert_grid(viewer, &[0, 1, 2]);
        assert!(!viewer.undo_action.is_enabled());
    });
}

/// Provenance is worth showing when there is something to tell apart. On a
/// document that was never merged, naming the one source on every card is
/// noise — the checklist asks for provenance "without overloading the card".
#[gtk::test]
fn gtk_ui_a_document_with_one_source_names_it_on_no_card() {
    with_organize(|viewer| {
        assert_eq!(source_lines(viewer), vec![None, None, None]);
    });
}

/// Two base pages and three imported ones share one grid: the cards say which
/// PDF each page came from, and a page can be dragged out of its run and in
/// among the others' — which is the whole point of the view.
#[gtk::test]
fn gtk_ui_pages_of_different_pdfs_name_their_source_and_interleave() {
    with_pages(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        assert_grid(viewer, &[0, 1, 2, 3, 4]);
        assert_eq!(
            source_lines(viewer),
            vec![
                Some("base.pdf".to_owned()),
                Some("base.pdf".to_owned()),
                Some("report.pdf".to_owned()),
                Some("report.pdf".to_owned()),
                Some("report.pdf".to_owned()),
            ]
        );

        // The first imported page, dropped between the two base ones.
        assert!(drop_at_slot(viewer, 2, 1));
        assert_grid(viewer, &[0, 2, 1, 3, 4]);
        assert_eq!(
            source_lines(viewer),
            vec![
                Some("base.pdf".to_owned()),
                Some("report.pdf".to_owned()),
                Some("base.pdf".to_owned()),
                Some("report.pdf".to_owned()),
                Some("report.pdf".to_owned()),
            ],
            "a provenance line travels with the card it belongs to"
        );
    });
}

/// A page moved in the Pages view is a page moved in the document, so the
/// Documents view has to come back showing the runs that order now makes —
/// here, one base page, the interleaved import, then the rest.
#[gtk::test]
fn gtk_ui_returning_to_documents_shows_the_order_the_pages_view_left() {
    with_pages(two_blocks(), vec![source(7, "report.pdf")], |viewer| {
        assert!(drop_at_slot(viewer, 2, 1));
        assert_grid(viewer, &[0, 2, 1, 3, 4]);

        viewer.organize.documents_toggle.set_active(true);
        let titles: Vec<(String, String)> = super::documents::cards(viewer)
            .iter()
            .map(super::documents::card_text)
            .collect();
        assert_eq!(
            titles,
            vec![
                ("base.pdf — Part 1".to_owned(), "1 page · 1".to_owned()),
                ("report.pdf — Part 1".to_owned(), "1 page · 2".to_owned()),
                ("base.pdf — Part 2".to_owned(), "1 page · 3".to_owned()),
                ("report.pdf — Part 2".to_owned(), "2 pages · 4–5".to_owned()),
            ],
            "the move split both documents into labelled parts"
        );
        assert_eq!(page_ids_of(viewer), vec![0, 2, 1, 3, 4]);
    });
}

/// Deleting the last page of the only import leaves a single-source document
/// again, and the cards that survive must stop naming it.
#[gtk::test]
fn gtk_ui_deleting_the_last_imported_page_takes_the_provenance_lines_with_it() {
    let mut document = Document::blank();
    document.pages = vec![base_page(0), base_page(1), imported_page(2, 7)];

    with_pages(document, vec![source(7, "report.pdf")], |viewer| {
        assert_eq!(
            source_lines(viewer),
            vec![
                Some("base.pdf".to_owned()),
                Some("base.pdf".to_owned()),
                Some("report.pdf".to_owned()),
            ]
        );

        delete_button(viewer, 2).emit_clicked();
        assert_grid(viewer, &[0, 1]);
        assert_eq!(source_lines(viewer), vec![None, None]);
    });
}
