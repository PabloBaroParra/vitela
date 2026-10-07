//! The PDFs the user removed from Home's Recent list.
//!
//! Recent itself is the desktop's shared `GtkRecentManager` store (see
//! [`super::recents`]). Removing a card must not edit that store: the file
//! manager and every other GTK application read it too, and "take this off
//! Vitela's list" is not "erase it from my desktop's history". So this module
//! keeps Vitela's own note of what it hides, and when.
//!
//! A document stays hidden only until it is opened again: an entry the store
//! says was used *after* the moment it was hidden shows again. Reopening a
//! removed PDF — here or in any other application — brings it back; a restart,
//! a store refresh or another application's activity does not.
//!
//! One line per document, `unix-seconds TAB uri`. The URI rather than the
//! path because a Linux path may contain a tab or a newline and a `file://`
//! URI escapes both. Written to a sibling and renamed over the target, owner
//! readable only — it is a list of the user's files. Every method touches
//! storage; the card's remove action calls them off the main thread.

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use gtk::prelude::FileExt;
use gtk::{gio, glib};

/// The directory under the user's data directory this application keeps its
/// own files in — the same one the remembered signature uses.
const APP_DIR: &str = "vitela";

const FILE_NAME: &str = "hidden-recents";

/// Old entries beyond this are dropped: a hidden list must not grow forever.
pub(crate) const CAPACITY: usize = 256;

/// Each hidden document's path, and when it was hidden (Unix seconds).
pub(crate) type Hidden = HashMap<PathBuf, i64>;

/// Whether `path`, last used at `used` (Unix seconds), is still hidden.
///
/// `>=`: the store only has second precision, so an entry used in the same
/// second it was hidden counts as hidden. Vitela's own reopen inside that
/// second is covered by [`HiddenRecents::unhide`].
pub(crate) fn hides(hidden: &Hidden, path: &Path, used: i64) -> bool {
    hidden.get(path).is_some_and(|&hidden_at| hidden_at >= used)
}

/// The list's file, or nothing at all on a machine with no usable data
/// directory — which then hides nothing and cannot hide anything.
#[derive(Clone, Debug)]
pub(crate) struct HiddenRecents {
    file: Option<PathBuf>,
}

impl HiddenRecents {
    #[cfg(test)]
    pub(crate) fn new(file: PathBuf) -> Self {
        Self { file: Some(file) }
    }

    /// `$XDG_DATA_HOME/vitela/hidden-recents`, which GLib resolves
    /// (`~/.local/share` when the variable is unset).
    pub(crate) fn in_user_data_dir() -> Self {
        let directory = glib::user_data_dir();
        // GLib falls back to a relative path when there is no home to name;
        // a list written relative to the launch directory would not persist.
        Self {
            file: directory
                .is_absolute()
                .then(|| directory.join(APP_DIR).join(FILE_NAME)),
        }
    }

    /// Everything hidden so far. Missing or unreadable storage hides nothing.
    pub(crate) fn load(&self) -> Hidden {
        let mut hidden = Hidden::new();
        let Some(text) = self
            .file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
        else {
            return hidden;
        };
        for line in text.lines() {
            let Some((seconds, uri)) = line.split_once('\t') else {
                continue;
            };
            let (Ok(hidden_at), Some(path)) =
                (seconds.parse::<i64>(), gio::File::for_uri(uri).path())
            else {
                continue;
            };
            let known = hidden.entry(path).or_insert(hidden_at);
            *known = (*known).max(hidden_at);
        }
        hidden
    }

    /// Hides `path` as of `now` (Unix seconds). `false` when the list could
    /// not be written; the previous list is then still there.
    pub(crate) fn hide(&self, path: &Path, now: i64) -> bool {
        let mut hidden = self.load();
        hidden.insert(path.to_path_buf(), now);
        self.write(&hidden)
    }

    /// Forgets that `path` was hidden. Called on Vitela's own successful
    /// opens, so a reopen inside the second of the removal still brings the
    /// card back.
    pub(crate) fn unhide(&self, path: &Path) {
        let mut hidden = self.load();
        if hidden.remove(path).is_some() {
            self.write(&hidden);
        }
    }

    fn write(&self, hidden: &Hidden) -> bool {
        let Some(file) = &self.file else {
            return false;
        };
        // A document that no longer exists can never show again; keeping it
        // would only grow the file.
        let mut entries: Vec<_> = hidden.iter().filter(|(path, _)| path.exists()).collect();
        entries.sort_by_key(|(_, &hidden_at)| std::cmp::Reverse(hidden_at));
        entries.truncate(CAPACITY);
        let text: String = entries
            .into_iter()
            .map(|(path, hidden_at)| format!("{hidden_at}\t{}\n", gio::File::for_path(path).uri()))
            .collect();

        let mut partial = file.clone().into_os_string();
        partial.push(".partial");
        let partial = PathBuf::from(partial);
        let written = write_partial(file, &partial, text.as_bytes())
            .and_then(|()| std::fs::rename(&partial, file));
        if written.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        written.is_ok()
    }
}

