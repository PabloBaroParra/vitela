//! Deciding whether a Save writes back to the file the document came from,
//! or has to ask where to go.
//!
//! Save As always asks ([`super::chooser`]). An ordinary Save asks only when
//! there is no file to write back to. Everything past that decision is the
//! one chain in [`super::save`]: the route settles the destination and
//! nothing else, so the signature warning, the core's refusals and the
//! validating reopen are the same for both.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gio, glib};

use super::super::state::Viewer;
use super::worker::through_links;

/// Where an ordinary Save writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SaveRoute {
    /// Straight back to the file the document came from: no chooser, and no
    /// overwrite question — replacing that file is what Save means.
    InPlace(PathBuf),
    /// Through the Save As chooser.
    Ask,
}

/// What is known about the file a document came from, when it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OriginFacts {
    /// The file is still there and both it and its folder can be written.
    pub(super) replaceable: bool,
    /// Another tab has the same file open. Writing under it would leave that
    /// tab showing, and saving, bytes the file no longer holds — the refusal
    /// `chooser::confirm_destination` makes for a Save As.
    pub(super) held_by_another_tab: bool,
    /// The session was rebuilt from a crash-recovery copy rather than read
    /// from this file (`DocumentSession::unsaved_base`). Nothing has compared
    /// that copy with what the file holds now, so its first save is still a
    /// question; the session the save reopens is the file's own again.
    pub(super) restored_copy: bool,
}

/// Chooses the route for a document that came from `origin`.
///
/// Asking is the fallback for every case in which writing back is not
/// plainly what will happen: a document that never had a file (the sample, a
/// blank one, pasted bytes), a file that is gone or read-only, a file another
/// tab holds, and a copy restored after a crash. The chooser then opens on
/// the origin, so the user sees what was tried.
pub(super) fn save_route(origin: Option<&Path>, facts: OriginFacts) -> SaveRoute {
    match origin {
        Some(path) if facts.replaceable && !facts.held_by_another_tab && !facts.restored_copy => {
            SaveRoute::InPlace(path.to_path_buf())
        }
        _ => SaveRoute::Ask,
    }
}

/// Reads [`OriginFacts`] for `origin` off the file system and the tabs.
pub(super) fn origin_facts(viewer: &Viewer, origin: &Path) -> OriginFacts {
    let restored_copy = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| session.unsaved_base);
    OriginFacts {
        replaceable: replaceable_in_place(origin),
        held_by_another_tab: crate::app::tabs::path_held_by_other_tab(viewer, origin),
        restored_copy,
    }
}

/// Whether `path` is a file this process can replace where it is.
///
/// The file's own permission is asked as well as its folder's:
/// `worker::atomic_write` replaces by renaming a sibling temporary file over
/// the destination, which needs only the folder, so without this a read-only
/// file in a writable folder would be overwritten without a word.
///
/// Asked of the file the write will land on: a symbolic link is followed
/// first, exactly as `atomic_write` follows it, so the folder that has to
/// take the temporary file is the real file's and not the link's.
pub(super) fn replaceable_in_place(path: &Path) -> bool {
    let Ok(path) = through_links(path) else {
        return false;
    };
    let path = path.as_path();
    let folder = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
        && can_write(path)
        && can_write(folder)
}

const CAN_WRITE: &str = "access::can-write";

