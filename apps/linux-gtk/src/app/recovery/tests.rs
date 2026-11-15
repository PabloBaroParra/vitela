//! The recovery flows through a real window: the timer's tick writing and
//! forgetting entries, and a crashed entry coming back as the unsaved
//! document it was.
//!
//! Every window here gets its own scratch directory through
//! [`autosave::enable`]; nothing reaches the user's state directory, and the
//! directory is removed by a `Drop` guard even when a test panics.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gtk::prelude::*;
use pdf_document::Command;

use super::autosave::{
    self, attempts, crash, is_writing, pause_next_serialize, pause_next_write, shutdown_within,
    tick, tick_at,
};
use super::meta::Meta;
use super::restore::restore;
use super::store::{EntryKey, RecoveryDir};
use crate::app::document::{begin_loading, open_document, show_document, SAMPLE_PDF};
use crate::app::i18n::tr;
use crate::app::state::{DocumentSource, Viewer};
use crate::app::test_fixtures::{a_highlight, one_password_security};
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> (Scratch, RecoveryDir) {
    let base = std::env::temp_dir().join(format!(
        "vitela-recovery-ui-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_nanos()
    ));
    let dir = RecoveryDir::new(base.join("recovery"));
    (Scratch(base), dir)
}

/// A window showing the sample, autosaving into `dir`.
struct Window {
    built: BuiltUi,
}

impl Window {
    fn over(dir: &RecoveryDir) -> Self {
        let built = built_ui();
        autosave::enable(&built.viewer, dir.clone());
        let opened =
            open_document(&DocumentSource::Embedded(SAMPLE_PDF), None).expect("the sample opens");
        let generation = begin_loading(&built.viewer);
        show_document(&built.viewer, generation, opened);
        Self { built }
    }

    fn viewer(&self) -> &Viewer {
        &self.built.viewer
    }

    /// Records one annotation the way a toolbar command does.
    fn edit(&self) {
        let mut state = self.viewer().state.borrow_mut();
        let session = state.session.as_mut().expect("a document is open");
        let model = session
            .document_model
            .as_mut()
            .expect("the sample is editable");
        let page = model.pages[0].id;
        let id = session.next_annotation_id;
        session.next_annotation_id += 1;
        let mut log = std::mem::take(&mut model.pending_edits);
        log.apply(model, Command::AddAnnotation(a_highlight(id, page)));
        model.pending_edits = log;
        session.edit_revision += 1;
        session.unsaved_to_disk = true;
    }

    fn tick_and_settle(&self) {
        tick(self.viewer());
        settle_until(|| !is_writing(self.viewer()));
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // The session goes first so the close raises no unsaved-changes
        // prompt.
        self.built.viewer.state.borrow_mut().session = None;
        self.built.window.close();
    }
}

