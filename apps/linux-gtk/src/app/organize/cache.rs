//! The Organize screen's thumbnail cache: rendered pixels kept by page,
//! size and scale factor, so leaving a view and coming back does not pay
//! pdfium again (checklist §11, `docs/batch-pdf-assembly.md`).
//!
//! ## Why the key is a `PageId` and not a pdfium page index
//!
//! The index a thumbnail is *rendered* by is the page's position in the open
//! pdfium handle, and that number is not stable: `Command::MovePage`
//! renumbers it, an import shifts every page after the insertion, and
//! `document::refresh_preview` swaps the handle outright. A cache keyed on it
//! would hand a moved page the picture of whatever page inherited its old
//! slot — the exact confusion `grid::PageRow::backend` exists to avoid.
//! `PageId` is the page's identity for the life of the session, which is
//! precisely the life of this cache.
//!
//! Size and scale factor are in the key because the two views ask for the
//! same pages at different sizes (`grid::card::CARD_WIDTH_PX` against
//! `documents::card::COVER_WIDTH_PX`), and a monitor change can move the
//! scale factor under both.
//!
//! ## Why there is a byte budget
//!
//! A page thumbnail is a `Pixbuf`, not a handle: an A4 page at the Pages
//! view's size renders to about 380x538 px — roughly 0.8 MB — and four times
//! that on a HiDPI monitor. Keeping every page of a 500-page assembly would
//! be hundreds of megabytes of pixels for cards nobody is looking at, so the
//! cache holds what fits in [`BUDGET_BYTES`] and evicts least-recently-used
//! entries beyond it. On a document small enough to fit, a view switch costs
//! zero renders; on one too big it degrades to rendering what fell out,
//! which is still strictly less than the full re-render it replaces.
//!
//! ## Why a generation counter rather than removing keys
//!
//! A render is asked for on the main thread and lands several frames later
//! (`grid::thumbnail::spawn_thumbnail`). Between the two, a content edit can
//! repaint the very page being rendered, which makes both the in-flight
//! result and every cached entry wrong at once. Bumping a counter answers
//! both in one step: the cache empties, and a render that started before the
//! bump can see that it did and drop its own result instead of caching stale
//! pixels under a fresh key.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gdk_pixbuf::Pixbuf;
use pdf_document::PageId;

/// How many bytes of rendered thumbnails to keep. 96 MiB holds roughly 110
/// page-sized thumbnails at a scale factor of 1, or about 28 on a HiDPI
/// monitor — comfortably more than the two views show at once on any
/// reasonable window, which is what makes a view switch free.
const BUDGET_BYTES: usize = 96 * 1024 * 1024;

/// Which rendered thumbnail this is: the page, the logical size the card
/// asked for, and the scale factor it will be drawn at.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct ThumbnailKey {
    pub(crate) page: PageId,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) scale: i32,
}

struct Entry {
    pixbuf: Pixbuf,
    bytes: usize,
    /// The value of [`Store::clock`] when this entry was last handed out —
    /// the "recently used" half of the eviction order.
    used: u64,
}

struct Store {
    entries: RefCell<HashMap<ThumbnailKey, Entry>>,
    bytes: Cell<usize>,
    clock: Cell<u64>,
    generation: Cell<u64>,
}

/// The cache itself, shared by the two views through `OrganizePanel`.
///
/// Like `state::Cards`, it never hands out a `RefMut`: every method takes its
/// borrow, finishes with it and drops it before returning. Callers here set
/// `Picture` contents while they work, and GTK is free to re-enter this
/// module from a resize or a redraw while they do.
#[derive(Clone)]
pub(crate) struct Thumbnails(Rc<Store>);

impl Thumbnails {
    pub(crate) fn new() -> Self {
        Self(Rc::new(Store {
            entries: RefCell::new(HashMap::new()),
            bytes: Cell::new(0),
            clock: Cell::new(0),
            generation: Cell::new(0),
        }))
    }

    /// The generation a render should carry, to be checked against
    /// [`Self::is_current`] when it lands.
    pub(crate) fn generation(&self) -> u64 {
        self.0.generation.get()
    }

