//! The autosave timer: every [`INTERVAL_SECONDS`], every tab whose document
//! has unsaved changes — the active one and every parked one — writes its
//! bytes, pending edits applied, to that tab's own recovery entry, and a tab
//! whose document has none deletes its entry.
//!
//! The bytes are the ones a real Save would write (`pdf_save::save_document`,
//! annotation layer included — not the preview's `save_preview`), serialized
//! on a worker thread like every other write in this shell. What the main
//! thread pays per snapshot is the model clone `write::save` and
//! `write::refresh_preview` already pay per commit; the save and the disk
//! write — the parts that grow with a 1600-page file — happen off it.
//!
//! Only document bytes are recovered, never the undo history: the core has no
//! serialization of the edit log, and a restored document starts a new one.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{glib, ApplicationWindow};

use super::backoff::Backoff;
use super::policy::{decide, observe, Decision, Observed, Revision};
use super::store::{EntryKey, Orphan, RecoveryDir, Slot};
use crate::app::i18n::tr;
use crate::app::state::Viewer;
use crate::app::tabs::{self, TabId};

mod job;
use job::write;

/// How often a window looks for unsaved changes to write. A crash loses at
/// most this much work; a shorter interval would re-serialize a large
/// document more often while the user is still editing it.
pub(super) const INTERVAL_SECONDS: u32 = 60;

/// One window's autosave state, behind `Viewer::recovery`.
///
/// The default is **disabled** — no directory, so nothing is ever written or
/// deleted. Only the production startup ([`super::install`]) gives a window a
/// directory, so the hundreds of windows the GTK tests build never touch the
/// user's state directory.
#[derive(Default)]
pub(crate) struct Autosave {
    dir: Option<RecoveryDir>,
    /// One entry per tab that has written one — keyed by the tab, never by
    /// "whichever tab is active", so one tab's bytes can never land in
    /// another tab's entry.
    entries: HashMap<TabId, Entry>,
    /// Holds writes back after a failure — see [`super::backoff`]. Shared
    /// by the window's tabs: a failure is almost always the directory's.
    backoff: Backoff,
    timer: Option<glib::SourceId>,
    /// How many snapshots have been started — what the tests read to tell
    /// a skipped attempt from one that serialized the document.
    #[cfg(test)]
    attempts: u32,
    /// Holds the next write's worker just before it writes, for the tests
    /// that need a forget to land in exactly that window.
    #[cfg(test)]
    pause: Option<test_hooks::PauseHook>,
}

/// One tab's recovery entry.
#[derive(Default)]
struct Entry {
    /// Minted the first time this tab writes, and kept for the tab's
    /// lifetime: every snapshot of it reuses the one entry.
    key: Option<EntryKey>,
    slot: Option<Slot>,
    /// The revision the entry on disk holds.
    written: Option<Revision>,
    in_flight: bool,
    /// The write in flight, as a clean exit sees it; see [`shutdown`].
    worker: Option<Worker>,
    /// Bumped by every forget of this entry, before it deletes anything. A
    /// write that started before one sees the change once its files are in
    /// place and deletes them, instead of resurrecting an entry the user just
    /// saved or discarded. Shared with the worker thread — see [`write`].
    epoch: Arc<AtomicU64>,
    /// A crashed session's entry restored into this tab. Kept — and locked
    /// — until this tab has written the same document to its own entry, so
    /// a crash in between still has something to recover.
    adopted: Option<Orphan>,
}

/// Gives `viewer` a directory to write to, without a timer — what
/// [`start`] does first, and all a test needs to drive [`tick`] by hand.
pub(super) fn enable(viewer: &Viewer, dir: RecoveryDir) {
    viewer.recovery.borrow_mut().dir = Some(dir);
}

/// What a clean exit needs to know about a write in flight.
struct Worker {
    /// Disconnected once the worker has finished — its cleanup included.
    done: mpsc::Receiver<()>,
    /// Set before the worker's last forget check: from then on it may put
    /// files on disk, and a clean exit has to wait for it to take them back.
    writing: Arc<AtomicBool>,
}

