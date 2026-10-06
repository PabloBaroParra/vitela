//! Where the remembered signature lives: one PNG in the user's data directory.
//!
//! Behind a small trait so the flow that uses it is testable without touching
//! the real home directory, and so a machine with no usable data directory
//! simply remembers nothing instead of failing.
//!
//! Every method touches storage: call them off the main thread. The trait is
//! `Send + Sync` for exactly that reason.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use gtk::glib;

/// The directory under the user's data directory (`$XDG_DATA_HOME`, usually
/// `~/.local/share`) this application keeps its own files in.
const APP_DIR: &str = "vitela";

/// The file name of the remembered signature.
const FILE_NAME: &str = "signature.png";

/// The one drawn signature the user asked to be remembered, as the PNG the pad
/// made.
pub(crate) trait SignatureStore: Send + Sync {
    /// The remembered PNG, or `None` when there is none or it cannot be read.
    fn load(&self) -> Option<Vec<u8>>;

    /// Replaces the remembered PNG. `false` when it could not be written; the
    /// previous one is then still there.
    fn save(&self, png: &[u8]) -> bool;

    /// Forgets the remembered PNG. Forgetting nothing is not an error.
    fn delete(&self);
}

/// Remembers nothing — for a machine with no usable data directory.
pub(crate) struct NoSignatureStore;

impl SignatureStore for NoSignatureStore {
    fn load(&self) -> Option<Vec<u8>> {
        None
    }

    fn save(&self, _png: &[u8]) -> bool {
        false
    }

    fn delete(&self) {}
}

/// The signature kept in one file.
///
/// Written to a sibling and renamed over the target, so a crash mid-write
/// leaves the old signature, not half of the new one. Created readable by its
/// owner only: it is a picture of the user's signature, not something other
/// accounts on the machine have a use for.
pub(crate) struct FileSignatureStore {
    file: PathBuf,
}

impl FileSignatureStore {
    pub(crate) fn new(file: PathBuf) -> Self {
        Self { file }
    }

    /// `$XDG_DATA_HOME/vitela/signature.png`, which GLib resolves
    /// (`~/.local/share` when the variable is unset).
    fn in_user_data_dir() -> Option<Self> {
        let directory = glib::user_data_dir();
        // GLib falls back to a relative path when there is no home to name; a
        // signature written relative to wherever the app was launched from
        // would be neither remembered nor private.
        directory
            .is_absolute()
            .then(|| Self::new(directory.join(APP_DIR).join(FILE_NAME)))
    }

    fn partial(&self) -> PathBuf {
        let mut name = self.file.clone().into_os_string();
        name.push(".partial");
        PathBuf::from(name)
    }

    fn write_partial(&self, png: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut partial = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(self.partial())?;
        partial.write_all(png)?;
        // Durable before the rename publishes it: a rename of an unsynced file
        // can survive a power cut as an empty one.
        partial.sync_all()
    }
}

impl SignatureStore for FileSignatureStore {
    fn load(&self) -> Option<Vec<u8>> {
        std::fs::read(&self.file)
            .ok()
            .filter(|bytes| !bytes.is_empty())
    }

    fn save(&self, png: &[u8]) -> bool {
        let written = self
            .write_partial(png)
            .and_then(|()| std::fs::rename(self.partial(), &self.file));
        if written.is_err() {
            let _ = std::fs::remove_file(self.partial());
        }
        written.is_ok()
    }

