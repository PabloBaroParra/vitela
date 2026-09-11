//! Built-window behavior with thumbnail requests captured at the renderer boundary.

use std::cell::RefCell;

use gtk::{gdk, gdk_pixbuf, Picture};

use super::cache::ThumbnailKey;
use super::command::{apply_command, command, model, move_page};
use super::grid::drop::handle_drop;
use super::grid::{populate_grid, CARD_HEIGHT_PX, CARD_WIDTH_PX};
use super::*;
use crate::app::home::EDITOR_PAGE;
use crate::app::state::DocumentSession;
use crate::app::test_fixtures::{a_highlight, model_session};
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;
use pdf_document::{
    AnnotationId, Command, Document, ImportedDocumentId, Orientation as PageOrientation, Page,
    PageId, PageSize,
};

thread_local! {
    static THUMBNAILS: RefCell<Option<Vec<(u32, Picture)>>> = const { RefCell::new(None) };
}

/// Records that a render was asked for and stands in for it landing, or
/// returns `None` when no test has armed the capture.
///
/// The returned pixbuf is what the caller caches, exactly as it caches a real
/// render's result — which is what lets the assertions about the thumbnail
/// cache be about the same code path the shell runs. The pixel itself is
/// meaningless; what matters is that the card now *has* a paintable, because
/// `fill_missing_thumbnails` reads exactly that to tell a card it must render
/// from one it must leave alone. Without it every card in a test would look
/// like a placeholder for ever.
pub(super) fn capture_thumbnail(index: u32, picture: &Picture) -> Option<gdk_pixbuf::Pixbuf> {
    THUMBNAILS.with_borrow_mut(|requests| {
        let requests = requests.as_mut()?;
        requests.push((index, picture.clone()));
        let pixbuf = gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, true, 8, 1, 1)
            .expect("a 1x1 pixbuf");
        picture.set_pixbuf(Some(&pixbuf));
        Some(pixbuf)
    })
}

/// How many thumbnail renders have been asked for since the capture was
/// armed. The point of most assertions below is that this does *not* move.
fn render_count() -> usize {
    THUMBNAILS.with_borrow(|requests| requests.as_ref().unwrap().len())
}

fn with_organize(test: impl FnOnce(&Viewer)) {
    with_organize_of(3, test);
}

/// [`with_organize`] over a document of `pages` pages — the seam the §11
/// measurements need, since what a view switch costs is only interesting on
/// a document big enough for the answer to matter.
fn with_organize_of(pages: u32, test: impl FnOnce(&Viewer)) {
    let built = built_ui();
    let mut document = Document::blank();
    document.pages = (0..pages)
        .map(|id| {
            Page::base(
                PageId(id),
                id,
                PageSize::A4,
                PageOrientation::Portrait,
                pdf_document::Rotation::None,
            )
        })
        .collect();
    built.viewer.state.borrow_mut().session = Some(model_session(document));
    THUMBNAILS.set(Some(Vec::new()));
    show(&built.viewer);
    // `show` opens on the Documents view (checklist §9). Everything in this
    // module is about the per-page grid, so it switches there the way a user
    // would — `documents::tests` covers the block view on its own.
    built.viewer.organize.pages_toggle.set_active(true);
    built.window.present();
    // Teardown runs on the unwind path too. `#[gtk::test]` bodies all execute
    // on one shared main thread (`gtk::test_synced`), so `THUMBNAILS` is not
    // private to this test: a panic that skipped the reset would leave the
    // capture armed and swallow every later test's real thumbnail render,
    // turning one failure into a cascade of unrelated ones. Clearing the
    // session before closing also keeps the window clean, so the close never
    // raises the unsaved-changes prompt.
    let _teardown = Teardown(&built);
    test(&built.viewer);
}

struct Teardown<'a>(&'a BuiltUi);

impl Drop for Teardown<'_> {
    fn drop(&mut self) {
        THUMBNAILS.set(None);
        self.0.viewer.state.borrow_mut().session = None;
        self.0.window.close();
    }
}

fn history_button(viewer: &Viewer, label: &str) -> Button {
    let header = viewer.organize.save_button.parent().unwrap();
    let mut child = header.first_child();
    while let Some(widget) = child {
        match widget.downcast_ref::<Button>() {
            Some(button) if button.label().as_deref() == Some(label) => return button.clone(),
            _ => {}
        }
        child = widget.next_sibling();
    }
    panic!("missing {label} button beside Save");
}

fn delete_button(viewer: &Viewer, index: usize) -> Button {
    let footer = viewer.organize.cards.snapshot()[index]
        .root
        .last_child()
        .unwrap();
    footer.last_child().unwrap().downcast().unwrap()
}

fn session(viewer: &Viewer) -> std::cell::RefMut<'_, DocumentSession> {
    std::cell::RefMut::map(viewer.state.borrow_mut(), |state| {
        state.session.as_mut().unwrap()
    })
}

/// The model's page order, as plain numbers — what a block move or delete
/// is judged by.
fn page_ids_of(viewer: &Viewer) -> Vec<u32> {
    session(viewer)
        .document_model
        .as_ref()
        .unwrap()
        .pages
        .iter()
        .map(|page| page.id.0)
        .collect()
}