fn settle_until(done: impl Fn() -> bool) {
    let context = gtk::glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "the main loop never settled");
        while context.iteration(false) {}
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn files_in(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<_> = entries
        .map(|entry| entry.expect("an entry").file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn extensions(root: &Path) -> Vec<String> {
    files_in(root)
        .iter()
        .map(|name| name.rsplit('.').next().unwrap().to_owned())
        .collect()
}

#[gtk::test]
fn gtk_ui_unsaved_changes_are_written_to_an_entry() {
    let (_scratch, dir) = scratch("written");
    let window = Window::over(&dir);
    window.edit();

    window.tick_and_settle();

    assert_eq!(extensions(dir.root()), ["lock", "meta", "pdf"]);
}

#[gtk::test]
fn gtk_ui_a_document_with_no_unsaved_changes_writes_nothing() {
    let (_scratch, dir) = scratch("clean");
    let window = Window::over(&dir);

    window.tick_and_settle();

    assert!(files_in(dir.root()).is_empty());
}

#[gtk::test]
fn gtk_ui_an_unchanged_document_is_not_written_again() {
    let (_scratch, dir) = scratch("unchanged");
    let window = Window::over(&dir);
    window.edit();
    window.tick_and_settle();
    let pdf = dir
        .root()
        .join(files_in(dir.root()).pop().expect("the entry's bytes"));
    fs::remove_file(&pdf).expect("the bytes are there");

    window.tick_and_settle();
    assert!(!pdf.exists(), "nothing changed, so nothing was written");

    window.edit();
    window.tick_and_settle();
    assert!(pdf.exists(), "the next edit is written");
}

#[gtk::test]
fn gtk_ui_a_document_back_in_step_with_disk_forgets_its_entry() {
    let (_scratch, dir) = scratch("back-to-clean");
    let window = Window::over(&dir);
    window.edit();
    window.tick_and_settle();

    if let Some(session) = window.viewer().state.borrow_mut().session.as_mut() {
        session.unsaved_to_disk = false;
    }
    window.tick_and_settle();

    assert!(files_in(dir.root()).is_empty());
}

/// What a successful Save, an open after Discard and a clean close all call.
#[gtk::test]
fn gtk_ui_forgetting_deletes_the_entry() {
    let (_scratch, dir) = scratch("forget");
    let window = Window::over(&dir);
    window.edit();
    window.tick_and_settle();

    super::forget(window.viewer());

    assert!(files_in(dir.root()).is_empty());
}

/// The race the epoch exists for: a Save lands while the snapshot is still
/// being written, and the late write must not bring the entry back.
#[gtk::test]
fn gtk_ui_a_write_that_lands_after_a_forget_does_not_resurrect_the_entry() {
    let (_scratch, dir) = scratch("late-write");
    let window = Window::over(&dir);
    window.edit();

    tick(window.viewer());
    assert!(is_writing(window.viewer()));
    super::forget(window.viewer());
    settle_until(|| !is_writing(window.viewer()));

    assert!(files_in(dir.root()).is_empty());
}

#[gtk::test]
fn gtk_ui_an_encrypted_document_is_never_written() {
    let (_scratch, dir) = scratch("encrypted");
    let window = Window::over(&dir);
    window.edit();
    if let Some(session) = window.viewer().state.borrow_mut().session.as_mut() {
        session.document_model.as_mut().unwrap().security = Some(one_password_security());
    }

    window.tick_and_settle();

    assert!(files_in(dir.root()).is_empty());
}

#[gtk::test]
fn gtk_ui_a_crashed_entry_comes_back_as_the_unsaved_document() {
    let (_scratch, dir) = scratch("restore");
    let origin = PathBuf::from("/home/someone/quote.pdf");
    let crashed = Window::over(&dir);
    crashed.edit();
    if let Some(session) = crashed.viewer().state.borrow_mut().session.as_mut() {
        session.base_name = "quote.pdf".to_owned();
        session.origin_path = Some(origin.clone());
    }
    crashed.tick_and_settle();
    let crashed_files = files_in(dir.root());
    crash(crashed.viewer());

    let mut orphans = dir.orphans();
    assert_eq!(orphans.len(), 1, "the crashed entry is offered");
    let restored = Window::over(&dir);
    restore(
        &restored.built.window,
        restored.viewer(),
        orphans.remove(0),
        Vec::new(),
        None,
    );
    settle_until(|| {
        restored.viewer().status.text() == tr("recovery-restored") && !is_writing(restored.viewer())
    });

    {
        let state = restored.viewer().state.borrow();
        let session = state.session.as_ref().expect("the restored document");
        assert!(session.unsaved_to_disk, "restored work is unsaved work");
        assert_eq!(session.base_name, "quote.pdf");
        assert_eq!(session.origin_path.as_deref(), Some(origin.as_path()));
        let model = session.document_model.as_ref().expect("an editable model");
        assert!(
            !model.pending_edits.can_undo(),
            "bytes only: the undo history is not recovered"
        );
    }
    assert!(
        restored.viewer().save_button.is_sensitive(),
        "the status asks for a Save, so the button must offer one"
    );
    // The window took the document into an entry of its own, and only then
    // let the crashed one go.
    let now = files_in(dir.root());
    assert_eq!(extensions(dir.root()), ["lock", "meta", "pdf"]);
    assert!(
        now.iter().all(|name| !crashed_files.contains(name)),
        "{now:?} still holds the crashed entry {crashed_files:?}"
    );
}

#[gtk::test]
fn gtk_ui_an_entry_that_cannot_be_opened_is_discarded() {
    let (_scratch, dir) = scratch("unreadable");
    drop({
        let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
        let meta = Meta {
            saved_at: 1,
            name: "broken.pdf".to_owned(),
            origin: None,
            breaks_signatures: false,
        };
        slot.paths()
            .write(b"%PDF-1.7 and then nothing", &meta)
            .unwrap();
        slot
    });
    let window = Window::over(&dir);
    let session_before = window.viewer().state.borrow().session_id;

    let mut orphans = dir.orphans();
    restore(
        &window.built.window,
        window.viewer(),
        orphans.remove(0),
        Vec::new(),
        None,
    );
    settle_until(|| window.viewer().status.text() == tr("recovery-unreadable"));

    assert!(files_in(dir.root()).is_empty(), "the entry is discarded");
    assert_eq!(
        window.viewer().state.borrow().session_id,
        session_before,
        "the open document stays on screen"
    );
}

/// Leaves an entry the way a crashed window would: written, then let go.
fn crashed_entry(dir: &RecoveryDir, bytes: &[u8], meta: &Meta) {
    let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
    slot.paths().write(bytes, meta).unwrap();
}

fn a_meta(name: &str) -> Meta {
    Meta {
        saved_at: 1,
        name: name.to_owned(),
        origin: None,
        breaks_signatures: false,
    }
}

fn the_file_ending(root: &Path, extension: &str) -> PathBuf {
    let name = files_in(root)
        .into_iter()
        .find(|name| name.ends_with(extension))
        .unwrap_or_else(|| panic!("no {extension} file in {root:?}"));
    root.join(name)
}

/// The window the post-write check exists for: the forget lands after the
/// worker's last check before writing, so the bytes still reach the disk and
/// only the check after the write can take them back.
#[gtk::test]
fn gtk_ui_a_forget_during_the_disk_write_leaves_nothing_behind() {
    let (_scratch, dir) = scratch("forget-mid-write");
    let window = Window::over(&dir);
    window.edit();
    let pause = pause_next_write(window.viewer());

    tick(window.viewer());
    pause.wait_until_held();
    super::forget(window.viewer());
    (pause.releaser())();
    settle_until(|| !is_writing(window.viewer()));

    assert!(
        files_in(dir.root()).is_empty(),
        "{:?}",
        files_in(dir.root())
    );
}

/// A clean exit with a snapshot still on its way to disk: the process must
/// not end before the worker has removed what it wrote, or the next launch
/// offers work that was saved or discarded.
#[gtk::test]
fn gtk_ui_a_clean_shutdown_waits_for_the_write_in_flight() {
    let (_scratch, dir) = scratch("shutdown-waits");
    let window = Window::over(&dir);
    window.edit();
    let pause = pause_next_write(window.viewer());
    tick(window.viewer());
    pause.wait_until_held();
    let release = pause.releaser();
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        release();
    });

    shutdown_within(window.viewer(), Duration::from_secs(10));

    assert!(
        pause.worker_finished(),
        "shutdown returned with the write still running"
    );
    assert!(
        files_in(dir.root()).is_empty(),
        "{:?}",
        files_in(dir.root())
    );
    releaser.join().unwrap();
    settle_until(|| !is_writing(window.viewer()));
}

