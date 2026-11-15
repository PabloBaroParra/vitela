//! The autosave timer: every [`INTERVAL_SECONDS`], a window whose document has
//! unsaved changes writes its bytes — pending edits applied — to its own
//! recovery entry, and a window whose document has none deletes it.
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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gtk::prelude::*;
use gtk::{gio, glib, ApplicationWindow};
use pdf_document::Document;

use super::backoff::Backoff;
use super::meta::Meta;
use super::policy::{decide, observe, Decision, Revision};
use super::store::{EntryKey, EntryPaths, Orphan, RecoveryDir, Slot};
use crate::app::i18n::tr;
use crate::app::state::{ImportedSource, SaveBacking, Viewer};

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
    /// Minted the first time this window writes, and kept for its lifetime:
    /// every document the window shows reuses the one entry.
    key: Option<EntryKey>,
    slot: Option<Slot>,
    /// The revision the entry on disk holds.
    written: Option<Revision>,
    in_flight: bool,
    /// Disconnected once the write in flight has finished on its worker —
    /// its cleanup included. What a clean exit waits on; see [`shutdown`].
    worker: Option<mpsc::Receiver<()>>,
    /// Holds writes back after a failure — see [`super::backoff`].
    backoff: Backoff,
    /// Bumped by every [`forget`], before it deletes anything. A write that
    /// started before one sees the change once its files are in place and
    /// deletes them, instead of resurrecting an entry the user just saved or
    /// discarded. Shared with the worker thread — see [`write`].
    epoch: Arc<AtomicU64>,
    /// A crashed session's entry this window restored. Kept — and locked —
    /// until this window has written the same document to its own entry, so a
    /// crash in between still has something to recover.
    adopted: Option<Orphan>,
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

/// Gives `viewer` a directory to write to, without a timer — what
/// [`start`] does first, and all a test needs to drive [`tick`] by hand.
pub(super) fn enable(viewer: &Viewer, dir: RecoveryDir) {
    viewer.recovery.borrow_mut().dir = Some(dir);
}

/// Starts the timer for `window`, and stops it — deleting the entry — when
/// the window is destroyed.
///
/// Destruction is a clean exit: the close request has already asked about
/// unsaved work, and only Save or Discard lets it through. A crash destroys
/// nothing, which is what leaves the entry for the next launch.
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

/// The window is going away cleanly: stop the timer and delete the entry.
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
pub(super) fn shutdown_within(viewer: &Viewer, wait: Duration) {
    let worker = {
        let mut recovery = viewer.recovery.borrow_mut();
        if let Some(timer) = recovery.timer.take() {
            timer.remove();
        }
        recovery.worker.take()
    };
    forget(viewer);
    if let Some(worker) = worker {
        if let Err(mpsc::RecvTimeoutError::Timeout) = worker.recv_timeout(wait) {
            eprintln!(
                "recovery: an autosave was still being written after {wait:?}; \
                 its entry may be offered at the next launch"
            );
        }
    }
}

/// One look at the document: write it, forget its entry, or wait.
pub(super) fn tick(viewer: &Viewer) {
    tick_at(viewer, Instant::now());
}

/// [`tick`], with the clock the failure backoff is measured against.
pub(super) fn tick_at(viewer: &Viewer, now: Instant) {
    let decision = {
        let recovery = viewer.recovery.borrow();
        if recovery.dir.is_none() {
            return;
        }
        let state = viewer.state.borrow();
        let observed = observe(state.session.as_ref(), state.session_id, state.generation);
        decide(observed, recovery.written, recovery.in_flight)
    };
    match decision {
        Decision::Wait => {}
        Decision::Forget => forget(viewer),
        // Backing off after a failure: not even serialized, since the
        // attempt would most likely fail the same way.
        Decision::Write(_) if !viewer.recovery.borrow().backoff.ready(now) => {}
        Decision::Write(revision) => write(viewer, revision),
    }
}

/// A snapshot could not be written. The first failure of a streak tells the
/// user — once, on the status line, not in a dialog: their work is still
/// open in front of them, and only a crash would cost it.
fn failed(viewer: &Viewer, error: &str) {
    eprintln!("recovery: autosave failed: {error}");
    let first = viewer.recovery.borrow_mut().backoff.failed(Instant::now());
    if first {
        viewer.status.set_text(&tr("recovery-autosave-unavailable"));
    }
}