fn assert_grid(viewer: &Viewer, expected: &[u32]) {
    let session = session(viewer);
    let page_ids: Vec<PageId> = session
        .document_model
        .as_ref()
        .unwrap()
        .pages
        .iter()
        .map(|page| page.id)
        .collect();
    let ids: Vec<_> = page_ids.iter().map(|id| id.0).collect();
    assert_eq!(ids, expected);
    let grid = &viewer.organize.grid;
    let cards = viewer.organize.cards.snapshot();
    assert_eq!(cards.len(), expected.len());
    for (index, card) in cards.iter().enumerate() {
        assert_eq!(card.number.text(), (index + 1).to_string());
        // The card's id is the drag payload, so a card holding the wrong
        // page's id would move the wrong page on the next drop — with a grid
        // that still looked entirely correct.
        assert_eq!(card.id, page_ids[index]);
        let child = grid.child_at_index(index as i32).unwrap().child().unwrap();
        assert_eq!(&child, card.root.upcast_ref::<gtk::Widget>());
        let picture = card.picture.clone();
        // A card's thumbnail is asked for by the page's position in the *open
        // handle*, never by its id — the two stop being the same number the
        // moment a page op is recorded, and an imported page's id was never a
        // position at all. A page the handle does not hold (an insert whose
        // preview refresh has not landed) is not requested and keeps its
        // placeholder.
        let expected_backend = session.backend_index(page_ids[index]);
        let requested = THUMBNAILS.with_borrow(|requests| {
            requests
                .as_ref()
                .unwrap()
                .iter()
                .find(|(_, requested)| *requested == picture)
                .map(|(backend_index, _)| *backend_index as usize)
        });
        // The other correct answer is a card that asked for nothing because
        // the thumbnail cache already held its page (checklist §11). That is
        // not a weaker assertion than the one below: the cache is keyed on
        // the very `PageId` this loop has just checked the card carries, and
        // the entry under it was put there by a render that went through the
        // backend-position check itself.
        if requested.is_none() && cached_thumbnail(viewer, &card.picture, page_ids[index]) {
            assert!(
                card.picture.paintable().is_some(),
                "a cached card must be painted, not left a placeholder"
            );
            continue;
        }
        assert_eq!(
            requested, expected_backend,
            "thumbnail must be requested by backend page position"
        );
    }
    assert!(grid.child_at_index(expected.len() as i32).is_none());
}

/// Whether the cache holds `page`'s thumbnail at the size and scale
/// `picture` would have asked for.
fn cached_thumbnail(viewer: &Viewer, picture: &Picture, page: PageId) -> bool {
    viewer
        .organize
        .thumbnails
        .get(&ThumbnailKey {
            page,
            width: CARD_WIDTH_PX,
            height: CARD_HEIGHT_PX,
            scale: picture.scale_factor().max(1),
        })
        .is_some()
}

/// Drops the page carrying `id` on insertion slot `slot` — `k` meaning
/// "before the card at `k`", `cards.len()` meaning "past the last card".
///
/// The pointer is placed in the leading half of the card at `slot`, which is
/// how the real gesture picks one: the grid has no gap widgets to aim at (see
/// `organize::grid`'s header). The last slot is aimed at the empty space
/// *under* the final row — a point no card occupies, which is exactly the
/// drop the grid used to throw away.
fn drop_at_slot(viewer: &Viewer, id: u32, slot: usize) -> bool {
    let grid = &viewer.organize.grid;
    // Picking requires mapped widgets, not just an allocation.
    assert!(grid.is_mapped());
    grid.allocate(800, 600, -1, None);
    let count = viewer.organize.cards.len();
    let (x, y) = if slot < count {
        let bounds = grid.child_at_index(slot as i32).unwrap().allocation();
        (bounds.x() + 1, bounds.y() + bounds.height() / 2)
    } else {
        let bounds = grid.child_at_index(count as i32 - 1).unwrap().allocation();
        let below = bounds.y() + bounds.height() + 10;
        assert!(grid.child_at_pos(10, below).is_none(), "no card sits there");
        (10, below)
    };
    handle_drop(viewer, &id.to_value(), f64::from(x), f64::from(y))
}

/// Drags the card at `from` and drops it so its page ends up at index `to`.
///
/// The payload is the card's own `PageId`, the way `build_card`'s drag source
/// prepares it — never `from`, which is only used here to pick a card to
/// grab.
fn drop_on(viewer: &Viewer, from: usize, to: usize) -> bool {
    let id = viewer.organize.cards.snapshot()[from].id.0;
    // Landing *at* index `to` means aiming past the card that currently sits
    // there when the page is travelling forwards, since the page vacates a
    // slot on its way — the `slot`/`to` distinction `grid::destination` owns.
    let slot = if to > from { to + 1 } else { to };
    drop_at_slot(viewer, id, slot)
}

/// Opens the Organize screen on the Documents view — where [`show`] leaves
/// it — with `document` as the model and `sources` registered as the PDFs
/// its imported pages came from.
///
/// The per-page twin, [`with_organize`], switches to the Pages view on the
/// way in; this one stays put, so `documents::tests` exercises the blocks.
fn with_documents(
    document: Document,
    sources: Vec<crate::app::state::ImportedSource>,
    test: impl FnOnce(&Viewer),
) {
    let built = built_ui();
    {
        let mut session = model_session(document);
        session.base_name = "base.pdf".to_owned();
        session.imported_sources = sources;
        built.viewer.state.borrow_mut().session = Some(session);
    }
    THUMBNAILS.set(Some(Vec::new()));
    show(&built.viewer);
    built.window.present();
    let _teardown = Teardown(&built);
    test(&built.viewer);
}

mod add_pdfs;
mod blocks;
mod documents;
mod history;
mod motion;
mod pages;
mod refusals;
mod resolution;
mod thumbnails;
