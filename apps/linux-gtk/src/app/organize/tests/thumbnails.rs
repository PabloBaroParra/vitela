//! What a view switch costs, and what makes it cost again (checklist §11).
//!
//! Every assertion here is a render count or a card's identity rather than a
//! stopwatch. Showing either view costs one pdfium render per card and one
//! card's worth of widgets per page, and `super::THUMBNAILS` counts the first
//! at the boundary where they are asked for while `cards` *is* the second —
//! which makes "switching views builds nothing again" a fact a test can
//! state, rather than a timing that depends on the machine it ran on. The
//! timings themselves live in `super::measure`.

use super::*;

/// The measurement document: twelve pages is more than the grid shows in one
/// row and more than the stagger animates, so a full re-render would be
/// obvious in the count.
const PAGES: u32 = 12;

fn switch(viewer: &Viewer, to_documents: bool) {
    if to_documents {
        viewer.organize.documents_toggle.set_active(true);
    } else {
        viewer.organize.pages_toggle.set_active(true);
    }
}

/// The headline of §11: leaving a view and coming back is free. Before the
/// cache, each switch re-rendered every card of the view being entered — on
/// a fifty-page document, fifty pdfium renders to look at pages that had not
/// changed.
#[gtk::test]
fn gtk_ui_switching_between_the_two_views_renders_nothing_again() {
    with_organize_of(PAGES, |viewer| {
        let rendered_on_open = render_count();
        assert_eq!(
            rendered_on_open,
            PAGES as usize + 1,
            "opening renders the Documents view's one block cover, then every page"
        );

        for _ in 0..3 {
            switch(viewer, true);
            switch(viewer, false);
        }

        assert_eq!(
            render_count(),
            rendered_on_open,
            "every card of both views was already in the cache"
        );
        assert_grid(viewer, &(0..PAGES).collect::<Vec<_>>());
    });
}

/// The cards are the other half of what a switch costs, and the cache says
/// nothing about them: before `grid::fill_grid`, every arrival tore the grid
/// down and built one `Box`, one `Picture`, three labels and two buttons per
/// page again. Measured at about 2.2 ms a card, which is most of a second on
/// the four-hundred-page assembly `super::measure` times.
#[gtk::test]
fn gtk_ui_returning_to_an_unchanged_grid_keeps_the_cards_it_built() {
    with_organize_of(PAGES, |viewer| {
        let built_on_open = viewer.organize.cards.snapshot();

        for _ in 0..3 {
            switch(viewer, true);
            switch(viewer, false);
        }

        assert_eq!(
            viewer.organize.cards.snapshot(),
            built_on_open,
            "the same card widgets, not rebuilt copies of them"
        );
        assert_grid(viewer, &(0..PAGES).collect::<Vec<_>>());
    });
}

/// The reuse is only allowed while the cards still say what the model says.
/// `move_page` on its own is that disagreement: a real drag pairs it with the
/// `reorder_cards` that moves the card to match, and without one the grid is
/// holding an order the document no longer has.
#[gtk::test]
fn gtk_ui_a_model_that_moved_behind_the_grids_back_rebuilds_it() {
    with_organize_of(PAGES, |viewer| {
        let built_on_open = viewer.organize.cards.snapshot();

        assert!(move_page(viewer, 0, 2));

        switch(viewer, true);
        switch(viewer, false);

        assert_ne!(
            viewer.organize.cards.snapshot(),
            built_on_open,
            "a grid that disagrees with the model must be rebuilt, not reused"
        );
        let mut expected: Vec<u32> = (0..PAGES).collect();
        expected.remove(0);
        expected.insert(2, 0);
        assert_grid(viewer, &expected);
    });
}

/// The trap under the identity check: `PageId`s start over at 0 in the next
/// document, so two documents of the same length present *the same ids in the
/// same order*. Nothing about the page list can tell them apart, and a grid
/// reused across that boundary would show the previous document's pages.
#[gtk::test]
fn gtk_ui_another_document_does_not_inherit_the_previous_ones_cards() {
    with_organize_of(PAGES, |viewer| {
        let built_for_the_previous_document = viewer.organize.cards.snapshot();

        document_changed(viewer);
        switch(viewer, true);
        switch(viewer, false);

        assert_ne!(
            viewer.organize.cards.snapshot(),
            built_for_the_previous_document,
            "a new document must render its own cards, id collision or not"
        );
    });
}

/// A content edit leaves every card holding a picture of a page that no
/// longer looks like that. The page *order* is untouched, so the identity
/// check alone would wave the grid through — `thumbnails_stale` is the second
/// half of the condition, and this is the case it exists for.
#[gtk::test]
fn gtk_ui_a_grid_whose_pages_were_repainted_is_rebuilt_not_reused() {
    with_organize_of(PAGES, |viewer| {
        let built_on_open = viewer.organize.cards.snapshot();

        invalidate_thumbnails(viewer);
        switch(viewer, true);
        switch(viewer, false);

        assert_ne!(
            viewer.organize.cards.snapshot(),
            built_on_open,
            "repainted pages must be rendered onto fresh cards"
        );
    });
}