    /// Whether a render that started at `generation` may still be shown and
    /// cached.
    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.0.generation.get() == generation
    }

    /// The cached pixels for `key`, if any, marked as just used.
    pub(crate) fn get(&self, key: &ThumbnailKey) -> Option<Pixbuf> {
        let tick = self.tick();
        let mut entries = self.0.entries.borrow_mut();
        let entry = entries.get_mut(key)?;
        entry.used = tick;
        Some(entry.pixbuf.clone())
    }

    /// Stores `pixbuf` under `key`, evicting least-recently-used entries
    /// until the total is back inside [`BUDGET_BYTES`].
    ///
    /// An entry larger than the whole budget on its own is stored anyway and
    /// then evicted by the loop below, which leaves the cache empty rather
    /// than looping forever — a thumbnail that big means a card the size of a
    /// wall, and the render still reaches the screen either way.
    pub(crate) fn insert(&self, key: ThumbnailKey, pixbuf: &Pixbuf) {
        let tick = self.tick();
        let bytes = pixbuf.rowstride().max(0) as usize * pixbuf.height().max(0) as usize;
        {
            let mut entries = self.0.entries.borrow_mut();
            let previous = entries.insert(
                key,
                Entry {
                    pixbuf: pixbuf.clone(),
                    bytes,
                    used: tick,
                },
            );
            let freed = previous.map_or(0, |entry| entry.bytes);
            self.0
                .bytes
                .set(self.0.bytes.get().saturating_sub(freed) + bytes);
        }
        self.evict_to_budget();
    }

    /// Throws away every cached thumbnail and moves the generation on, so
    /// renders already in flight drop their results instead of refilling the
    /// cache with the pixels this call just rejected.
    pub(crate) fn invalidate(&self) {
        self.0.generation.set(self.0.generation.get() + 1);
        self.clear();
    }

    /// Empties the cache without moving the generation — for a new document,
    /// whose `PageId`s start over at 0 and would otherwise collide with the
    /// previous document's keys.
    pub(crate) fn clear(&self) {
        self.0.entries.borrow_mut().clear();
        self.0.bytes.set(0);
    }

    /// How many thumbnails are held. Test-facing: the assertions about a
    /// view switch are about renders not happening, and this is how they
    /// tell an empty cache from a full one.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.entries.borrow().len()
    }

    fn tick(&self) -> u64 {
        let tick = self.0.clock.get() + 1;
        self.0.clock.set(tick);
        tick
    }

    fn evict_to_budget(&self) {
        while self.0.bytes.get() > BUDGET_BYTES {
            let mut entries = self.0.entries.borrow_mut();
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| *key)
            else {
                // Nothing left to evict: the running total disagrees with
                // what is actually held, so believe the entries.
                drop(entries);
                self.0.bytes.set(0);
                return;
            };
            let freed = entries.remove(&oldest).map_or(0, |entry| entry.bytes);
            drop(entries);
            self.0.bytes.set(self.0.bytes.get().saturating_sub(freed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_pixbuf(width: i32, height: i32) -> Pixbuf {
        Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, width, height)
            .expect("a test pixbuf")
    }

    fn a_key(page: u32) -> ThumbnailKey {
        ThumbnailKey {
            page: PageId(page),
            width: 140,
            height: 180,
            scale: 1,
        }
    }

    #[test]
    fn the_same_page_at_two_sizes_is_two_entries() {
        let cache = Thumbnails::new();
        let pages = a_key(0);
        let cover = ThumbnailKey {
            width: 96,
            height: 124,
            ..pages
        };

        cache.insert(pages, &a_pixbuf(4, 4));
        cache.insert(cover, &a_pixbuf(4, 4));

        assert_eq!(cache.len(), 2);
        assert!(cache.get(&pages).is_some());
        assert!(cache.get(&cover).is_some());
    }

    #[test]
    fn invalidating_empties_the_cache_and_retires_renders_in_flight() {
        let cache = Thumbnails::new();
        let in_flight = cache.generation();
        cache.insert(a_key(0), &a_pixbuf(4, 4));

        cache.invalidate();

        assert_eq!(cache.len(), 0);
        assert!(!cache.is_current(in_flight));
        assert!(cache.is_current(cache.generation()));
    }

    #[test]
    fn a_new_document_clears_the_cache_but_keeps_renders_in_flight_valid() {
        let cache = Thumbnails::new();
        let in_flight = cache.generation();
        cache.insert(a_key(0), &a_pixbuf(4, 4));

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_current(in_flight));
    }

    #[test]
    fn going_over_budget_evicts_the_least_recently_used_page() {
        let cache = Thumbnails::new();
        // Three pixbufs of half the budget each: the third cannot land until
        // one of the first two is gone.
        let half = (BUDGET_BYTES / 2 / 4 / 1024) as i32;
        for page in 0..2 {
            cache.insert(a_key(page), &a_pixbuf(1024, half));
        }
        // Touching page 0 makes page 1 the oldest, even though page 0 went in
        // first.
        assert!(cache.get(&a_key(0)).is_some());

        cache.insert(a_key(2), &a_pixbuf(1024, half));

        assert!(cache.get(&a_key(1)).is_none(), "the oldest must go first");
        assert!(cache.get(&a_key(0)).is_some());
        assert!(cache.get(&a_key(2)).is_some());
    }

    #[test]
    fn replacing_a_key_does_not_leak_its_old_bytes() {
        let cache = Thumbnails::new();
        let full = (BUDGET_BYTES / 4 / 1024) as i32;

        for _ in 0..4 {
            cache.insert(a_key(0), &a_pixbuf(1024, full));
        }

        // Re-inserting the same key four times is one entry's worth of
        // pixels, not four: a running total that counted the replacements
        // would have evicted the only entry there is.
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&a_key(0)).is_some());
    }
}