/// The wait is bounded: a write that never finishes does not keep the
/// application from quitting.
#[gtk::test]
fn gtk_ui_a_stuck_write_does_not_hold_the_shutdown() {
    let (_scratch, dir) = scratch("shutdown-bounded");
    let window = Window::over(&dir);
    window.edit();
    let pause = pause_next_write(window.viewer());
    tick(window.viewer());
    pause.wait_until_held();
    let started = Instant::now();

    shutdown_within(window.viewer(), Duration::from_millis(100));

    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!pause.worker_finished());
    (pause.releaser())();
    settle_until(|| !is_writing(window.viewer()));
    assert!(files_in(dir.root()).is_empty());
}

/// A snapshot still being serialized has written nothing, and the forget
/// the exit makes lands before its last check: there is nothing to wait for.
#[gtk::test]
fn gtk_ui_a_shutdown_does_not_wait_for_a_snapshot_still_serializing() {
    let (_scratch, dir) = scratch("shutdown-serializing");
    let window = Window::over(&dir);
    window.edit();
    let pause = pause_next_serialize(window.viewer());
    tick(window.viewer());
    pause.wait_until_held();
    let started = Instant::now();

    shutdown_within(window.viewer(), Duration::from_secs(10));

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "shutdown waited for a worker that had written nothing"
    );
    (pause.releaser())();
    settle_until(|| !is_writing(window.viewer()));
    assert!(
        files_in(dir.root()).is_empty(),
        "{:?}",
        files_in(dir.root())
    );
}

