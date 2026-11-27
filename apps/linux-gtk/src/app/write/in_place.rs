//! Deciding whether a Save writes back to the file the document came from,
//! or has to ask where to go.
//!
//! Save As always asks ([`super::chooser`]). An ordinary Save asks only when
//! writing back is not plainly what will happen. Everything past that
//! decision is the one chain in [`super::save`]: the route settles the
//! destination and nothing else, so the signature warning, the core's
//! refusals and the validating reopen are the same for both.
//!
//! The destination it settles is a *resolved* one. A document opened through
//! a symbolic link is saved into the file the link points at, so the link is
//! followed here, once, on the main thread, and every question below is
//! asked of the file it led to — the same path the chain is then handed.

use std::fs;
use std::path::{Path, PathBuf};

use gtk::gio;
use gtk::prelude::*;

use super::super::state::{FileStamp, Viewer};

/// Where an ordinary Save writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SaveRoute {
    /// Straight back to the file the document came from: no chooser, and no
    /// overwrite question — replacing that file is what Save means. The path
    /// is the file itself, with any link to it already followed.
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
    /// The file is not the one this session read: its [`FileStamp`] differs
    /// from the one taken at the open or the last save, or none was taken.
    /// Something else has written it since, and replacing it without a word
    /// would discard that. The chooser's "Replace?" is the question.
    pub(super) changed_on_disk: bool,
}

/// Chooses the route for a document whose file, links followed, is `file`.
///
/// Asking is the fallback for every case in which writing back is not
/// plainly what will happen: a document that never had a file (the sample, a
/// blank one, pasted bytes), a file that is gone or read-only, a file another
/// tab holds, a copy restored after a crash, and a file written by something
/// else since it was read. The chooser then opens on the origin, so the user
/// sees what was tried.
pub(super) fn save_route(file: Option<&Path>, facts: OriginFacts) -> SaveRoute {
    match file {
        Some(path)
            if facts.replaceable
                && !facts.held_by_another_tab
                && !facts.restored_copy
                && !facts.changed_on_disk =>
        {
            SaveRoute::InPlace(path.to_path_buf())
        }
        _ => SaveRoute::Ask,
    }
}

/// The route for the active document, which came from `origin`.
///
/// `origin` is resolved first and only once: the facts are read for the
/// resolved file, and the resolved file is what [`SaveRoute::InPlace`]
/// carries, so what was checked here is what gets replaced.
pub(super) fn route_for(viewer: &Viewer, origin: &Path) -> SaveRoute {
    match through_links(origin) {
        Ok(file) => save_route(Some(&file), origin_facts(viewer, &file)),
        Err(_) => SaveRoute::Ask,
    }
}

/// Reads [`OriginFacts`] for `file` off the file system, the tabs and the
/// active session.
fn origin_facts(viewer: &Viewer, file: &Path) -> OriginFacts {
    let (restored_copy, read_stamp) = {
        let state = viewer.state.borrow();
        state.session.as_ref().map_or((false, None), |session| {
            (session.unsaved_base, session.origin_stamp)
        })
    };
    OriginFacts {
        replaceable: replaceable_in_place(file),
        held_by_another_tab: crate::app::tabs::path_held_by_other_tab(viewer, file),
        restored_copy,
        changed_on_disk: changed_since(read_stamp, FileStamp::of(file)),
    }
}

/// Whether a file stamped `now` is no longer the one stamped `read`.
///
/// A stamp that was never taken counts as changed: with nothing to compare
/// against, the file cannot be said to be the one that was read.
pub(super) fn changed_since(read: Option<FileStamp>, now: Option<FileStamp>) -> bool {
    read.is_none() || read != now
}

