//! The autosave timer: every [`INTERVAL`], a window whose document has
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
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::prelude::*;
use gtk::{gio, glib, ApplicationWindow};
use pdf_document::Document;

use super::policy::{decide, observe, Decision, Revision};
use super::store::{EntryKey, Meta, Orphan, RecoveryDir, Slot};
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
            if let Some(timer) = viewer.recovery.borrow_mut().timer.take() {
                timer.remove();
            }
            forget(&viewer);
        }
    });
}

/// One look at the document: write it, forget its entry, or wait.
pub(super) fn tick(viewer: &Viewer) {
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
        Decision::Write(revision) => write(viewer, revision),
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
    meta: Meta,
}

fn write(viewer: &Viewer, revision: Revision) {
    let Some(snapshot) = take_snapshot(viewer) else {
        return;
    };
    let (paths, epoch) = {
        let mut recovery = viewer.recovery.borrow_mut();
        let Some(dir) = recovery.dir.clone() else {
            return;
        };
        if recovery.slot.is_none() {
            let key = recovery.key.get_or_insert_with(EntryKey::fresh).clone();
            match dir.open_slot(&key) {
                Ok(slot) => recovery.slot = Some(slot),
                Err(error) => {
                    eprintln!("recovery: cannot open {}: {error}", dir.root().display());
                    return;
                }
            }
        }
        let Some(paths) = recovery.slot.as_ref().map(|slot| slot.paths().clone()) else {
            return;
        };
        recovery.in_flight = true;
        let epoch = Arc::clone(&recovery.epoch);
        let started = epoch.load(Ordering::SeqCst);
        (paths, (epoch, started))
    };

    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let (shared_epoch, started) = epoch;
            let forgotten = move || shared_epoch.load(Ordering::SeqCst) != started;
            let result = gio::spawn_blocking(move || {
                let bytes =
                    snapshot_bytes(&snapshot.document, &snapshot.backing, &snapshot.sources)?;
                if forgotten() {
                    return Ok(false);
                }
                paths
                    .write(&bytes, &snapshot.meta)
                    .map_err(|error| error.to_string())?;
                // Checked here, on the worker, and not only once the result
                // reaches the main loop: a window destroyed mid-write takes
                // the main loop with it, so the completion below may never
                // run — and files it should have deleted would be offered
                // for recovery at the next launch.
                if forgotten() {
                    paths.remove_data();
                    return Ok(false);
                }
                Ok(true)
            })
            .await
            .unwrap_or_else(|_| Err("the autosave worker stopped unexpectedly".to_owned()));

            let adopted = {
                let mut recovery = viewer.recovery.borrow_mut();
                recovery.in_flight = false;
                match result {
                    // Still the entry this window holds: the restored
                    // document, if any, is now safely in it.
                    Ok(true) if recovery.epoch.load(Ordering::SeqCst) == started => {
                        recovery.written = Some(revision);
                        recovery.adopted.take()
                    }
                    // Forgotten while writing; whoever forgot it deleted it.
                    Ok(_) => None,
                    Err(error) => {
                        eprintln!("recovery: autosave failed: {error}");
                        None
                    }
                }
            };
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
        meta: Meta {
            saved_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs() as i64),
            name: session.base_name.clone(),
            origin: session.origin_path.clone(),
        },
    })
}

/// The bytes a Save of this session would write.
///
/// Signatures are acknowledged silently, as the preview refresh and the print
/// snapshot do: a recovery file is not the user's document, and the real
/// Save of a restored one still meets `confirm_signature_loss` — its bytes
/// carry the signature it would break.
fn snapshot_bytes(
    document: &Document,
    backing: &SaveBacking,
    sources: &[ImportedSource],
) -> Result<Vec<u8>, String> {
    let source_refs: Vec<_> = sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect();
    pdf_save::save_document(pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::document::{open_document, SAMPLE_PDF};
    use crate::app::state::DocumentSource;
    use crate::app::test_fixtures::a_highlight;
    use pdf_document::Command;

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

        let bytes = snapshot_bytes(&model, &backing, &[]).expect("the snapshot serializes");

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
}