/// Deletes this window's entry, and any restored one it was keeping: the
/// document it protected was saved, closed, discarded or replaced.
pub(super) fn forget(viewer: &Viewer) {
    let (slot, adopted) = {
        let mut recovery = viewer.recovery.borrow_mut();
        recovery.epoch.fetch_add(1, Ordering::SeqCst);
        recovery.written = None;
        (recovery.slot.take(), recovery.adopted.take())
    };
    if let Some(slot) = slot {
        slot.clear();
    }
    if let Some(adopted) = adopted {
        adopted.discard();
    }
}

/// Takes over a crashed session's entry once its document is on screen, and
/// snapshots it right away rather than a whole interval later.
pub(super) fn adopt(viewer: &Viewer, orphan: Orphan) {
    if let Some(previous) = viewer.recovery.borrow_mut().adopted.replace(orphan) {
        previous.discard();
    }
    tick(viewer);
}

/// How many snapshots this window has started.
#[cfg(test)]
pub(super) fn attempts(viewer: &Viewer) -> u32 {
    viewer.recovery.borrow().attempts
}

/// Whether a snapshot is being written right now.
#[cfg(test)]
pub(super) fn is_writing(viewer: &Viewer) -> bool {
    viewer.recovery.borrow().in_flight
}

/// Lets go of the entry the way a crash does: the lock is released and the
/// files stay.
#[cfg(test)]
pub(super) fn crash(viewer: &Viewer) {
    let mut recovery = viewer.recovery.borrow_mut();
    drop(recovery.slot.take());
    drop(recovery.adopted.take());
    recovery.written = None;
}

/// What the worker needs, read off the session on the main thread.
struct Snapshot {
    document: Document,
    backing: SaveBacking,
    sources: Vec<ImportedSource>,
    /// `DocumentSession::base_breaks_signatures`: the bytes the session
    /// opened from had already broken a signature.
    base_breaks_signatures: bool,
    meta: Meta,
}

/// One snapshot's work on the worker thread.
struct Job {
    snapshot: Snapshot,
    paths: EntryPaths,
    epoch: Arc<AtomicU64>,
    started: u64,
    /// Dropped with the job, which is what tells [`shutdown`] it finished.
    _done: mpsc::Sender<()>,
    #[cfg(test)]
    pause: Option<test_hooks::PauseHook>,
}

impl Job {
    fn forgotten(&self) -> bool {
        self.epoch.load(Ordering::SeqCst) != self.started
    }

    /// `Ok(true)` when the entry now holds the snapshot, `Ok(false)` when it
    /// was forgotten meanwhile and holds nothing.
    fn run(mut self) -> Result<bool, String> {
        let snapshot = &self.snapshot;
        let serialized = serialize(
            &snapshot.document,
            &snapshot.backing,
            &snapshot.sources,
            snapshot.base_breaks_signatures,
        )?;
        self.snapshot.meta.breaks_signatures = serialized.breaks_signatures;
        if self.forgotten() {
            return Ok(false);
        }
        #[cfg(test)]
        if let Some(pause) = &self.pause {
            pause.hold();
        }
        self.paths
            .write(&serialized.bytes, &self.snapshot.meta)
            .map_err(|error| error.to_string())?;
        // Checked here, on the worker, and not only once the result reaches
        // the main loop: a window destroyed mid-write takes the main loop
        // with it, so the completion in [`write`] may never run — and files
        // it should have deleted would be offered for recovery at the next
        // launch.
        if self.forgotten() {
            self.paths.remove_data();
            return Ok(false);
        }
        Ok(true)
    }
}