/// Starts the timer for `window`, and stops it — deleting every tab's entry —
/// when the window is destroyed.
///
/// Destruction is a clean exit: the close request has already asked about
/// unsaved work, and only Save or Discard lets it through. A crash destroys
/// nothing, which is what leaves the entries for the next launch.
pub(super) fn start(window: &ApplicationWindow, viewer: &Viewer, dir: RecoveryDir) {
    enable(viewer, dir);
    let timer = glib::timeout_add_seconds_local(INTERVAL_SECONDS, {
        let viewer = viewer.clone();
        move || {
            tick(&viewer);
            glib::ControlFlow::Continue
        }
    });
    viewer.recovery.borrow_mut().timer = Some(timer);
    window.connect_destroy({
        let viewer = viewer.clone();
        move |_| {
            shutdown(&viewer);
        }
    });
}

/// How long a clean exit waits for a snapshot still being written.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

/// The window is going away cleanly: stop the timer and delete every tab's
/// entry.
pub(super) fn shutdown(viewer: &Viewer) {
    shutdown_within(viewer, SHUTDOWN_WAIT);
}

/// [`shutdown`], waiting at most `wait` for a write in flight.
///
/// The forget alone is not enough when a snapshot is still being written:
/// the worker checks the epoch only once its files are in place, and a
/// process that exits before then leaves a whole entry behind — one the next
/// launch would offer, for work the user just saved or discarded. So the
/// exit waits for the worker, bounded: a disk that hangs must not keep the
/// application from quitting, and the worst a timeout costs is that offer.
///
/// A worker still serializing is not waited for: the forget lands before its
/// last check, so it will write nothing. See `job::AutosaveWrite::run` for why that
/// holds.
pub(super) fn shutdown_within(viewer: &Viewer, wait: Duration) {
    let workers: Vec<Worker> = {
        let mut recovery = viewer.recovery.borrow_mut();
        if let Some(timer) = recovery.timer.take() {
            timer.remove();
        }
        recovery
            .entries
            .values_mut()
            .filter_map(|entry| entry.worker.take())
            .collect()
    };
    forget_all(viewer);
    for worker in workers
        .into_iter()
        .filter(|worker| worker.writing.load(Ordering::SeqCst))
    {
        if let Err(mpsc::RecvTimeoutError::Timeout) = worker.done.recv_timeout(wait) {
            eprintln!(
                "recovery: an autosave was still being written after {wait:?}; \
                 its entry may be offered at the next launch"
            );
        }
    }
}

/// One look at every tab: write it, forget its entry, or wait.
pub(super) fn tick(viewer: &Viewer) {
    tick_at(viewer, Instant::now());
}

/// [`tick`], with the clock the failure backoff is measured against.
pub(super) fn tick_at(viewer: &Viewer, now: Instant) {
    if viewer.recovery.borrow().dir.is_none() {
        return;
    }
    let observed = observe_tabs(viewer);
    let is_open = |id: &TabId| observed.iter().any(|(open, _)| open == id);
    let decisions: Vec<(TabId, Decision)> = {
        let recovery = viewer.recovery.borrow();
        let mut decisions: Vec<_> = observed
            .iter()
            .map(|(id, observed)| {
                let entry = recovery.entries.get(id);
                let written = entry.and_then(|entry| entry.written);
                let in_flight = entry.is_some_and(|entry| entry.in_flight);
                (*id, decide(*observed, written, in_flight))
            })
            .collect();
        // A tab that closed since the last tick has nothing left to protect.
        decisions.extend(
            recovery
                .entries
                .keys()
                .filter(|id| !is_open(id))
                .map(|id| (*id, Decision::Forget)),
        );
        decisions
    };
    for (id, decision) in decisions {
        match decision {
            Decision::Wait => {}
            Decision::Forget => forget_tab(viewer, id),
            // Backing off after a failure: not even serialized, since the
            // attempt would most likely fail the same way.
            Decision::Write(_) if !viewer.recovery.borrow().backoff.ready(now) => {}
            Decision::Write(revision) => write(viewer, id, revision, now),
        }
    }
    viewer
        .recovery
        .borrow_mut()
        .entries
        .retain(|id, entry| is_open(id) || entry.in_flight);
}

/// What a tick sees of every tab, read on the main thread.
///
/// The active tab's revision carries the window's `session_id` and
/// `generation`, which move on every reopen; a parked tab's carries the pair
/// it was parked at. Both only ever grow, so a revision never repeats — even
/// when a rebuild starts the edit revision over — and parking or unparking a
/// tab costs at most one redundant snapshot.
fn observe_tabs(viewer: &Viewer) -> Vec<(TabId, Observed)> {
    let mut observed = Vec::new();
    tabs::for_each_revision(viewer, |id, session, (session_id, generation)| {
        observed.push((id, observe(Some(session), session_id, generation)));
    });
    observed
}

