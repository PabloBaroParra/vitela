//! The recovery directory on disk — no GTK, no pdfium, only files.
//!
//! One entry per tab that has unsaved work, named by an [`EntryKey`] the
//! tab's autosave mints once. An entry is three files beside each other:
//!
//! - `<key>.pdf` — the document's bytes, pending edits applied.
//! - `<key>.meta` — a small text sidecar ([`Meta`]): when, what, and from where.
//! - `<key>.lock` — held with an exclusive `flock` by the process that owns
//!   the entry, for as long as it lives.
//!
//! The lock is what tells a crash from a running window. A process that dies
//! — however it dies — drops its locks with it, so on the next launch an
//! entry whose lock can be taken is one nobody is writing any more: an
//! orphan, offered for recovery. An entry whose lock is held belongs to a
//! window still open (another instance, or a test), and is left alone. A
//! PID file could not tell the two apart once the PID is reused; the kernel
//! releasing a dead process's locks can.
//!
//! The directory is `0700` and every file `0600`: the bytes are the user's
//! document. Both data files are written to a sibling and renamed into place,
//! the sidecar last, so an entry is only ever complete or absent — a crash in
//! the middle of a write leaves the previous complete entry, never half a PDF
//! paired with a sidecar that vouches for it.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::glib;

use super::meta::Meta;

/// Under the user's state directory, beside nothing else of Vitela's yet.
const APP_DIR: &str = "vitela";
const RECOVERY_DIR: &str = "recovery";

const PDF_EXTENSION: &str = "pdf";
const META_EXTENSION: &str = "meta";
const LOCK_EXTENSION: &str = "lock";

const PRIVATE_DIR: u32 = 0o700;
const PRIVATE_FILE: u32 = 0o600;

/// The directory recovery entries live in.
#[derive(Clone, Debug)]
pub(crate) struct RecoveryDir {
    root: PathBuf,
}

/// Names one tab's entry: process id, the moment the key was minted, and a
/// per-process sequence number, so neither two tabs of one process nor a
/// later process that reuses the PID can land on the same files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EntryKey(String);

/// The paths of one entry. `Send`, so a worker thread can write them.
#[derive(Clone, Debug)]
pub(crate) struct EntryPaths {
    pdf: PathBuf,
    meta: PathBuf,
    lock: PathBuf,
}

/// An entry this process owns and writes. Holds its lock until dropped.
///
/// Dropping a slot without [`Slot::clear`] releases the lock and leaves the
/// files, which is exactly what a crash does — and what makes them an orphan
/// for the next launch.
#[derive(Debug)]
pub(crate) struct Slot {
    paths: EntryPaths,
    _lock: File,
}

/// An entry left behind by a process that is gone, claimed by this one: its
/// lock is held, so no other instance can offer or delete it meanwhile.
#[derive(Debug)]
pub(crate) struct Orphan {
    paths: EntryPaths,
    meta: Meta,
    _lock: File,
}