/// Whether `path` is a file this process can replace where it is.
///
/// The file's own permission is asked as well as its folder's:
/// `worker::atomic_write` replaces by renaming a sibling temporary file over
/// the destination, which needs only the folder, so without this a read-only
/// file in a writable folder would be overwritten without a word.
///
/// Asked of the path as given. A link is a file to nobody here — callers
/// resolve first ([`through_links`]), so the folder that has to take the
/// temporary file is the real file's and not the link's.
pub(super) fn replaceable_in_place(path: &Path) -> bool {
    let folder = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
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

/// How many links [`through_links`] follows before calling it a loop — the
/// kernel's own limit.
const MAX_LINKS: usize = 40;

/// The file `path` names once every symbolic link *in its last component*
/// has been followed: where a save of a document opened as `path` belongs.
///
/// A path that is no link — a file, or nothing yet — is returned as it is.
/// A link to a file that is gone is followed too; whether that is a file to
/// write is [`replaceable_in_place`]'s to say. Links among the folders above
/// are left alone; a rename beside the file goes through them wherever they
/// lead.
pub(super) fn through_links(path: &Path) -> Result<PathBuf, String> {
    let mut path = path.to_path_buf();
    for _ in 0..MAX_LINKS {
        let is_link =
            fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink());
        if !is_link {
            return Ok(path);
        }
        let target = fs::read_link(&path).map_err(|error| error.to_string())?;
        // A relative target is relative to the folder the link is in;
        // `join` keeps an absolute one as it is.
        path = match path.parent() {
            Some(folder) => folder.join(target),
            None => target,
        };
    }
    Err("too many levels of symbolic links".to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::{
        changed_since, replaceable_in_place, save_route, through_links, OriginFacts, SaveRoute,
    };
    use crate::app::state::FileStamp;

    const WRITABLE: OriginFacts = OriginFacts {
        replaceable: true,
        held_by_another_tab: false,
        restored_copy: false,
        changed_on_disk: false,
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

    /// Whether this process ignores permission bits, as root does: the
    /// read-only cases below cannot be staged for it. Says so when it does —
    /// a case that returns early has checked nothing, and a run as root
    /// should not read as if it had.
    fn permissions_are_ignored(folder: &Folder, case: &str) -> bool {
        let probe = folder.0.join("probe");
        fs::write(&probe, b"").expect("write the probe");
        fs::set_permissions(&probe, fs::Permissions::from_mode(0o400)).expect("lock the probe");
        let ignored = fs::OpenOptions::new().write(true).open(&probe).is_ok();
        if ignored {
            eprintln!(
                "SKIPPED {case}: this process ignores permission bits (running as root?), \
                 so a read-only file or folder cannot be staged. Nothing was asserted."
            );
        }
        ignored
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
        if permissions_are_ignored(&folder, "a_read_only_file_is_not_replaced") {
            return;
        }

        assert!(!replaceable_in_place(&folder.file(0o400)));
    }

    #[test]
    fn a_file_in_a_read_only_folder_cannot_be_replaced() {
        let folder = Folder::new("read-only-folder");
        if permissions_are_ignored(&folder, "a_file_in_a_read_only_folder") {
            return;
        }
        let file = folder.file(0o600);
        fs::set_permissions(&folder.0, fs::Permissions::from_mode(0o500))
            .expect("lock the test folder");

        assert!(!replaceable_in_place(&file));
    }

    /// Something else wrote the file after this session read it: replacing
    /// it without a word would throw that away.
    #[test]
    fn a_file_written_by_something_else_since_it_was_read_asks() {
        let facts = OriginFacts {
            changed_on_disk: true,
            ..WRITABLE
        };

        assert_eq!(
            save_route(Some(Path::new("/home/user/report.pdf")), facts),
            SaveRoute::Ask
        );
    }

    #[test]
    fn a_file_left_alone_still_has_the_stamp_it_was_read_with() {
        let folder = Folder::new("stamp-same");
        let file = folder.file(0o600);
        let read = FileStamp::of(&file);

        assert!(read.is_some());
        assert!(!changed_since(read, FileStamp::of(&file)));
    }

    #[test]
    fn a_file_rewritten_where_it_is_has_changed() {
        let folder = Folder::new("stamp-rewritten");
        let file = folder.file(0o600);
        let read = FileStamp::of(&file);
        fs::write(&file, b"%PDF-1.7 and then some").expect("rewrite it");

        assert!(changed_since(read, FileStamp::of(&file)));
    }

    /// The way most programs save — this one included: a new file renamed
    /// over the old. Same name, same length here, another file.
    #[test]
    fn a_file_replaced_by_another_of_the_same_size_has_changed() {
        let folder = Folder::new("stamp-replaced");
        let file = folder.file(0o600);
        let read = FileStamp::of(&file);
        let other = folder.0.join("other.pdf");
        fs::write(&other, b"%PDF-1.7").expect("write the other file");
        fs::rename(&other, &file).expect("put it in the first one's place");

        assert!(changed_since(read, FileStamp::of(&file)));
    }

    #[test]
    fn a_file_that_is_gone_has_changed() {
        let folder = Folder::new("stamp-gone");
        let file = folder.file(0o600);
        let read = FileStamp::of(&file);
        fs::remove_file(&file).expect("remove it");

        assert!(changed_since(read, FileStamp::of(&file)));
    }

    /// No stamp was taken — a copy restored after a crash, a session a test
    /// built by hand: nothing says the file is the one that was read.
    #[test]
    fn a_file_that_was_never_stamped_counts_as_changed() {
        let folder = Folder::new("stamp-none");
        let file = folder.file(0o600);

        assert!(changed_since(None, FileStamp::of(&file)));
    }

    #[test]
    fn a_path_that_is_no_link_resolves_to_itself() {
        let folder = Folder::new("no-link");
        let file = folder.file(0o600);
        let missing = folder.0.join("missing.pdf");

        assert_eq!(through_links(&file), Ok(file));
        assert_eq!(through_links(&missing), Ok(missing));
    }

    #[test]
    fn a_link_resolves_to_the_file_it_points_at() {
        let folder = Folder::new("link");
        let file = folder.file(0o600);
        let link = folder.0.join("link.pdf");
        std::os::unix::fs::symlink(&file, &link).expect("link to the file");

        assert_eq!(through_links(&link), Ok(file));
    }

    /// A relative link is relative to the folder the link is in.
    #[test]
    fn a_relative_link_into_another_folder_is_followed_from_the_link_s_own() {
        let folder = Folder::new("link-relative");
        fs::create_dir(folder.0.join("store")).expect("the other folder");
        let link = folder.0.join("link.pdf");
        std::os::unix::fs::symlink("store/real.pdf", &link).expect("link into it");

        assert_eq!(
            through_links(&link),
            Ok(folder.0.join("store").join("real.pdf"))
        );
    }

    #[test]
    fn a_chain_of_links_is_followed_to_its_end() {
        let folder = Folder::new("link-chain");
        let file = folder.file(0o600);
        let middle = folder.0.join("middle.pdf");
        let link = folder.0.join("link.pdf");
        std::os::unix::fs::symlink("document.pdf", &middle).expect("the middle link");
        std::os::unix::fs::symlink("middle.pdf", &link).expect("the outer link");

        assert_eq!(through_links(&link), Ok(file));
    }

    #[test]
    fn links_that_never_reach_a_file_are_refused() {
        let folder = Folder::new("link-loop");
        let one = folder.0.join("one.pdf");
        let other = folder.0.join("other.pdf");
        std::os::unix::fs::symlink("other.pdf", &one).expect("one way");
        std::os::unix::fs::symlink("one.pdf", &other).expect("and back");

        assert!(through_links(&one).is_err());
    }

    /// The write goes beside the real file, so it is the real file's folder
    /// that has to take it — not the one the link happens to sit in.
    #[test]
    fn a_link_to_a_file_in_a_read_only_folder_cannot_be_replaced() {
        let links = Folder::new("link-outside");
        let folder = Folder::new("link-target");
        if permissions_are_ignored(&folder, "a_link_to_a_file_in_a_read_only_folder") {
            return;
        }
        let link = links.0.join("link.pdf");
        std::os::unix::fs::symlink(folder.file(0o600), &link).expect("link to the file");
        fs::set_permissions(&folder.0, fs::Permissions::from_mode(0o500))
            .expect("lock the real file's folder");
        let file = through_links(&link).expect("the link resolves");

        assert!(!replaceable_in_place(&file));
    }

    /// Resolved, a link to nothing is a path with no file at it.
    #[test]
    fn a_link_to_nothing_cannot_be_replaced() {
        let folder = Folder::new("link-dangling");
        let link = folder.0.join("link.pdf");
        std::os::unix::fs::symlink("missing.pdf", &link).expect("a link to nothing");
        let file = through_links(&link).expect("the link resolves");

        assert!(!replaceable_in_place(&file));
    }

    /// The check is of the path it is given: a link is not itself a file to
    /// replace, which is why callers resolve first.
    #[test]
    fn a_link_is_not_itself_a_file_to_replace() {
        let folder = Folder::new("link-unresolved");
        let link = folder.0.join("link.pdf");
        std::os::unix::fs::symlink(folder.file(0o600), &link).expect("link to the file");

        assert!(!replaceable_in_place(&link));
    }
}
