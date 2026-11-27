//! Opt-in timing traces for the latencies a user feels — see
//! `docs/perf-baseline.md` for the span names and how to read them.
//!
//! Off unless `VITELA_PERF` says otherwise ([`enablement`]). Off, every
//! entry point here costs one check of a cached flag: [`now`] reads no clock
//! and hands back `None`, and everything else returns on that `None`. On,
//! each measurement is one line on stderr ([`line`]).
//!
//! Three shapes of measurement:
//!
//! - **A stretch of one function** — [`now`] at the top, [`finish`] at the
//!   end. An early return in between reports nothing, which is what a call
//!   that did no work should report.
//! - **A latency that ends in another callback** — [`mark`] the start under
//!   a [`Mark`], and the callback that ends it reports through
//!   [`page_landed`]/[`tiles_landed`]. Marks live on the main thread only.
//! - **A batch of futures** — the Organize thumbnails, timed to the last one
//!   through [`thumbnails_begin`]/[`thumbnails_join`]/[`thumbnails_sealed`].
//!
//! The logic under those — parsing, formatting, mark and batch bookkeeping —
//! is in the submodules, free of GTK and of this module's globals, and
//! tested there against explicit instants and an in-memory sink.

mod batch;
mod enablement;
mod line;
mod marks;

use std::cell::RefCell;
use std::io;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use pdf_render::DocumentHandle;

pub(crate) use line::Field;

/// The environment variable that switches the traces on.
const ENV_VAR: &str = "VITELA_PERF";

/// More marks than can be pending at once in practice (a handful of kinds
/// for the document on screen); the bound only exists for the ones nothing
/// ever takes.
const MARK_CAPACITY: usize = 32;

static ENABLED: OnceLock<bool> = OnceLock::new();

thread_local! {
    static MARKS: RefCell<marks::Marks<Mark>> =
        const { RefCell::new(marks::Marks::new(MARK_CAPACITY)) };
    static THUMBNAILS: RefCell<batch::Batch> = const { RefCell::new(batch::Batch::new()) };
}

/// The start of a latency that ends when a page of `DocumentHandle` is next
/// painted. Keyed by the pdfium handle because that is what a finished
/// render is matched against before it may touch the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    /// An open was started; ends at the document's first painted page.
    Open(DocumentHandle),
    /// A preview refresh was requested; ends at the first page re-rendered
    /// from the reopened handle.
    Preview(DocumentHandle),
    /// A tab switch began; ends at the first page painted in the tab.
    TabSwitch(DocumentHandle),
    /// The layout was refitted (a zoom step, a resize); ends at the first
    /// page rendered at the new size.
    Zoom(DocumentHandle),
    /// As [`Mark::Zoom`], ending at the first batch of deep-zoom tiles.
    ZoomTiles(DocumentHandle),
}

/// Whether the traces are on. Read from the environment once per process.
pub(crate) fn enabled() -> bool {
    *ENABLED.get_or_init(|| enablement::parse(std::env::var(ENV_VAR).ok().as_deref()))
}

/// The clock, when the traces are on. `None` otherwise — without reading it.
pub(crate) fn now() -> Option<Instant> {
    enabled().then(Instant::now)
}

/// Reports the time since `started` as span `name`.
pub(crate) fn finish(name: &str, started: Option<Instant>, fields: &[Field]) {
    finish_over(name, started, Duration::ZERO, fields);
}

/// [`finish`], but silent under `floor` — for a span that runs on every
/// scroll tick and is only worth a line when it was slow.
pub(crate) fn finish_over(name: &str, started: Option<Instant>, floor: Duration, fields: &[Field]) {
    let Some(started) = started else {
        return;
    };
    if let Some(line) = line::measurement(name, started.elapsed(), floor, fields) {
        emit(&line);
    }
}

/// Starts the latency `key` names at `started`. Main thread only.
pub(crate) fn mark(key: Mark, started: Option<Instant>) {
    let Some(started) = started else {
        return;
    };
    MARKS.with_borrow_mut(|marks| marks.set(key, started));
}

/// Drops the mark under `key` unreported — for a start whose end will not
/// come the way the mark expects (a document opened into a background tab
/// paints nothing until it is switched to).
pub(crate) fn forget(key: Mark) {
    if enabled() {
        MARKS.with_borrow_mut(|marks| marks.take(&key));
    }
}

/// A page of `document` has just been painted: ends every latency that was
/// waiting for one.
pub(crate) fn page_landed(document: DocumentHandle, page_index: usize) {
    if !enabled() {
        return;
    }
    for (key, name) in [
        (Mark::Open(document), "open.first_page"),
        (Mark::Preview(document), "preview.first_page"),
        (Mark::TabSwitch(document), "tab.first_page"),
        (Mark::Zoom(document), "zoom.first_page"),
    ] {
        land(key, name, page_index);
    }
}

/// A batch of deep-zoom tiles of `document` has just been placed.
pub(crate) fn tiles_landed(document: DocumentHandle, page_index: usize) {
    if enabled() {
        land(Mark::ZoomTiles(document), "zoom.first_tiles", page_index);
    }
}

fn land(key: Mark, name: &str, page_index: usize) {
    let started = MARKS.with_borrow_mut(|marks| marks.take(&key));
    finish(name, started, &[("page", page_index.into())]);
}

/// Opens the Organize thumbnail batch, timed from `started`.
pub(crate) fn thumbnails_begin(started: Option<Instant>) {
    if let Some(started) = started {
        THUMBNAILS.with_borrow_mut(|batch| batch.start(started));
    }
}

/// The pass that dispatches the thumbnails is over. A batch every card
/// answered from the cache ends here, unreported.
pub(crate) fn thumbnails_sealed() {
    if enabled() {
        THUMBNAILS.with_borrow_mut(batch::Batch::seal);
    }
}

/// Joins the open thumbnail batch for as long as the returned value lives.
/// Meant to be moved into the render's future, so that every way out of it
/// — painted, failed, stale — counts as that thumbnail being done.
pub(crate) fn thumbnails_join() -> Option<ThumbnailRender> {
    if !enabled() {
        return None;
    }
    THUMBNAILS
        .with_borrow_mut(batch::Batch::join)
        .map(ThumbnailRender)
}

/// One thumbnail render's membership of the batch. Dropping the last one
/// out reports `organize.thumbnails`.
pub(crate) struct ThumbnailRender(batch::Ticket);

impl Drop for ThumbnailRender {
    fn drop(&mut self) {
        // `try_with`: a future still pending when the main thread exits is
        // dropped after this thread's locals may already be gone.
        let finished = THUMBNAILS
            .try_with(|batch| batch.borrow_mut().leave(self.0, Instant::now()))
            .ok()
            .flatten();
        if let Some((elapsed, count)) = finished {
            let fields: [Field; 1] = [("count", count.into())];
            emit(&line::format_line("organize.thumbnails", elapsed, &fields));
        }
    }
}

/// Stderr, locked for the one write. A failed write is dropped: a trace
/// must never be what breaks the action it measures.
fn emit(line: &str) {
    let _ = line::write_line(&mut io::stderr().lock(), line);
}