impl RecoveryDir {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// `$XDG_STATE_HOME/vitela/recovery`, or `~/.local/state/vitela/recovery`
    /// when the variable is unset. `None` when neither names an absolute
    /// directory — recovery written relative to the launch directory would
    /// never be found again.
    ///
    /// Resolved here rather than through `glib::user_state_dir`, which the
    /// `glib` this crate builds against only offers behind its GLib 2.72
    /// feature.
    pub(crate) fn in_user_state_dir() -> Option<Self> {
        Some(Self::new(app_state_dir()?.join(RECOVERY_DIR)))
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Opens and locks the entry `key` names, creating the directory first.
    pub(crate) fn open_slot(&self, key: &EntryKey) -> io::Result<Slot> {
        self.ensure()?;
        let paths = self.paths(&key.0);
        let lock = open_private(&paths.lock)?;
        // A fresh key is never held by anyone else; failing here means the
        // filesystem cannot lock at all, and then no entry could ever be told
        // apart from a live one — better to write none.
        lock.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => io::Error::other("recovery entry is already held"),
            TryLockError::Error(error) => error,
        })?;
        Ok(Slot { paths, _lock: lock })
    }

    /// Every entry no live process holds, newest first, each claimed.
    ///
    /// Entries that cannot be recovered — a sidecar that does not parse, bytes
    /// that are missing, a write a crash cut short — are deleted here with a
    /// log line rather than offered or left to pile up.
    pub(crate) fn orphans(&self) -> Vec<Orphan> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let keys: BTreeSet<String> = entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter_map(|name| key_of(&name).map(str::to_owned))
            .collect();
        let mut orphans: Vec<Orphan> = keys.iter().filter_map(|key| self.claim(key)).collect();
        orphans.sort_by_key(|orphan| std::cmp::Reverse(orphan.meta.saved_at));
        orphans
    }

    /// Claims the entry `key` names if its owner is gone, cleaning it up
    /// instead when there is nothing in it worth offering.
    fn claim(&self, key: &str) -> Option<Orphan> {
        let paths = self.paths(key);
        let lock = match open_private(&paths.lock) {
            Ok(lock) => lock,
            Err(error) => {
                eprintln!("recovery: cannot open {}: {error}", paths.lock.display());
                return None;
            }
        };
        match lock.try_lock() {
            Ok(()) => {}
            // Held: a window that is still open owns this entry.
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Error(error)) => {
                eprintln!("recovery: cannot lock {}: {error}", paths.lock.display());
                return None;
            }
        }
        self.remove_temporaries(key);
        let meta = fs::read_to_string(&paths.meta)
            .ok()
            .and_then(|text| Meta::parse(&text));
        match meta {
            Some(meta) if paths.pdf.is_file() => Some(Orphan {
                paths,
                meta,
                _lock: lock,
            }),
            _ => {
                eprintln!(
                    "recovery: discarding incomplete or unreadable entry {}",
                    self.root.join(key).display()
                );
                paths.remove_all();
                None
            }
        }
    }

    fn paths(&self, key: &str) -> EntryPaths {
        let file = |extension: &str| self.root.join(format!("{key}.{extension}"));
        EntryPaths {
            pdf: file(PDF_EXTENSION),
            meta: file(META_EXTENSION),
            lock: file(LOCK_EXTENSION),
        }
    }

    fn remove_temporaries(&self, key: &str) {
        for extension in [PDF_EXTENSION, META_EXTENSION] {
            let _ = fs::remove_file(temporary_of(&self.root.join(format!("{key}.{extension}"))));
        }
    }

    /// Creates the directory owner-only, and tightens it if it already
    /// existed looser — it is about to hold the user's documents.
    fn ensure(&self) -> io::Result<()> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(PRIVATE_DIR)
            .create(&self.root)?;
        fs::set_permissions(&self.root, fs::Permissions::from_mode(PRIVATE_DIR))
    }
}

/// This application's own directory under the user's state directory:
/// `$XDG_STATE_HOME/vitela`, or `~/.local/state/vitela`. `None` when neither
/// names an absolute directory. Where the recovery entries live, and the
/// remembered window state beside them (`session_state::store`).
pub(crate) fn app_state_dir() -> Option<PathBuf> {
    let state = state_home(std::env::var_os("XDG_STATE_HOME"), glib::home_dir())?;
    Some(state.join(APP_DIR))
}

/// The XDG state directory: `$XDG_STATE_HOME` when it is an absolute path —
/// the specification says a relative one is to be ignored — and
/// `$HOME/.local/state` otherwise.
fn state_home(xdg_state_home: Option<OsString>, home: PathBuf) -> Option<PathBuf> {
    xdg_state_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.is_absolute()
                .then(|| home.join(".local").join("state"))
        })
}

/// The entry key a file in the directory belongs to: the stem of `<key>.pdf`,
/// `<key>.meta`, `<key>.lock`, or of a temporary `.<key>.pdf.tmp`. `None` for
/// anything else, which the scan leaves alone.
fn key_of(name: &str) -> Option<&str> {
    let name = match name.strip_prefix('.') {
        Some(hidden) => hidden.strip_suffix(".tmp")?,
        None => name,
    };
    let (key, extension) = name.rsplit_once('.')?;
    let ours = [PDF_EXTENSION, META_EXTENSION, LOCK_EXTENSION].contains(&extension);
    let shaped = !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-');
    (ours && shaped).then_some(key)
}

/// The sibling a file is written to before it is renamed into place.
fn temporary_of(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.tmp"))
}

fn open_private(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(PRIVATE_FILE)
        .open(path)
}

/// Writes `bytes` to `path` through a private sibling and a rename.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = temporary_of(path);
    // A sibling left by a crash may carry other permissions, and `mode` only
    // applies to a file being created — so start from none.
    let _ = fs::remove_file(&temporary);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(PRIVATE_FILE)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

