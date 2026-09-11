//! What a view switch costs, and what makes it cost again (checklist §11).
//!
//! Every assertion here is a render count rather than a stopwatch. The
//! expensive part of showing either view is one pdfium render per card, and
//! `super::THUMBNAILS` counts exactly those at the boundary where they are
//! asked for — which makes "switching views renders nothing again" a fact a
//! test can state, rather than a timing that depends on the machine it ran
//! on.

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

        invalidate_thumbnails(viewer);

        assert!(!viewer.organize.thumbnails.is_current(in_flight));
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
