//! One snapshot of one tab: reading it off the session on the main thread,
//! serializing it and writing it to the tab's entry on a worker, and folding
//! the result back into that tab's bookkeeping.
//!
//! Split from [`super`], which decides *when* each tab is written or
//! forgotten; this is *how* one write happens.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gtk::{gio, glib};
use pdf_document::Document;

use super::super::meta::Meta;
use super::super::policy::Revision;
use super::super::store::{EntryKey, EntryPaths};
#[cfg(test)]
use super::test_hooks;
use super::{failed, Worker};
use crate::app::state::{DocumentSession, ImportedSource, SaveBacking, Viewer};
use crate::app::tabs::{self, TabId};
use crate::app::write::default_save_breaks_signature;

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
struct AutosaveWrite {
    snapshot: Snapshot,
    paths: EntryPaths,
    epoch: Arc<AtomicU64>,
    started: u64,
    /// See [`Worker::writing`].
    writing: Arc<AtomicBool>,
    /// Declared before `_done` on purpose: fields drop in declaration order,
    /// and a test reading the hook's "finished" after [`shutdown`] returns
    /// must find it already set.
    #[cfg(test)]
    pause: Option<test_hooks::PauseHook>,
    /// Dropped with the job, which is what tells [`shutdown`] it finished.
    _done: mpsc::Sender<()>,
}

impl AutosaveWrite {
    fn forgotten(&self) -> bool {
        self.epoch.load(Ordering::SeqCst) != self.started
    }

    /// `Ok(true)` when the entry now holds the snapshot, `Ok(false)` when it
    /// was forgotten meanwhile and holds nothing.
    fn run(mut self) -> Result<bool, String> {
        #[cfg(test)]
        self.hold_at(test_hooks::Stage::Serializing);
        let snapshot = &self.snapshot;
        let serialized = serialize(
            &snapshot.document,
            &snapshot.backing,
            &snapshot.sources,
            snapshot.base_breaks_signatures,
        )?;
        self.snapshot.meta.breaks_signatures = serialized.breaks_signatures;
        // Set before the check, both sequentially consistent: a shutdown
        // that bumps the epoch and then finds this unset is ordered before
        // it, so the check below sees the bump and nothing is written.
        self.writing.store(true, Ordering::SeqCst);
        if self.forgotten() {
            return Ok(false);
        }
        #[cfg(test)]
        self.hold_at(test_hooks::Stage::Writing);
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

    #[cfg(test)]
    fn hold_at(&self, stage: test_hooks::Stage) {
        if let Some(pause) = self.pause.as_ref().filter(|pause| pause.stage == stage) {
            pause.hold();
        }
    }
}

pub(super) fn write(viewer: &Viewer, id: TabId, revision: Revision, now: Instant) {
    let Some(snapshot) = take_snapshot(viewer, id) else {
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
        #[cfg(test)]
        let pause = recovery.pause.take();
        let entry = recovery.entries.entry(id).or_default();
        if entry.slot.is_none() {
            let key = entry.key.get_or_insert_with(EntryKey::fresh).clone();
            match dir.open_slot(&key) {
                Ok(slot) => entry.slot = Some(slot),
                Err(error) => {
                    drop(recovery);
                    failed(
                        viewer,
                        &format!("cannot open {}: {error}", dir.root().display()),
                        now,
                    );
                    return;
                }
            }
        }
        let Some(paths) = entry.slot.as_ref().map(|slot| slot.paths().clone()) else {
            return;
        };
        entry.in_flight = true;
        let (done, finished) = mpsc::channel();
        let writing = Arc::new(AtomicBool::new(false));
        entry.worker = Some(Worker {
            done: finished,
            writing: Arc::clone(&writing),
        });
        let epoch = Arc::clone(&entry.epoch);
        let started = epoch.load(Ordering::SeqCst);
        AutosaveWrite {
            snapshot,
            paths,
            epoch,
            started,
            writing,
            #[cfg(test)]
            pause,
            _done: done,
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
                let landed = matches!(result, Ok(true));
                let adopted = recovery.entries.get_mut(&id).and_then(|entry| {
                    entry.in_flight = false;
                    // Still the entry this tab holds: the restored
                    // document, if any, is now safely in it.
                    (landed && entry.epoch.load(Ordering::SeqCst) == started).then(|| {
                        entry.written = Some(revision);
                        entry.adopted.take()
                    })?
                });
                match result {
                    Ok(true) => {
                        recovery.backoff.succeeded();
                        (adopted, None)
                    }
                    // Forgotten while writing; whoever forgot it deleted it.
                    Ok(false) => (adopted, None),
                    Err(error) => (adopted, Some(error)),
                }
            };
            if let Some(error) = error {
                failed(&viewer, &error, now);
            }
            // The restored document is now in this tab's own entry.
            if let Some(adopted) = adopted {
                adopted.discard();
            }
        }
    });
}

/// What the worker needs from tab `id`'s session, whether it is active or
/// parked.
fn take_snapshot(viewer: &Viewer, id: TabId) -> Option<Snapshot> {
    let mut snapshot = None;
    tabs::for_each_session(viewer, |tab, session, _| {
        if tab == id {
            snapshot = snapshot_of(session);
        }
    });
    snapshot
}

fn snapshot_of(session: &DocumentSession) -> Option<Snapshot> {
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
    // The question `write::save` asks before a real Save, asked the same way.
    let breaks_signatures =
        default_save_breaks_signature(document, backing, sources, base_breaks_signatures);
    let source_refs: Vec<_> = sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect();
    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .map_err(|error| error.to_string())?;
    Ok(Serialized {
        bytes,
        breaks_signatures,
    })
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
        include_bytes!("../../../../../../tests/fixtures/signed/rsa2048_sha256.pdf");

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