/// A full or read-only disk: one notice, then no further serializing until
/// the backoff has passed.
#[gtk::test]
fn gtk_ui_a_failing_autosave_backs_off_and_says_so_once() {
    let (_scratch, dir) = scratch("disk-fails");
    let window = Window::over(&dir);
    let viewer = window.viewer();
    window.edit();
    window.tick_and_settle();
    // The directory vanishes under the open entry, and a file takes its
    // place: every write now fails, even for root.
    fs::remove_dir_all(dir.root()).unwrap();
    fs::write(dir.root(), b"not a directory").unwrap();
    let start = Instant::now();

    window.edit();
    tick_at(viewer, start);
    settle_until(|| !is_writing(viewer));
    assert_eq!(viewer.status.text(), tr("recovery-autosave-unavailable"));
    let failed_attempts = attempts(viewer);

    viewer.status.set_text("");
    window.edit();
    tick_at(viewer, start + Duration::from_secs(30));
    settle_until(|| !is_writing(viewer));
    assert_eq!(
        attempts(viewer),
        failed_attempts,
        "backing off: the document is not even serialized"
    );

    tick_at(viewer, start + Duration::from_secs(60 * 60));
    settle_until(|| !is_writing(viewer));
    assert_eq!(attempts(viewer), failed_attempts + 1, "tried again later");
    assert_eq!(viewer.status.text(), "", "and said nothing twice");
}

/// The backoff runs on the clock the tick was given, not the wall clock: a
/// failure at a late tick holds the next attempt back from *that* tick.
#[gtk::test]
fn gtk_ui_the_backoff_is_measured_from_the_failing_tick() {
    let (_scratch, dir) = scratch("backoff-clock");
    let window = Window::over(&dir);
    let viewer = window.viewer();
    fs::create_dir_all(dir.root().parent().unwrap()).unwrap();
    fs::write(dir.root(), b"not a directory").unwrap();
    let late = Instant::now() + Duration::from_secs(2 * 60 * 60);

    window.edit();
    tick_at(viewer, late);
    settle_until(|| !is_writing(viewer));
    let failed_attempts = attempts(viewer);
    window.edit();
    tick_at(viewer, late + Duration::from_secs(30));
    settle_until(|| !is_writing(viewer));

    assert_eq!(attempts(viewer), failed_attempts, "still backing off");
}

/// A forget ends the document the failures were about: the next one is
/// written — and its failures reported — without waiting out the old streak.
#[gtk::test]
fn gtk_ui_a_forget_ends_the_failure_streak() {
    let (_scratch, dir) = scratch("backoff-forget");
    let window = Window::over(&dir);
    let viewer = window.viewer();
    fs::create_dir_all(dir.root().parent().unwrap()).unwrap();
    fs::write(dir.root(), b"not a directory").unwrap();
    let start = Instant::now();
    window.edit();
    tick_at(viewer, start);
    settle_until(|| !is_writing(viewer));
    let failed_attempts = attempts(viewer);

    super::forget(viewer);
    viewer.status.set_text("");
    window.edit();
    tick_at(viewer, start + Duration::from_secs(30));
    settle_until(|| !is_writing(viewer));

    assert_eq!(attempts(viewer), failed_attempts + 1, "tried right away");
    assert_eq!(
        viewer.status.text(),
        tr("recovery-autosave-unavailable"),
        "and the new streak is reported"
    );
}

