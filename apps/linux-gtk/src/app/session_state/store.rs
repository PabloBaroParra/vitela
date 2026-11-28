//! The one file the remembered state lives in, and the only code that
//! touches it.
//!
//! `$XDG_STATE_HOME/vitela/session-state` — beside the crash-recovery
//! entries, under the directory `recovery::store` resolves. The state
//! directory rather than the configuration one because that is what the
//! XDG specification puts there: "the current state of the application,
//! that can be reused on a restart (view, layout, open files…)". Nothing in
//! it is a setting the user chose, and nothing breaks when it is deleted.
//!
//! Every way of failing here is survivable and none is reported to the
//! user: a file that cannot be read is no state (the defaults), and one
//! that cannot be written is a log line. Nothing waits on either.
//!
//! **Atomic.** A write goes to a sibling temporary file and is renamed over
//! the target, so a reader — this process at its next start, or another
//! window's merge — sees the old file or the new one, never half of one.
//!
//! **Two windows.** Two instances can share the file (a second profile, a
//! sandboxed and an unsandboxed build). Each writes through a temporary
//! file named for its own process, so neither can rename the other's
//! half-written bytes into place; and each write first takes in what the
//! file holds for every document it has not recorded itself
//! ([`State::merge_documents`]), so the last writer does not erase the
//! other's reading positions. The window size and the panels are simply the
//! last writer's — there is one of each to remember.
//!
//! **One window, two threads.** The window writes from a worker thread
//! while it is in use and from the main thread as it closes, and the two
//! can overlap. Each state it hands over carries a revision; writes take
//! turns ([`Store::save_revision`]) and one that arrives after a newer
//! revision is already on disk is dropped, so the file never goes back to
//! an older state.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use super::model::State;

const FILE_NAME: &str = "session-state";

/// The directory is this user's alone, like the recovery directory beside
/// it: the file names the documents they read.
const PRIVATE_DIR: u32 = 0o700;
const PRIVATE_FILE: u32 = 0o600;

/// A file larger than this is not this file. Read as nothing rather than
/// parsed line by line at every start.
const MAX_BYTES: u64 = 1024 * 1024;

/// Where the state is kept. Clones share the record of what was written.
#[derive(Clone, Debug)]
pub(crate) struct Store {
    file: PathBuf,
    /// The newest revision on disk, held for as long as a write takes.
    written: Arc<Mutex<u64>>,
}

impl Store {
    /// A store at `file` — a test's own, or the user's.
    pub(crate) fn new(file: PathBuf) -> Self {
        Self {
            file,
            written: Arc::default(),
        }
    }

    /// The record of what is on disk. A writer that panicked is not a
    /// reason to stop writing.
    fn turn(&self) -> MutexGuard<'_, u64> {
        self.written
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Holds the turn a write takes, as a write under way does.
    #[cfg(test)]
    pub(crate) fn hold_turn_for_test(&self) -> MutexGuard<'_, u64> {
        self.turn()
    }

    /// The newest revision written, once any write under way has finished.
    pub(crate) fn written_revision(&self) -> u64 {
        *self.turn()
    }

    /// [`Store::save`] for the window's own state at `revision`, in turn:
    /// waits for a write under way, and does nothing when a revision at
    /// least as new is already on disk.
    pub(crate) fn save_revision(&self, state: State, revision: u64) -> io::Result<()> {
        let mut written = self.turn();
        if revision <= *written {
            return Ok(());
        }
        self.save(state)?;
        *written = revision;
        Ok(())
    }

    /// The user's own, or `None` on a machine with no usable state
    /// directory — which then remembers nothing.
    pub(crate) fn in_user_state_dir() -> Option<Self> {
        Some(Self::new(
            crate::app::recovery::app_state_dir()?.join(FILE_NAME),
        ))
    }

    /// What the file holds. Missing, unreadable, oversized or damaged all
    /// read as less — down to the defaults — and never as an error.
    pub(crate) fn load(&self) -> State {
        let oversized = fs::metadata(&self.file).is_ok_and(|metadata| metadata.len() > MAX_BYTES);
        if oversized {
            return State::default();
        }
        // Lossy: a file that is not UTF-8 is not this file, and its lines
        // are skipped as any other unknown line is.
        match fs::read(&self.file) {
            Ok(bytes) => State::parse(&String::from_utf8_lossy(&bytes)),
            Err(_) => State::default(),
        }
    }

    /// Writes `state`, having first taken in what the file holds for the
    /// documents `state` did not record itself.
    pub(crate) fn save(&self, mut state: State) -> io::Result<()> {
        state.merge_documents(&self.load());
        self.write(state.serialize().as_bytes())
    }

    fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let folder = self
            .file
            .parent()
            .ok_or_else(|| io::Error::other("the state file has no folder"))?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(PRIVATE_DIR)
            .create(folder)?;
        let temporary = temporary_of(&self.file);
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(PRIVATE_FILE)
                .open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.file)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