/// The file system's own answer for this process, not a reading of mode
/// bits: ownership, groups and read-only mounts all count.
fn can_write(path: &Path) -> bool {
    gio::File::for_path(path)
        .query_info(
            CAN_WRITE,
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .is_ok_and(|info| info.boolean(CAN_WRITE))
}

/// How often a save held back by a preview refresh looks again.
const SETTLE_TICK: Duration = Duration::from_millis(20);

/// Runs `run` once no preview refresh is in flight — at once when none is.
///
/// A content edit refreshes the canvas through a save-and-reopen of its own
/// (`preview::refresh_preview`), which moves the session on a generation when
/// it lands. A disk save started under it would finish against a session
/// that is no longer the one it was written from: the bytes would be on disk
/// and the document still marked unsaved. The chooser used to hide this by
/// taking longer than any refresh; a save with no question to ask has to
/// wait on purpose — Ctrl+S with a retype box open commits the box, which
/// starts exactly such a refresh.
///
/// The tabs hold still while a refresh runs (`tabs::busy_refusal`), and the
/// tab is checked once more before `run`, for the instant between the
/// refresh landing and this noticing.
pub(super) fn once_preview_settles(viewer: &Viewer, run: impl FnOnce(&Viewer) + 'static) {
    if !preview_in_flight(viewer) {
        run(viewer);
        return;
    }
    let bound = crate::app::tabs::bind_active(viewer);
    let viewer = viewer.clone();
    let mut run = Some(run);
    glib::timeout_add_local(SETTLE_TICK, move || {
        // A document closed under the wait has nothing left to save.
        if viewer.state.borrow().session.is_none() {
            return glib::ControlFlow::Break;
        }
        if preview_in_flight(&viewer) {
            return glib::ControlFlow::Continue;
        }
        if bound.ensure_active_or_report(&viewer) {
            if let Some(run) = run.take() {
                run(&viewer);
            }
        }
        glib::ControlFlow::Break
    });
}

fn preview_in_flight(viewer: &Viewer) -> bool {
    viewer.state.borrow().preview_refresh_in_flight
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::{replaceable_in_place, save_route, OriginFacts, SaveRoute};

    const WRITABLE: OriginFacts = OriginFacts {
        replaceable: true,
        held_by_another_tab: false,
        restored_copy: false,
    };

    #[test]
    fn a_document_with_a_writable_file_saves_back_to_it() {
        let origin = Path::new("/home/user/report.pdf");

        assert_eq!(
            save_route(Some(origin), WRITABLE),
            SaveRoute::InPlace(origin.to_path_buf())
        );
    }

    #[test]
    fn a_document_that_never_had_a_file_asks_where_to_go() {
        assert_eq!(save_route(None, WRITABLE), SaveRoute::Ask);
    }

    #[test]
    fn a_file_that_cannot_be_replaced_asks_where_to_go() {
        let facts = OriginFacts {
            replaceable: false,
            ..WRITABLE
        };

        assert_eq!(
            save_route(Some(Path::new("/media/cd/report.pdf")), facts),
            SaveRoute::Ask
        );
    }

    /// The refusal Save As makes for a file another tab holds: a Save never
    /// writes under that tab either, and asks instead.
    #[test]
    fn a_file_another_tab_holds_is_not_written_in_place() {
        let facts = OriginFacts {
            held_by_another_tab: true,
            ..WRITABLE
        };

        assert_eq!(
            save_route(Some(Path::new("/home/user/report.pdf")), facts),
            SaveRoute::Ask
        );
    }

    /// A copy restored after a crash was never read from the file it names:
    /// its first save asks, as every save of it did before.
    #[test]
    fn a_copy_restored_after_a_crash_asks_before_replacing_its_file() {
        let facts = OriginFacts {
            restored_copy: true,
            ..WRITABLE
        };

        assert_eq!(
            save_route(Some(Path::new("/home/user/report.pdf")), facts),
            SaveRoute::Ask
        );
    }

    /// A folder of this test's own, removed on drop.
    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("vitela-in-place-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir(&path).expect("create the test folder");
            Self(path)
        }

        fn file(&self, mode: u32) -> PathBuf {
            let path = self.0.join("document.pdf");
            fs::write(&path, b"%PDF-1.7").expect("write the test file");
            fs::set_permissions(&path, fs::Permissions::from_mode(mode))
                .expect("set the test file's mode");
            path
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Whether this process ignores permission bits, as root does: the two
    /// read-only cases below cannot be staged for it.
    fn permissions_are_ignored(folder: &Folder) -> bool {
        let probe = folder.0.join("probe");
        fs::write(&probe, b"").expect("write the probe");
        fs::set_permissions(&probe, fs::Permissions::from_mode(0o400)).expect("lock the probe");
        fs::OpenOptions::new().write(true).open(&probe).is_ok()
    }

    #[test]
    fn a_writable_file_in_a_writable_folder_can_be_replaced() {
        let folder = Folder::new("writable");

        assert!(replaceable_in_place(&folder.file(0o600)));
    }

    #[test]
    fn a_file_that_is_gone_cannot_be_replaced() {
        let folder = Folder::new("gone");

        assert!(!replaceable_in_place(&folder.0.join("missing.pdf")));
    }

    #[test]
    fn a_folder_is_not_a_file_to_replace() {
        let folder = Folder::new("folder");

        assert!(!replaceable_in_place(&folder.0));
    }

    /// The rename `atomic_write` ends on would replace this file all the
    /// same — the folder is writable — so the route has to be what stops it.
    #[test]
    fn a_read_only_file_is_not_replaced_even_in_a_writable_folder() {
        let folder = Folder::new("read-only-file");
        if permissions_are_ignored(&folder) {
            return;
        }

        assert!(!replaceable_in_place(&folder.file(0o400)));
    }

    #[test]
    fn a_file_in_a_read_only_folder_cannot_be_replaced() {
        let folder = Folder::new("read-only-folder");
        if permissions_are_ignored(&folder) {
            return;
        }
        let file = folder.file(0o600);
        fs::set_permissions(&folder.0, fs::Permissions::from_mode(0o500))
            .expect("lock the test folder");

        assert!(!replaceable_in_place(&file));
    }

    #[test]
    fn a_link_to_a_writable_file_can_be_written_through() {
        let folder = Folder::new("link");
        let link = folder.0.join("link.pdf");
        std::os::unix::fs::symlink(folder.file(0o600), &link).expect("link to the file");

        assert!(replaceable_in_place(&link));
    }

    /// The write goes beside the real file, so it is the real file's folder
    /// that has to take it — not the one the link happens to sit in.
    #[test]
    fn a_link_to_a_file_in_a_read_only_folder_cannot_be_replaced() {
        let links = Folder::new("link-outside");
        let folder = Folder::new("link-target");
        if permissions_are_ignored(&folder) {
            return;
        }
        let link = links.0.join("link.pdf");
        std::os::unix::fs::symlink(folder.file(0o600), &link).expect("link to the file");
        fs::set_permissions(&folder.0, fs::Permissions::from_mode(0o500))
            .expect("lock the real file's folder");

        assert!(!replaceable_in_place(&link));
    }

    #[test]
    fn a_link_to_nothing_cannot_be_replaced() {
        let folder = Folder::new("link-dangling");
        let link = folder.0.join("link.pdf");
        std::os::unix::fs::symlink("missing.pdf", &link).expect("a link to nothing");

        assert!(!replaceable_in_place(&link));
    }
}