fn write(viewer: &Viewer, revision: Revision) {
    let Some(snapshot) = take_snapshot(viewer) else {
        return;
    };
    let job = {
        let mut recovery = viewer.recovery.borrow_mut();
        #[cfg(test)]
        {
            recovery.attempts += 1;
        }
        let Some(dir) = recovery.dir.clone() else {
            return;
        };
        if recovery.slot.is_none() {
            let key = recovery.key.get_or_insert_with(EntryKey::fresh).clone();
            match dir.open_slot(&key) {
                Ok(slot) => recovery.slot = Some(slot),
                Err(error) => {
                    drop(recovery);
                    failed(
                        viewer,
                        &format!("cannot open {}: {error}", dir.root().display()),
                    );
                    return;
                }
            }
        }
        let Some(paths) = recovery.slot.as_ref().map(|slot| slot.paths().clone()) else {
            return;
        };
        recovery.in_flight = true;
        let (done, worker) = mpsc::channel();
        recovery.worker = Some(worker);
        let epoch = Arc::clone(&recovery.epoch);
        let started = epoch.load(Ordering::SeqCst);
        Job {
            snapshot,
            paths,
            epoch,
            started,
            _done: done,
            #[cfg(test)]
            pause: recovery.pause.take(),
        }
    };
    let started = job.started;
    // Started here rather than inside the future below, which only runs once
    // the main loop next turns: a window destroyed before then must still
    // find its worker running, finishing and cleaning up.
    let running = gio::spawn_blocking(move || job.run());

    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = running
                .await
                .unwrap_or_else(|_| Err("the autosave worker stopped unexpectedly".to_owned()));

            let (adopted, error) = {
                let mut recovery = viewer.recovery.borrow_mut();
                recovery.in_flight = false;
                match result {
                    // Still the entry this window holds: the restored
                    // document, if any, is now safely in it.
                    Ok(true) if recovery.epoch.load(Ordering::SeqCst) == started => {
                        recovery.written = Some(revision);
                        recovery.backoff.succeeded();
                        (recovery.adopted.take(), None)
                    }
                    // Forgotten while writing; whoever forgot it deleted it.
                    Ok(_) => (None, None),
                    Err(error) => (None, Some(error)),
                }
            };
            if let Some(error) = error {
                failed(&viewer, &error);
            }
            // The restored document is now in this window's own entry.
            if let Some(adopted) = adopted {
                adopted.discard();
            }
        }
    });
}

fn take_snapshot(viewer: &Viewer) -> Option<Snapshot> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    Some(Snapshot {
        document: session.document_model.clone()?,
        backing: session.save_backing.clone()?,
        sources: session.imported_sources.clone(),
        base_breaks_signatures: session.base_breaks_signatures,
        meta: Meta {
            saved_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs() as i64),
            name: session.base_name.clone(),
            origin: session.origin_path.clone(),
            // Answered on the worker, beside the save it describes.
            breaks_signatures: false,
        },
    })
}

/// A snapshot's bytes, and whether they broke a signature.
struct Serialized {
    bytes: Vec<u8>,
    breaks_signatures: bool,
}

/// The bytes a Save of this session would write.
///
/// Signatures are acknowledged silently, as the preview refresh and the print
/// snapshot do: a recovery file is not the user's document. What the user
/// would have been asked is recorded instead — `breaks_signatures`, carried
/// into the sidecar — because once restored, these bytes *are* the base, and
/// the edit that broke the signature is no longer there for
/// `pdf_save::will_invalidate_signatures` to find.
fn serialize(
    document: &Document,
    backing: &SaveBacking,
    sources: &[ImportedSource],
    base_breaks_signatures: bool,
) -> Result<Serialized, String> {
    let source_refs: Vec<_> = sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect();
    let input = |signatures: pdf_save::SignatureAcknowledgement| pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    };
    // The question `write::save` asks before a real Save, asked the same way.
    let breaks_signatures = base_breaks_signatures
        || pdf_save::will_invalidate_signatures(input(
            pdf_save::SignatureAcknowledgement::Unacknowledged,
        ))
        .unwrap_or(false);
    let bytes = pdf_save::save_document(input(
        pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
    ))
    .map_err(|error| error.to_string())?;
    Ok(Serialized {
        bytes,
        breaks_signatures,
    })
}

#[cfg(test)]
pub(super) mod test_hooks;