/// The sibling a write goes through: hidden, and named for this process so
/// two windows never share one.
fn temporary_of(file: &Path) -> PathBuf {
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(FILE_NAME);
    file.with_file_name(format!(".{name}.{}.tmp", std::process::id()))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::model::{DocumentView, WindowGeometry};
    use super::*;
    use crate::app::layout::{ReadingPosition, Zoom};

    /// A scratch directory that removes itself when dropped, panics
    /// included. Never the user's own state directory.
    pub(crate) struct ScratchDir(pub(crate) PathBuf);

    impl ScratchDir {
        pub(crate) fn new() -> Self {
            static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "vitela-session-state-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("the scratch directory must be creatable");
            Self(path)
        }

        /// A store in a folder that does not exist yet, as on a first run.
        pub(crate) fn store(&self) -> Store {
            Store::new(self.0.join("state").join("vitela").join(FILE_NAME))
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sized(width: i32) -> State {
        let mut state = State::default();
        state.window = Some(WindowGeometry {
            width,
            height: 900,
            maximized: false,
        });
        state
    }

    fn reading(state: &mut State, name: &str, page_index: usize, seen: i64) {
        state.remember_document(
            Path::new(name),
            DocumentView {
                zoom: Zoom::FitWidth,
                reading: ReadingPosition {
                    page_index,
                    fraction: 0.0,
                },
                seen,
            },
        );
    }

    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .expect("read the folder")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_first_run_has_nothing_to_load() {
        let scratch = ScratchDir::new();

        assert!(scratch.store().load() == State::default());
    }

    #[test]
    fn what_is_saved_is_what_loads() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let mut state = sized(1280);
        reading(&mut state, "/home/user/a.pdf", 3, 10);

        store.save(state.clone()).expect("the state is written");

        assert!(store.load() == state);
    }

    /// The folder is made on the way, owner-only, and the file with it; no
    /// temporary file is left beside it.
    #[test]
    fn a_save_creates_a_private_folder_and_file_and_leaves_no_temporary() {
        let scratch = ScratchDir::new();
        let store = scratch.store();

        store.save(sized(1280)).expect("the state is written");

        let folder = store.file.parent().expect("a folder");
        assert_eq!(names(folder), [FILE_NAME]);
        let mode =
            |path: &Path| fs::metadata(path).expect("it exists").permissions().mode() & 0o777;
        assert_eq!(mode(folder), PRIVATE_DIR);
        assert_eq!(mode(&store.file), PRIVATE_FILE);
    }

    #[test]
    fn a_save_replaces_the_file_whole() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        store.save(sized(1280)).expect("the first write");

        store.save(sized(1600)).expect("the second write");

        assert_eq!(store.load().window.map(|window| window.width), Some(1600));
        assert_eq!(
            fs::read_to_string(&store.file).expect("the file"),
            sized(1600).serialize()
        );
    }

    #[test]
    fn a_damaged_file_loads_as_the_defaults_and_is_replaced_by_the_next_save() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        fs::create_dir_all(store.file.parent().expect("a folder")).expect("the folder");
        fs::write(&store.file, [0xff, 0xfe, 0x00, b'\n', 0x80, b'\t', 0xc3]).expect("garbage");

        assert!(store.load() == State::default());
        store.save(sized(1280)).expect("the state is written");

        assert!(store.load() == sized(1280));
    }

    #[test]
    fn a_file_far_too_large_to_be_this_file_is_not_read() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        fs::create_dir_all(store.file.parent().expect("a folder")).expect("the folder");
        let line = "window\t1280\t900\t0\n";
        let many = line.repeat(MAX_BYTES as usize / line.len() + 2);
        fs::write(&store.file, many).expect("an oversized file");

        assert!(store.load() == State::default());
    }

    /// Where the file should be is a folder: nothing to read, and the write
    /// reports its error instead of panicking — the caller logs it.
    #[test]
    fn a_path_that_cannot_be_a_file_fails_the_save_and_nothing_else() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        fs::create_dir_all(&store.file).expect("a folder where the file goes");

        assert!(store.load() == State::default());
        assert!(store.save(sized(1280)).is_err());

        let folder = store.file.parent().expect("a folder");
        assert_eq!(names(folder), [FILE_NAME], "no temporary is left behind");
    }

    #[test]
    fn a_folder_that_cannot_be_written_fails_the_save_and_nothing_else() {
        let scratch = ScratchDir::new();
        let probe = scratch.0.join("probe");
        fs::write(&probe, b"").expect("the probe");
        fs::set_permissions(&scratch.0, fs::Permissions::from_mode(0o500)).expect("lock it");
        if fs::write(scratch.0.join("another"), b"").is_ok() {
            eprintln!(
                "SKIPPED a_folder_that_cannot_be_written: this process ignores permission \
                 bits (running as root?). Nothing was asserted."
            );
            return;
        }

        assert!(scratch.store().save(sized(1280)).is_err());
        assert!(scratch.store().load() == State::default());
    }

    /// Two windows, each with its own idea of the file: the second to
    /// write keeps the documents the first one wrote.
    #[test]
    fn a_second_window_s_save_keeps_the_first_window_s_documents() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let mut first = sized(1280);
        reading(&mut first, "/home/user/first.pdf", 1, 10);
        reading(&mut first, "/home/user/shared.pdf", 2, 20);
        let mut second = sized(1600);
        reading(&mut second, "/home/user/second.pdf", 3, 30);
        reading(&mut second, "/home/user/shared.pdf", 9, 40);

        store.save(first).expect("the first window writes");
        store.save(second).expect("the second window writes");

        let read = store.load();
        assert_eq!(read.document_count(), 3);
        assert_eq!(read.window.map(|window| window.width), Some(1600));
        let page = |name: &str| {
            read.document(Path::new(name))
                .map(|view| view.reading.page_index)
        };
        assert_eq!(page("/home/user/first.pdf"), Some(1));
        assert_eq!(page("/home/user/second.pdf"), Some(3));
        assert_eq!(page("/home/user/shared.pdf"), Some(9), "the newer record");
    }

    /// The file holds a record of the document dated later than this
    /// window's clock — the clock went back, or the record came from a
    /// machine whose clock ran ahead. The window's own record is written
    /// all the same, every time.
    #[test]
    fn a_window_s_own_record_is_written_over_a_later_dated_one() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let mut earlier = State::default();
        reading(&mut earlier, "/home/user/a.pdf", 3, 5_000);
        store
            .save(earlier)
            .expect("the file as another run left it");
        let mut mine = store.load();

        for page_index in [7, 8] {
            reading(&mut mine, "/home/user/a.pdf", page_index, 100);
            store.save(mine.clone()).expect("this window writes");

            let read = store.load();
            assert_eq!(
                read.document(Path::new("/home/user/a.pdf"))
                    .map(|view| view.reading.page_index),
                Some(page_index)
            );
        }
    }

    /// The worker's write and the closing window's can cross: the older
    /// state arriving last must not replace the newer one.
    #[test]
    fn an_older_revision_arriving_late_does_not_replace_a_newer_one() {
        let scratch = ScratchDir::new();
        let store = scratch.store();

        store
            .save_revision(sized(1600), 2)
            .expect("the newer state");
        store
            .clone()
            .save_revision(sized(1280), 1)
            .expect("the older state is dropped, not an error");

        assert_eq!(store.load().window.map(|window| window.width), Some(1600));
        assert_eq!(store.written_revision(), 2);
    }

    /// A revision counts as written only once its write succeeded, so a
    /// later attempt is not skipped as already done.
    #[test]
    fn a_failed_write_leaves_its_revision_unwritten() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        fs::create_dir_all(&store.file).expect("a folder where the file goes");

        assert!(store.save_revision(sized(1280), 1).is_err());

        assert_eq!(store.written_revision(), 0);
    }

    #[test]
    fn the_temporary_file_is_a_hidden_sibling_named_for_this_process() {
        assert_eq!(
            temporary_of(Path::new("/home/user/.local/state/vitela/session-state")),
            PathBuf::from(format!(
                "/home/user/.local/state/vitela/.session-state.{}.tmp",
                std::process::id()
            ))
        );
    }
}