fn write_partial(file: &Path, partial: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(partial)?;
    out.write_all(bytes)?;
    out.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    const NOON: i64 = 1_791_374_400;

    /// A scratch directory that removes itself when dropped, panics included.
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new() -> Self {
            static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "vitela-hidden-recents-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("the scratch directory must be creatable");
            Self(path)
        }

        fn store(&self) -> HiddenRecents {
            HiddenRecents::new(self.0.join("data").join(FILE_NAME))
        }

        fn document(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, b"%PDF").expect("the fixture document must be writable");
            path
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_missing_list_hides_nothing() {
        let scratch = ScratchDir::new();
        assert!(scratch.store().load().is_empty());
    }

    #[test]
    fn a_hidden_document_stays_hidden_until_it_is_used_again() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let document = scratch.document("report.pdf");

        assert!(store.hide(&document, NOON));

        let hidden = store.load();
        assert!(hides(&hidden, &document, NOON - 3600));
        assert!(
            hides(&hidden, &document, NOON),
            "same second counts as hidden"
        );
        assert!(
            !hides(&hidden, &document, NOON + 1),
            "reopening it brings it back"
        );
        assert!(!hides(&hidden, &scratch.document("other.pdf"), NOON - 3600));
    }

    #[test]
    fn hiding_survives_a_restart() {
        let scratch = ScratchDir::new();
        let document = scratch.document("report.pdf");
        scratch.store().hide(&document, NOON);

        assert!(hides(&scratch.store().load(), &document, NOON - 60));
    }

    /// The reason the file stores URIs: a tab or newline in a file name must
    /// not split or corrupt a line.
    #[test]
    fn a_name_with_a_tab_and_a_newline_round_trips() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let document = scratch.document("odd\tname\nhere.pdf");

        store.hide(&document, NOON);

        let hidden = store.load();
        assert_eq!(hidden.len(), 1);
        assert!(hides(&hidden, &document, NOON - 1));
    }

    #[test]
    fn unhide_forgets_the_removal() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let document = scratch.document("report.pdf");
        store.hide(&document, NOON);

        store.unhide(&document);
        store.unhide(&document);

        assert!(!hides(&store.load(), &document, NOON - 60));
    }

    #[test]
    fn a_document_that_no_longer_exists_is_dropped() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let gone = scratch.document("gone.pdf");
        store.hide(&gone, NOON);
        std::fs::remove_file(&gone).expect("the fixture is removable");

        store.hide(&scratch.document("kept.pdf"), NOON);

        assert!(!store.load().contains_key(&gone));
    }

    #[test]
    fn the_list_keeps_only_the_newest_removals() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let oldest = scratch.document("oldest.pdf");
        store.hide(&oldest, NOON - 86_400);
        for index in 0..CAPACITY {
            store.hide(
                &scratch.document(&format!("{index}.pdf")),
                NOON + index as i64,
            );
        }

        let hidden = store.load();
        assert_eq!(hidden.len(), CAPACITY);
        assert!(!hidden.contains_key(&oldest));
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let document = scratch.document("report.pdf");
        let file = scratch.0.join("data").join(FILE_NAME);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("creatable");
        let uri = gio::File::for_path(&document).uri();
        std::fs::write(
            &file,
            format!("\nno tab\nnope\t{uri}\n{NOON}\t\n{NOON}\t{uri}\n"),
        )
        .expect("writable");

        let hidden = store.load();
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden.get(&document), Some(&NOON));
    }

    #[test]
    fn a_failed_write_reports_failure_and_keeps_the_old_list() {
        let scratch = ScratchDir::new();
        let store = scratch.store();
        let first = scratch.document("first.pdf");
        store.hide(&first, NOON);
        // A directory squatting on the sibling's name makes the write fail.
        std::fs::create_dir_all(scratch.0.join("data").join(format!("{FILE_NAME}.partial")))
            .expect("creatable");

        assert!(!store.hide(&scratch.document("second.pdf"), NOON));

        let hidden = store.load();
        assert_eq!(hidden.len(), 1);
        assert!(hidden.contains_key(&first));
    }

    #[test]
    fn the_list_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = ScratchDir::new();
        scratch.store().hide(&scratch.document("report.pdf"), NOON);

        let mode = std::fs::metadata(scratch.0.join("data").join(FILE_NAME))
            .expect("the list exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn without_a_data_directory_nothing_is_hidden_or_hideable() {
        let store = HiddenRecents { file: None };
        assert!(!store.hide(Path::new("/tmp/x.pdf"), NOON));
        assert!(store.load().is_empty());
    }
}