/// The same notice when the directory cannot be opened at all.
#[gtk::test]
fn gtk_ui_an_unusable_recovery_directory_is_reported() {
    let (scratch, dir) = scratch("unusable");
    fs::create_dir_all(&scratch.0).unwrap();
    fs::write(dir.root(), b"not a directory").unwrap();
    let window = Window::over(&dir);
    window.edit();

    window.tick_and_settle();

    assert_eq!(
        window.viewer().status.text(),
        tr("recovery-autosave-unavailable")
    );
}

#[gtk::test]
fn gtk_ui_a_restored_document_still_knows_it_broke_a_signature() {
    let (_scratch, dir) = scratch("restored-signature");
    let meta = Meta {
        breaks_signatures: true,
        ..a_meta("signed.pdf")
    };
    crashed_entry(&dir, SAMPLE_PDF, &meta);
    let window = Window::over(&dir);

    restore(
        &window.built.window,
        window.viewer(),
        dir.orphans().remove(0),
        Vec::new(),
        None,
    );
    settle_until(|| {
        window.viewer().status.text() == tr("recovery-restored") && !is_writing(window.viewer())
    });

    assert!(
        window
            .viewer()
            .state
            .borrow()
            .session
            .as_ref()
            .is_some_and(|session| session.base_breaks_signatures),
        "Save must still ask before writing over the signed original"
    );
    let sidecar = fs::read_to_string(the_file_ending(dir.root(), ".meta")).unwrap();
    assert!(
        Meta::parse(&sidecar).is_some_and(|meta| meta.breaks_signatures),
        "the window's own entry says so too"
    );
}

/// Bytes the disk cannot hand over right now say nothing about whether they
/// are a document: the entry stays for the next launch.
#[gtk::test]
fn gtk_ui_an_entry_that_cannot_be_read_right_now_is_kept() {
    let (_scratch, dir) = scratch("transient");
    crashed_entry(&dir, SAMPLE_PDF, &a_meta("kept.pdf"));
    let mut orphans = dir.orphans();
    // A directory where the bytes were: reading fails, and not because of
    // what the bytes are.
    let pdf = the_file_ending(dir.root(), ".pdf");
    fs::remove_file(&pdf).unwrap();
    fs::create_dir(&pdf).unwrap();
    let window = Window::over(&dir);

    restore(
        &window.built.window,
        window.viewer(),
        orphans.remove(0),
        Vec::new(),
        None,
    );
    settle_until(|| window.viewer().status.text() == tr("recovery-restore-failed"));

    assert!(
        files_in(dir.root())
            .iter()
            .any(|name| name.ends_with(".meta")),
        "the entry is kept: {:?}",
        files_in(dir.root())
    );
}

/// Launched by "Open With" and asked to restore instead: if the restore
/// fails, the file the user asked for is opened after all.
#[gtk::test]
fn gtk_ui_a_failed_restore_opens_the_file_the_app_was_launched_with() {
    let (scratch, dir) = scratch("launch-file");
    crashed_entry(&dir, b"%PDF-1.7 and then nothing", &a_meta("broken.pdf"));
    let launched = scratch.0.join("launched.pdf");
    fs::write(&launched, SAMPLE_PDF).unwrap();
    let window = Window::over(&dir);

    restore(
        &window.built.window,
        window.viewer(),
        dir.orphans().remove(0),
        Vec::new(),
        Some(launched.clone()),
    );
    settle_until(|| {
        window
            .viewer()
            .state
            .borrow()
            .session
            .as_ref()
            .is_some_and(|session| session.origin_path.as_deref() == Some(launched.as_path()))
    });
}

/// The origin an entry records is the file the document came from.
#[test]
fn a_document_opened_from_a_file_knows_its_origin() {
    let (scratch, _) = scratch("origin");
    fs::create_dir_all(&scratch.0).unwrap();
    let path = scratch.0.join("sample.pdf");
    fs::write(&path, SAMPLE_PDF).unwrap();

    let opened = open_document(&DocumentSource::File(path.clone()), None).expect("it opens");
    let embedded =
        open_document(&DocumentSource::Embedded(SAMPLE_PDF), None).expect("the sample opens");

    assert_eq!(opened.origin_path, Some(path));
    assert_eq!(embedded.origin_path, None);
    let renderer = pdf_render::PdfiumRenderer::new();
    let _ = renderer.close_document(opened.document);
    let _ = renderer.close_document(embedded.document);
}