/// A snapshot could not be written. The first failure of a streak tells the
/// user — once, on the status line, not in a dialog: their work is still
/// open in front of them, and only a crash would cost it.
///
/// `now` is the clock of the tick that made the attempt.
fn failed(viewer: &Viewer, error: &str, now: Instant) {
    eprintln!("recovery: autosave failed: {error}");
    let first = viewer.recovery.borrow_mut().backoff.failed(now);
    if first {
        viewer.status.set_text(&tr("recovery-autosave-unavailable"));
    }
}

/// Deletes the active tab's entry, and any restored one it was keeping: the
/// document it protected was saved or discarded.
///
/// Ends any failure streak too: the next document is written — and its own
/// failures reported — without waiting out the last one's backoff.
pub(super) fn forget_active(viewer: &Viewer) {
    if let Some(id) = tabs::active_tab(viewer) {
        forget_tab(viewer, id);
    }
    viewer.recovery.borrow_mut().backoff = Backoff::default();
}

/// Deletes every tab's entry — a clean exit.
fn forget_all(viewer: &Viewer) {
    let ids: Vec<TabId> = viewer.recovery.borrow().entries.keys().copied().collect();
    for id in ids {
        forget_tab(viewer, id);
    }
}

/// Deletes tab `id`'s entry, and any restored one it was keeping. Other
/// tabs' entries are left alone.
fn forget_tab(viewer: &Viewer, id: TabId) {
    let (slot, adopted) = {
        let mut recovery = viewer.recovery.borrow_mut();
        let Some(entry) = recovery.entries.get_mut(&id) else {
            return;
        };
        entry.epoch.fetch_add(1, Ordering::SeqCst);
        entry.written = None;
        (entry.slot.take(), entry.adopted.take())
    };
    if let Some(slot) = slot {
        slot.clear();
    }
    if let Some(adopted) = adopted {
        adopted.discard();
    }
}

/// Hands a crashed session's entry to tab `id` — the one its document was
/// just restored into, in front or in the background — and snapshots it
/// right away rather than a whole interval later.
pub(super) fn adopt(viewer: &Viewer, orphan: Orphan, id: TabId) {
    let previous = viewer
        .recovery
        .borrow_mut()
        .entries
        .entry(id)
        .or_default()
        .adopted
        .replace(orphan);
    if let Some(previous) = previous {
        previous.discard();
    }
    tick(viewer);
}

/// How many snapshots this window has started.
#[cfg(test)]
pub(super) fn attempts(viewer: &Viewer) -> u32 {
    viewer.recovery.borrow().attempts
}

/// Whether a snapshot of any tab is being written right now.
#[cfg(test)]
pub(super) fn is_writing(viewer: &Viewer) -> bool {
    viewer
        .recovery
        .borrow()
        .entries
        .values()
        .any(|entry| entry.in_flight)
}

/// How many tabs hold an entry on disk right now.
#[cfg(test)]
pub(super) fn held_entries(viewer: &Viewer) -> usize {
    viewer
        .recovery
        .borrow()
        .entries
        .values()
        .filter(|entry| entry.slot.is_some())
        .count()
}

/// Lets go of every entry the way a crash does: the locks are released and
/// the files stay.
#[cfg(test)]
pub(super) fn crash(viewer: &Viewer) {
    let mut recovery = viewer.recovery.borrow_mut();
    for entry in recovery.entries.values_mut() {
        drop(entry.slot.take());
        drop(entry.adopted.take());
        entry.written = None;
    }
}

#[cfg(test)]
pub(super) mod test_hooks;

/// Holds the next snapshot's worker just before it writes.
#[cfg(test)]
pub(super) fn pause_next_write(viewer: &Viewer) -> test_hooks::Pause {
    pause_next(viewer, test_hooks::Stage::Writing)
}

/// Holds the next snapshot's worker before it serializes anything.
#[cfg(test)]
pub(super) fn pause_next_serialize(viewer: &Viewer) -> test_hooks::Pause {
    pause_next(viewer, test_hooks::Stage::Serializing)
}

#[cfg(test)]
fn pause_next(viewer: &Viewer, stage: test_hooks::Stage) -> test_hooks::Pause {
    let (pause, hook) = test_hooks::pair(stage);
    viewer.recovery.borrow_mut().pause = Some(hook);
    pause
}