/// The two views ask for the same pages at different sizes, so they cannot
/// share entries — and must not, or a block cover would be drawn from a
/// thumbnail rendered for a card half again as wide.
#[gtk::test]
fn gtk_ui_each_view_caches_its_own_size() {
    with_organize_of(PAGES, |viewer| {
        // One entry per page at the Pages size, plus one for the single
        // block's cover at the Documents size.
        assert_eq!(viewer.organize.thumbnails.len(), PAGES as usize + 1);
    });
}

/// A content edit repaints a page without changing which page it is, so
/// every cached entry for it is a picture of something that no longer
/// exists under a key that still matches.
#[gtk::test]
fn gtk_ui_a_content_edit_empties_the_cache_and_the_next_switch_renders_again() {
    with_organize_of(PAGES, |viewer| {
        let rendered_on_open = render_count();

        invalidate_thumbnails(viewer);
        assert_eq!(viewer.organize.thumbnails.len(), 0);

        switch(viewer, true);
        assert_eq!(
            render_count(),
            rendered_on_open + 1,
            "the block cover is rendered from the repainted page"
        );
        switch(viewer, false);
        assert_eq!(
            render_count(),
            rendered_on_open + 1 + PAGES as usize,
            "and so is every page card"
        );
    });
}

/// `PageId`s are unique within one model and start over at 0 in the next, so
/// an entry that outlived its document would hand the new document's first
/// page the old one's picture.
#[gtk::test]
fn gtk_ui_opening_another_document_does_not_reuse_the_previous_ones_thumbnails() {
    with_organize_of(PAGES, |viewer| {
        assert_ne!(viewer.organize.thumbnails.len(), 0);

        document_changed(viewer);

        assert_eq!(viewer.organize.thumbnails.len(), 0);
    });
}

/// A render that started before a content edit must not fill the cache back
/// up with the pixels that edit just rejected. The generation is what tells
/// the two apart, since the key it would be filed under is still perfectly
/// valid.
#[gtk::test]
fn gtk_ui_a_render_in_flight_across_an_invalidation_is_dropped() {
    with_organize_of(PAGES, |viewer| {
        let in_flight = viewer.organize.thumbnails.generation();
        let handle = viewer.state.borrow().session.as_ref().unwrap().document;

        invalidate_thumbnails(viewer);

        assert!(!super::grid::thumbnail::render_is_current(
            viewer, in_flight, handle
        ));
    });
}

/// The other half of the gate, which fails on its own: closing the document
/// retires a render already in flight against it without ever touching the
/// cache generation.
///
/// The session is cleared rather than replaced because `model_session` zeroes
/// its handle (see `test_fixtures`), so two fixture sessions are
/// indistinguishable by handle — `None` is the only mismatch a fixture can
/// state.
#[gtk::test]
fn gtk_ui_a_render_that_outlives_its_session_is_dropped() {
    with_organize_of(PAGES, |viewer| {
        let generation = viewer.organize.thumbnails.generation();
        let previous_handle = viewer.state.borrow().session.as_ref().unwrap().document;

        viewer.state.borrow_mut().session = None;

        assert!(viewer.organize.thumbnails.is_current(generation));

        assert!(!super::grid::thumbnail::render_is_current(
            viewer,
            generation,
            previous_handle
        ));
    });
}

/// Adding pages to an assembled document is the other operation §11 asks to
/// be measured, and the answer is the same shape: the pages already in the
/// grid are repainted from the cache, and the imported ones are not rendered
/// at all until the preview refresh puts them in the open handle.
#[gtk::test]
fn gtk_ui_importing_pages_does_not_re_render_the_grid_it_lands_in() {
    with_organize_of(PAGES, |viewer| {
        let rendered_on_open = render_count();
        let token = {
            let state = viewer.state.borrow();
            let session = state.session.as_ref().unwrap();
            crate::app::state::SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            }
        };
        let sources = vec![crate::app::state::ImportedSource {
            id: ImportedDocumentId(7),
            document: pdf_manip::LopdfDocument::from_lopdf(lopdf::Document::new()),
            name: "source-7.pdf".to_owned(),
        }];
        let pages = vec![Page::imported(
            PageId(PAGES),
            ImportedDocumentId(7),
            0,
            PageSize::A4,
            PageOrientation::Portrait,
            pdf_document::Rotation::None,
        )];

        super::import::apply_prepared(viewer, token, sources, pages);

        assert_eq!(
            render_count(),
            rendered_on_open,
            "an import must not re-render the pages that were already there"
        );
        assert_eq!(viewer.organize.cards.len(), PAGES as usize + 1);
    });
}