    fn delete(&self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

/// The store the running application uses.
pub(crate) fn default_store() -> Arc<dyn SignatureStore> {
    match FileSignatureStore::in_user_data_dir() {
        Some(store) => Arc::new(store),
        None => Arc::new(NoSignatureStore),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// A scratch directory that removes itself when dropped — also when the
    /// test panics, which a trailing cleanup call would not survive.
    pub(crate) struct ScratchDir(pub(crate) PathBuf);

    impl ScratchDir {
        pub(crate) fn new() -> Self {
            static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "vitela-signature-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("the scratch directory must be creatable");
            Self(path)
        }

        pub(crate) fn file(&self) -> PathBuf {
            self.0.join(FILE_NAME)
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A store in memory, for the flow's tests: remembers what it is given and
    /// counts what was asked of it.
    #[derive(Default)]
    pub(crate) struct MemorySignatureStore {
        pub(crate) png: Mutex<Option<Vec<u8>>>,
    }

    impl MemorySignatureStore {
        pub(crate) fn holding(png: &[u8]) -> Self {
            Self {
                png: Mutex::new(Some(png.to_vec())),
            }
        }

        pub(crate) fn current(&self) -> Option<Vec<u8>> {
            self.png.lock().expect("the store lock").clone()
        }
    }

    impl SignatureStore for MemorySignatureStore {
        fn load(&self) -> Option<Vec<u8>> {
            self.current()
        }

        fn save(&self, png: &[u8]) -> bool {
            *self.png.lock().expect("the store lock") = Some(png.to_vec());
            true
        }

        fn delete(&self) {
            *self.png.lock().expect("the store lock") = None;
        }
    }

    #[test]
    fn nothing_is_remembered_at_first() {
        let scratch = ScratchDir::new();

        assert_eq!(FileSignatureStore::new(scratch.file()).load(), None);
    }

    #[test]
    fn a_saved_signature_loads_back_byte_for_byte() {
        let scratch = ScratchDir::new();

        assert!(FileSignatureStore::new(scratch.file()).save(&[1, 2, 3]));

        assert_eq!(
            FileSignatureStore::new(scratch.file()).load(),
            Some(vec![1, 2, 3])
        );
    }

    #[test]
    fn saving_creates_the_missing_directory() {
        let scratch = ScratchDir::new();
        let nested = scratch.0.join("not").join("yet").join(FILE_NAME);

        assert!(FileSignatureStore::new(nested.clone()).save(&[7]));

        assert_eq!(FileSignatureStore::new(nested).load(), Some(vec![7]));
    }

    #[test]
    fn a_new_signature_replaces_the_old_one_and_leaves_no_partial_file() {
        let scratch = ScratchDir::new();
        let store = FileSignatureStore::new(scratch.file());
        store.save(&[1, 2, 3]);

        store.save(&[9]);

        assert_eq!(store.load(), Some(vec![9]));
        let leftovers: Vec<_> = std::fs::read_dir(&scratch.0)
            .expect("the scratch directory is readable")
            .map(|entry| entry.expect("a directory entry").file_name())
            .collect();
        assert_eq!(leftovers, [std::ffi::OsString::from(FILE_NAME)]);
    }

    #[test]
    fn deleting_forgets_the_signature() {
        let scratch = ScratchDir::new();
        let store = FileSignatureStore::new(scratch.file());
        store.save(&[1]);

        store.delete();

        assert_eq!(store.load(), None);
        assert!(!scratch.file().exists());
    }

    #[test]
    fn forgetting_when_nothing_is_remembered_is_not_an_error() {
        let scratch = ScratchDir::new();

        FileSignatureStore::new(scratch.file()).delete();
    }

    #[test]
    fn an_empty_file_is_not_a_signature() {
        let scratch = ScratchDir::new();
        std::fs::write(scratch.file(), b"").expect("the file is writable");

        assert_eq!(FileSignatureStore::new(scratch.file()).load(), None);
    }

    #[test]
    fn a_failed_save_keeps_the_previous_signature_and_cleans_up() {
        let scratch = ScratchDir::new();
        let store = FileSignatureStore::new(scratch.file());
        store.save(&[1, 2, 3]);
        // A directory squatting on the partial file's name makes the write fail.
        std::fs::create_dir(store.partial()).expect("the blocker is creatable");

        assert!(!store.save(&[9]));

        assert_eq!(store.load(), Some(vec![1, 2, 3]));
    }

    #[cfg(unix)]
    #[test]
    fn the_signature_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = ScratchDir::new();
        FileSignatureStore::new(scratch.file()).save(&[1]);

        let mode = std::fs::metadata(scratch.file())
            .expect("the file exists")
            .permissions()
            .mode();

        assert_eq!(mode & 0o077, 0);
    }

    #[test]
    fn a_store_that_remembers_nothing_never_loads_or_saves() {
        let store = NoSignatureStore;

        assert!(!store.save(&[1]));
        assert_eq!(store.load(), None);
        store.delete();
    }
}