/// Holds the next snapshot's worker just before it writes.
#[cfg(test)]
pub(super) fn pause_next_write(viewer: &Viewer) -> test_hooks::Pause {
    let (pause, hook) = test_hooks::pair();
    viewer.recovery.borrow_mut().pause = Some(hook);
    pause
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::document::{open_document, SAMPLE_PDF};
    use crate::app::state::DocumentSource;
    use crate::app::test_fixtures::a_highlight;
    use pdf_document::{Command, Orientation, Page, PageSize};

    /// What a recovery file is for: the annotation exists only in the model —
    /// the canvas overlay draws it, no file holds it — and must survive in
    /// the bytes a crash leaves behind.
    #[test]
    fn the_snapshot_carries_unsaved_annotations() {
        let opened =
            open_document(&DocumentSource::Embedded(SAMPLE_PDF), None).expect("the sample opens");
        let mut model = opened.document_model.expect("the sample is editable");
        let backing = opened.save_backing.expect("the sample has a save backing");
        let page = model.pages[0].id;
        let mut log = std::mem::take(&mut model.pending_edits);
        log.apply(&mut model, Command::AddAnnotation(a_highlight(1, page)));
        model.pending_edits = log;

        let bytes = serialize(&model, &backing, &[], false)
            .expect("the snapshot serializes")
            .bytes;

        let (reopened, _) =
            pdf_manip::open_document_from_bytes(&bytes, None).expect("the snapshot reopens");
        let highlights = reopened
            .as_lopdf()
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .filter(|dict| {
                dict.get(b"Subtype")
                    .and_then(|subtype| subtype.as_name())
                    .is_ok_and(|name| name == b"Highlight")
            })
            .count();
        assert_eq!(highlights, 1);
        let _ = pdf_render::PdfiumRenderer::new().close_document(opened.document);
    }

    const SIGNED_PDF: &[u8] =
        include_bytes!("../../../../../tests/fixtures/signed/rsa2048_sha256.pdf");

    /// Opens `bytes` and records the command `edit` builds.
    fn edited(
        bytes: &'static [u8],
        edit: impl FnOnce(&mut Document) -> Command,
    ) -> (Document, SaveBacking) {
        let opened = open_document(&DocumentSource::Embedded(bytes), None).expect("it opens");
        let _ = pdf_render::PdfiumRenderer::new().close_document(opened.document);
        let mut model = opened.document_model.expect("it is editable");
        let backing = opened.save_backing.expect("it has a save backing");
        let command = edit(&mut model);
        let mut log = std::mem::take(&mut model.pending_edits);
        log.apply(&mut model, command);
        model.pending_edits = log;
        (model, backing)
    }

    fn a_highlight_on_page_one(model: &mut Document) -> Command {
        Command::AddAnnotation(a_highlight(1, model.pages[0].id))
    }

    /// A new page is a structural change only the rewriter can write, and
    /// the rewrite replaces the bytes the signature covers.
    #[test]
    fn a_snapshot_that_breaks_a_signature_records_it() {
        let (model, backing) = edited(SIGNED_PDF, |model| Command::InsertPage {
            index: 1,
            page: Page::blank(
                model.allocate_page_id().expect("a page id is free"),
                PageSize::A4,
                Orientation::Portrait,
            ),
            annotations: Vec::new(),
            form_fields: Vec::new(),
        });

        let serialized = serialize(&model, &backing, &[], false).expect("it serializes");

        assert!(serialized.breaks_signatures);
    }

    #[test]
    fn a_snapshot_of_an_unsigned_document_breaks_nothing() {
        let (model, backing) = edited(SAMPLE_PDF, a_highlight_on_page_one);

        let serialized = serialize(&model, &backing, &[], false).expect("it serializes");

        assert!(!serialized.breaks_signatures);
    }

    /// Once restored, the broken signature is in the base and no pending edit
    /// shows it — so the session's flag is what carries it into the next
    /// snapshot.
    #[test]
    fn a_restored_base_that_broke_a_signature_keeps_saying_so() {
        let (model, backing) = edited(SAMPLE_PDF, a_highlight_on_page_one);

        let serialized = serialize(&model, &backing, &[], true).expect("it serializes");

        assert!(serialized.breaks_signatures);
    }
}