impl EntryKey {
    /// A key no other window, in this or any other process, holds.
    pub(crate) fn fresh() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        Self(format!(
            "{}-{nanos}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }
}

impl EntryPaths {
    /// Replaces the entry's bytes and sidecar, each atomically, sidecar last.
    pub(crate) fn write(&self, bytes: &[u8], meta: &Meta) -> io::Result<()> {
        write_atomically(&self.pdf, bytes)?;
        write_atomically(&self.meta, meta.encode().as_bytes())
    }

    /// Deletes the bytes and the sidecar, keeping the lock file — for a
    /// write that finished after its entry was already forgotten.
    pub(crate) fn remove_data(&self) {
        // Sidecar first: without it the entry is never offered, even if
        // removing the bytes then fails.
        let _ = fs::remove_file(&self.meta);
        let _ = fs::remove_file(&self.pdf);
    }

    fn remove_all(&self) {
        self.remove_data();
        let _ = fs::remove_file(&self.lock);
    }
}

impl Slot {
    pub(crate) fn paths(&self) -> &EntryPaths {
        &self.paths
    }

    /// Deletes the entry: the document was saved, closed or discarded.
    pub(crate) fn clear(self) {
        self.paths.remove_all();
    }
}

impl Orphan {
    pub(crate) fn meta(&self) -> &Meta {
        &self.meta
    }

    /// The recovered bytes, refused as [`io::ErrorKind::InvalidData`] when
    /// they do not even start like a PDF.
    pub(crate) fn read_pdf(&self) -> io::Result<Vec<u8>> {
        let bytes = fs::read(&self.paths.pdf)?;
        if !bytes.starts_with(b"%PDF-") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the recovered file is not a PDF",
            ));
        }
        Ok(bytes)
    }

    /// Deletes the entry: restored and safely re-recorded, or discarded.
    pub(crate) fn discard(self) {
        self.paths.remove_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory outside the workspace, removed on drop even when the test
    /// panics — `0700` directories left under `target/` break CI's cache step.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> (Scratch, RecoveryDir) {
        let base = std::env::temp_dir().join(format!(
            "vitela-recovery-test-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("the clock is after the epoch")
                .as_nanos()
        ));
        let root = base.join("vitela").join("recovery");
        (Scratch(base), RecoveryDir::new(root))
    }

    fn a_meta(saved_at: i64) -> Meta {
        Meta {
            saved_at,
            name: "quote.pdf".to_owned(),
            origin: Some(PathBuf::from("/home/someone/quote.pdf")),
            breaks_signatures: false,
        }
    }

    const BYTES: &[u8] = b"%PDF-1.7\nnot really a document\n%%EOF\n";

    fn mode(path: &Path) -> u32 {
        fs::metadata(path)
            .expect("the path exists")
            .permissions()
            .mode()
            & 0o777
    }

    fn files_in(root: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(root)
            .expect("the directory exists")
            .map(|entry| entry.expect("an entry").file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_state_home_honours_an_absolute_xdg_variable() {
        assert_eq!(
            state_home(Some("/xdg/state".into()), PathBuf::from("/home/me")),
            Some(PathBuf::from("/xdg/state"))
        );
    }

    #[test]
    fn the_state_home_falls_back_to_the_home_directory() {
        let expected = Some(PathBuf::from("/home/me/.local/state"));
        assert_eq!(state_home(None, PathBuf::from("/home/me")), expected);
        // The XDG specification: a relative value is invalid and ignored.
        assert_eq!(
            state_home(Some("relative/state".into()), PathBuf::from("/home/me")),
            expected
        );
    }

    #[test]
    fn no_absolute_directory_means_no_recovery() {
        assert_eq!(state_home(None, PathBuf::from("")), None);
    }

    #[test]
    fn a_written_entry_is_private_to_the_user() {
        let (_scratch, dir) = scratch("private");
        let slot = dir.open_slot(&EntryKey::fresh()).expect("the slot opens");

        slot.paths()
            .write(BYTES, &a_meta(1))
            .expect("the entry writes");

        assert_eq!(mode(&dir.root), 0o700);
        assert_eq!(mode(&slot.paths().pdf), 0o600);
        assert_eq!(mode(&slot.paths().meta), 0o600);
        assert_eq!(mode(&slot.paths().lock), 0o600);
    }

    #[test]
    fn an_existing_directory_is_tightened_to_the_user() {
        let (_scratch, dir) = scratch("tighten");
        fs::create_dir_all(&dir.root).expect("the directory exists");
        fs::set_permissions(&dir.root, fs::Permissions::from_mode(0o755)).expect("loosened");

        dir.open_slot(&EntryKey::fresh()).expect("the slot opens");

        assert_eq!(mode(&dir.root), 0o700);
    }

    #[test]
    fn a_write_leaves_no_temporary_files_behind() {
        let (_scratch, dir) = scratch("atomic");
        let key = EntryKey::fresh();
        let slot = dir.open_slot(&key).expect("the slot opens");

        slot.paths().write(BYTES, &a_meta(1)).expect("first write");
        slot.paths()
            .write(b"%PDF-2.0\n", &a_meta(2))
            .expect("second write");

        assert_eq!(
            files_in(&dir.root),
            [
                format!("{}.lock", key.0),
                format!("{}.meta", key.0),
                format!("{}.pdf", key.0)
            ]
        );
        assert_eq!(fs::read(&slot.paths().pdf).unwrap(), b"%PDF-2.0\n");
    }

    #[test]
    fn an_entry_whose_owner_is_alive_is_not_an_orphan() {
        let (_scratch, dir) = scratch("alive");
        let slot = dir.open_slot(&EntryKey::fresh()).expect("the slot opens");
        slot.paths()
            .write(BYTES, &a_meta(1))
            .expect("the entry writes");

        assert!(dir.orphans().is_empty(), "a held lock means a live window");
        assert!(slot.paths().pdf.exists(), "and its files are left alone");
    }

    #[test]
    fn an_entry_left_by_a_dead_owner_is_offered_with_its_bytes_and_sidecar() {
        let (_scratch, dir) = scratch("orphan");
        let meta = Meta {
            saved_at: 1_700_000_000,
            // Both values are written by the user, not by us: a name or a
            // path may hold a tab or a newline, and must come back whole.
            name: "line\none\ttab ✓.pdf".to_owned(),
            origin: Some(PathBuf::from("/tmp/odd\nname\t.pdf")),
            breaks_signatures: false,
        };
        {
            let slot = dir.open_slot(&EntryKey::fresh()).expect("the slot opens");
            slot.paths().write(BYTES, &meta).expect("the entry writes");
            // Dropped without `clear`: what a crash does to it.
        }

        let orphans = dir.orphans();

        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].meta(), &meta);
        assert_eq!(orphans[0].read_pdf().expect("the bytes read"), BYTES);
    }

    #[test]
    fn a_document_without_an_origin_round_trips_as_none() {
        let (_scratch, dir) = scratch("no-origin");
        let meta = Meta {
            saved_at: 5,
            name: "Untitled document".to_owned(),
            origin: None,
            breaks_signatures: false,
        };
        drop({
            let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
            slot.paths().write(BYTES, &meta).unwrap();
            slot
        });

        assert_eq!(dir.orphans()[0].meta(), &meta);
    }

    #[test]
    fn a_claimed_orphan_is_not_offered_twice() {
        let (_scratch, dir) = scratch("claimed");
        drop({
            let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
            slot.paths().write(BYTES, &a_meta(1)).unwrap();
            slot
        });

        let first = dir.orphans();
        let second = dir.orphans();

        assert_eq!(first.len(), 1);
        assert!(second.is_empty(), "the first claim still holds its lock");
    }

    #[test]
    fn orphans_are_offered_newest_first() {
        let (_scratch, dir) = scratch("order");
        for saved_at in [10, 30, 20] {
            let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
            slot.paths().write(BYTES, &a_meta(saved_at)).unwrap();
        }

        let order: Vec<_> = dir.orphans().iter().map(|o| o.meta().saved_at).collect();

        assert_eq!(order, [30, 20, 10]);
    }

    #[test]
    fn two_windows_never_share_an_entry() {
        let (_scratch, dir) = scratch("two-windows");
        let first = dir.open_slot(&EntryKey::fresh()).unwrap();
        let second = dir.open_slot(&EntryKey::fresh()).unwrap();

        first.paths().write(b"%PDF-first", &a_meta(1)).unwrap();
        second.paths().write(b"%PDF-second", &a_meta(2)).unwrap();

        assert_ne!(first.paths().pdf, second.paths().pdf);
        assert_eq!(fs::read(&first.paths().pdf).unwrap(), b"%PDF-first");
        assert_eq!(fs::read(&second.paths().pdf).unwrap(), b"%PDF-second");
    }

    #[test]
    fn clearing_a_slot_deletes_its_whole_entry() {
        let (_scratch, dir) = scratch("clear");
        let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
        slot.paths().write(BYTES, &a_meta(1)).unwrap();

        slot.clear();

        assert!(files_in(&dir.root).is_empty());
        assert!(dir.orphans().is_empty());
    }

    #[test]
    fn removing_the_data_keeps_the_entry_from_being_offered() {
        let (_scratch, dir) = scratch("remove-data");
        let paths = {
            let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
            slot.paths().write(BYTES, &a_meta(1)).unwrap();
            slot.paths().clone()
        };

        paths.remove_data();

        assert!(dir.orphans().is_empty());
    }

    #[test]
    fn discarding_an_orphan_deletes_its_whole_entry() {
        let (_scratch, dir) = scratch("discard");
        drop({
            let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
            slot.paths().write(BYTES, &a_meta(1)).unwrap();
            slot
        });

        dir.orphans().pop().expect("one orphan").discard();

        assert!(files_in(&dir.root).is_empty());
    }

    #[test]
    fn an_unparseable_sidecar_is_discarded_instead_of_offered() {
        let (_scratch, dir) = scratch("corrupt-meta");
        let paths = {
            let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
            slot.paths().write(BYTES, &a_meta(1)).unwrap();
            slot.paths().clone()
        };
        fs::write(&paths.meta, "something else entirely\n").unwrap();

        assert!(dir.orphans().is_empty());
        assert!(files_in(&dir.root).is_empty(), "the debris is cleaned up");
    }

    /// A sidecar line this version cannot read is skipped, not taken as a
    /// reason to delete the user's only copy of their work.
    #[test]
    fn a_sidecar_with_an_unreadable_line_is_still_offered() {
        let (_scratch, dir) = scratch("odd-line");
        let paths = {
            let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
            slot.paths().write(BYTES, &a_meta(1)).unwrap();
            slot.paths().clone()
        };
        let mut text = fs::read_to_string(&paths.meta).unwrap();
        text.push_str(
            "a line from some later version
",
        );
        fs::write(&paths.meta, text).unwrap();

        let orphans = dir.orphans();

        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].meta(), &a_meta(1));
    }

    #[test]
    fn bytes_missing_beside_their_sidecar_are_discarded() {
        let (_scratch, dir) = scratch("missing-pdf");
        let paths = {
            let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
            slot.paths().write(BYTES, &a_meta(1)).unwrap();
            slot.paths().clone()
        };
        fs::remove_file(&paths.pdf).unwrap();

        assert!(dir.orphans().is_empty());
        assert!(files_in(&dir.root).is_empty());
    }

    #[test]
    fn a_first_write_cut_short_by_a_crash_is_cleaned_up() {
        let (_scratch, dir) = scratch("cut-short");
        let key = EntryKey::fresh();
        drop(dir.open_slot(&key).unwrap());
        // The bytes' temporary sibling, as a crash mid-write would leave it.
        fs::write(dir.root.join(format!(".{}.pdf.tmp", key.0)), b"%PDF-half").unwrap();

        assert!(dir.orphans().is_empty());
        assert!(files_in(&dir.root).is_empty());
    }

    #[test]
    fn bytes_that_are_not_a_pdf_are_refused_on_read() {
        let (_scratch, dir) = scratch("not-pdf");
        drop({
            let slot = dir.open_slot(&EntryKey::fresh()).unwrap();
            slot.paths().write(b"garbage", &a_meta(1)).unwrap();
            slot
        });

        let orphan = dir.orphans().pop().expect("one orphan");

        assert_eq!(
            orphan.read_pdf().expect_err("not a PDF").kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_missing_directory_has_no_orphans() {
        let (_scratch, dir) = scratch("missing-dir");

        assert!(dir.orphans().is_empty());
    }

    #[test]
    fn unrelated_files_in_the_directory_are_left_alone() {
        let (_scratch, dir) = scratch("unrelated");
        fs::create_dir_all(&dir.root).unwrap();
        fs::write(dir.root.join("README"), b"hands off").unwrap();

        assert!(dir.orphans().is_empty());
        assert_eq!(files_in(&dir.root), ["README"]);
    }
}
