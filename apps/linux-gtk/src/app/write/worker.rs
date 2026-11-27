//! Running a write on a worker thread, and deciding whether its result may
//! still be installed.
//!
//! Everything the three destination-taking chains do *after* the destination
//! is settled — and, for the validating reopen and the session guards, what
//! [`super::preview`] does too, which has no destination to settle.
//!
//! The session guards are the reason a slow write cannot clobber a newer
//! one: a finished write installs its reopened document only while the
//! [`SessionToken`] it started under still describes the live session, and is
//! discarded otherwise.

use std::any::Any;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::{gio, glib};
use pdf_document::Document;
use pdf_render::{PdfiumRenderer, Priority};

use super::super::document::{close_document_in_background, show_document};
use super::super::perf;
use super::super::state::{OpenedDocument, SessionToken, Viewer};
use super::preview::{restore_view_state, take_view_state};
use super::WriteOperation;

/// Runs `write` on a worker thread and folds its reopened document back into
/// the session.
///
/// The tail of all three destination-taking chains, and the reason they
/// report identically: a stale result is discarded rather than installed
/// (see [`prepare_reopened_session`]), a failure is reported only while the
/// session it belongs to is still the current one, and the status line comes
/// from `operation` rather than from three copies of the same `match`.
///
/// `after` is the continuation an ordinary Save may carry — how the
/// unsaved-changes prompt's Save button gets from "keep this work" to the
/// open it was blocking. Signing and protecting pass `None`: neither is ever
/// something another action is waiting on.
pub(super) fn spawn_write<W>(
    viewer: &Viewer,
    operation: WriteOperation,
    token: SessionToken,
    destination: &std::path::Path,
    after: Option<Rc<dyn Fn()>>,
    write: W,
) where
    W: FnOnce() -> Result<OpenedDocument, String> + Send + 'static,
{
    viewer.status.set_text(&operation.busy());
    // Holds the tabs still (`tabs::busy_refusal`) until the result is in:
    // the reopen below must land on the document it was written from.
    let job = crate::app::tabs::JobGuard::hold(viewer);
    // The file is half written until the worker is done: no tab may open it
    // meanwhile.
    let claim = crate::app::tabs::claim_destination(viewer, destination);
    let started = perf::now();
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = save_worker_result(gio::spawn_blocking(write).await);
            perf::finish("save.worker", started, &[("ok", result.is_ok().into())]);
            // Let go before anything below runs: the continuation may close
            // this very tab, which the job would refuse.
            drop(job);
            drop(claim);
            match result {
                Ok(reopened) if let Some(generation) = prepare_reopened_session(&viewer, token) => {
                    let rebuilding = perf::now();
                    let page_count = reopened.page_geometry.len();
                    // The file just written is the document already on
                    // screen. `show_document` installs it as it would any
                    // other — fit-width, top of page 1 — so where the user
                    // was reading is lifted over it, as a preview refresh
                    // and a tab switch lift it.
                    let view = take_view_state(&viewer);
                    show_document(&viewer, generation, reopened);
                    restore_view_state(&viewer, generation, view);
                    perf::finish("save.rebuild", rebuilding, &[("pages", page_count.into())]);
                    perf::finish("save.total", started, &[("pages", page_count.into())]);
                    // The work the recovery entry protected is on disk now.
                    crate::app::recovery::forget(&viewer);
                    viewer.status.set_text(&operation.done());
                    // Last: the continuation may replace this document, and
                    // its own status text should be what remains on screen.
                    if let Some(after) = after {
                        after();
                    }
                }
                // The bytes are on disk, so the write succeeded; only the
                // reopen is not installed, over a document that changed
                // under it.
                Ok(reopened) => {
                    close_document_in_background(reopened.document);
                    viewer.status.set_text(&operation.done());
                }
                Err(error) => viewer.status.set_text(&operation.failed(&error)),
            }
        }
    });
}

pub(super) fn session_matches(viewer: &Viewer, token: SessionToken) -> bool {
    let state = viewer.state.borrow();
    state
        .session
        .as_ref()
        .is_some_and(|session| token.matches(state.generation, session.edit_revision))
}

pub(super) fn prepare_reopened_session(viewer: &Viewer, token: SessionToken) -> Option<u64> {
    let mut state = viewer.state.borrow_mut();
    let edit_revision = state.session.as_ref()?.edit_revision;
    let generation = next_generation_if_current(token, state.generation, edit_revision)?;
    state.generation = generation;
    Some(generation)
}

pub(super) fn next_generation_if_current(
    token: SessionToken,
    generation: u64,
    edit_revision: u64,
) -> Option<u64> {
    token
        .matches(generation, edit_revision)
        .then(|| generation.saturating_add(1))
}

pub(super) fn save_worker_result<T>(
    result: Result<Result<T, String>, Box<dyn Any + Send>>,
) -> Result<T, String> {
    result.map_err(|_| "Save worker stopped unexpectedly.".to_owned())?
}

/// Opens the bytes a save has just produced with the same renderer that will
/// display them, and checks the page count against the model they came from.
///
/// Shared by [`save`](super::save) and [`protect`](super::protect), which
/// differ only in the password they present here: an ordinary save reuses the
/// session's, while a protecting save must use the **new** one, because the
/// old credential is exactly what those bytes no longer answer to.
pub(super) fn validate_written_bytes(
    bytes: &[u8],
    password: Option<&str>,
    document: &Document,
) -> Result<(), String> {
    let renderer = PdfiumRenderer::new();
    let handle = renderer
        .open_document_from_bytes(bytes.to_vec(), password)
        .map_err(|error| error.to_string())?;
    let page_count = renderer
        .page_count(handle, Priority::Visible)
        .wait()
        .map_err(|error| error.to_string());
    let _ = renderer.close_document(handle);
    reopened_matches_model(document, page_count? as usize)
}

/// Checks the pdfium handle a save just produced against the model it was
/// written from — before [`preview`](super::preview) installs it as the
/// preview, and before [`save`](super::save) replaces a file on disk with the
/// bytes behind it.
///
/// pdfium *opening* the bytes is already most of the validation — a file it
/// cannot parse fails `document::open_document` outright, and the
/// `page_sizes` sweep there touches every page — but "opened" is not
/// "matches". What the preview installs afterwards is `backend_pages` taken
/// from the **preserved** model (see `preview::edits::restore_edit_state`), so
/// every canvas index is resolved through the model's page order on the
/// assumption that the reopened handle holds exactly those pages. Let a
/// handle with a different page count through and that assumption fails
/// silently: pages draw, hit-test and accept annotations under another
/// page's identity. Refusing leaves the previous preview up with an
/// explanation instead — the same trade `preview::refresh_preview`'s error arm
/// already makes for a failed save.
///
/// A document with no editable model has no pages to name and expects nothing,
/// which is also why this takes the model rather than the session.
pub(super) fn reopened_matches_model(
    document: &Document,
    reopened_pages: usize,
) -> Result<(), String> {
    if reopened_pages == document.pages.len() {
        return Ok(());
    }
    Err(format!(
        "the saved file opened with {reopened_pages} pages, not the {} the model has",
        document.pages.len()
    ))
}

/// Puts `bytes` where `destination` names, all of them or none: they are
/// written to a temporary file beside the real file, which then takes its
/// place in one rename. A write that fails leaves whatever was there.
///
/// Two things a rename would otherwise change are kept. A destination that
/// is a symbolic link is written *through* ([`through_links`]), so the link
/// stays a link and the file it points at gets the bytes. And a file that is
/// replaced hands its permission mode to the one replacing it, so a document
/// its owner kept private does not come back readable by whatever the umask
/// allows. A destination that does not exist yet has neither, and is created
/// as it always was.
pub(super) fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let destination = &through_links(destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| "destination has no parent directory".to_string())?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document.pdf"),
        std::process::id()
    ));
    // Read before anything is created; `None` for a file that is new.
    let replaced = fs::metadata(destination)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut created_temporary = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        created_temporary = true;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        // After the bytes, through the open handle: the mode may be one that
        // would not let this process open the file for writing again.
        if let Some(permissions) = replaced {
            file.set_permissions(permissions)
                .map_err(|error| error.to_string())?;
        }
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, destination).map_err(|error| error.to_string())
    })();
    if result.is_err() && created_temporary {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// How many links [`through_links`] follows before calling it a loop — the
/// kernel's own limit.
const MAX_LINKS: usize = 40;

/// The file `path` names once every symbolic link *in its last component*
/// has been followed: where a write to `path` actually lands.
///
/// A path that is no link — a file, or nothing yet — is returned as it is.
/// A link to a file that is gone is followed too: it still says where the
/// file belongs. Links among the folders above are left alone; a rename
/// beside the file goes through them wherever they lead.
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
    use std::any::Any;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        atomic_write, next_generation_if_current, reopened_matches_model, save_worker_result,
    };
    use crate::app::state::SessionToken;
    use pdf_document::{Document, Orientation, Page, PageId, PageSize};

    fn a_document_of(pages: usize) -> Document {
        Document::with_pages(
            (0..pages)
                .map(|index| Page::blank(PageId(index as u32), PageSize::A4, Orientation::Portrait))
                .collect(),
        )
    }

    #[test]
    fn a_reopened_handle_with_the_model_s_pages_can_be_installed() {
        assert_eq!(reopened_matches_model(&a_document_of(3), 3), Ok(()));
    }

    #[test]
    fn a_reopened_handle_missing_a_page_is_refused_with_both_counts() {
        assert_eq!(
            reopened_matches_model(&a_document_of(3), 2),
            Err("the saved file opened with 2 pages, not the 3 the model has".to_owned())
        );
    }

    #[test]
    fn a_reopened_handle_with_an_extra_page_is_refused_too() {
        assert!(reopened_matches_model(&a_document_of(1), 2).is_err());
    }

    #[test]
    fn a_document_without_an_editable_model_expects_nothing_of_the_handle() {
        assert_eq!(reopened_matches_model(&Document::blank(), 0), Ok(()));
    }

    #[test]
    fn a_panicked_save_worker_returns_a_typed_save_error() {
        let worker_failure: Box<dyn Any + Send> = Box::new("save worker panic");

        assert_eq!(
            save_worker_result::<()>(Err(worker_failure)),
            Err("Save worker stopped unexpectedly.".to_owned())
        );
    }

    #[test]
    fn a_save_snapshot_error_is_preserved() {
        assert_eq!(
            save_worker_result::<()>(Ok(Err("destination is read-only".to_owned()))),
            Err("destination is read-only".to_owned())
        );
    }

    #[test]
    fn a_matching_save_token_can_install_the_reopened_session_at_the_next_generation() {
        let token = SessionToken {
            generation: 4,
            edit_revision: 2,
        };

        assert_eq!(next_generation_if_current(token, 4, 2), Some(5));
    }

    #[test]
    fn a_stale_save_token_cannot_install_a_reopened_session() {
        let token = SessionToken {
            generation: 4,
            edit_revision: 2,
        };

        assert_eq!(next_generation_if_current(token, 4, 3), None);
    }

    #[test]
    fn a_failed_atomic_write_does_not_delete_a_preexisting_temporary_file() {
        let directory = std::env::temp_dir().join(format!(
            "linux-gtk-atomic-write-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&directory).expect("create isolated temporary directory");
        let destination = directory.join("document.pdf");
        let temporary = directory.join(format!(".document.pdf.{}.tmp", std::process::id()));
        fs::write(&temporary, b"another save owns this file").expect("seed colliding temporary");

        let result = atomic_write(&destination, b"new PDF bytes");

        assert!(result.is_err());
        assert_eq!(
            fs::read(&temporary).expect("preexisting temporary remains"),
            b"another save owns this file"
        );
        fs::remove_dir_all(&directory).expect("remove isolated temporary directory");
    }

    #[test]
    fn an_atomic_write_persists_the_complete_destination_bytes() {
        let directory = std::env::temp_dir().join(format!(
            "linux-gtk-atomic-write-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&directory).expect("create isolated temporary directory");
        let destination = directory.join("document.pdf");

        atomic_write(&destination, b"complete PDF bytes").expect("persist destination");

        assert_eq!(
            fs::read(&destination).expect("read persisted destination"),
            b"complete PDF bytes"
        );
        fs::remove_dir_all(&directory).expect("remove isolated temporary directory");
    }

    /// A folder of one case's own, removed on drop.
    struct Folder(std::path::PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "linux-gtk-atomic-write-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir(&path).expect("create isolated temporary directory");
            Self(path)
        }

        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> = fs::read_dir(&self.0)
                .expect("read the folder")
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn mode_of(path: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).expect("the file").permissions().mode() & 0o7777
    }

    fn set_mode(path: &std::path::Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set the mode");
    }

    /// The rename puts a new file where the old one was; without carrying
    /// the mode over, a file its owner had kept private would come back
    /// readable by whatever the umask allows.
    #[test]
    fn an_atomic_write_keeps_the_mode_of_the_file_it_replaces() {
        let folder = Folder::new("mode");
        let destination = folder.0.join("document.pdf");
        for mode in [0o600, 0o640, 0o664] {
            fs::write(&destination, b"before").expect("seed the destination");
            set_mode(&destination, mode);

            atomic_write(&destination, b"after").expect("replace the destination");

            assert_eq!(mode_of(&destination), mode, "{mode:o}");
            assert_eq!(fs::read(&destination).expect("the file"), b"after");
        }
    }

    #[test]
    fn an_atomic_write_through_a_symlink_keeps_the_link() {
        let folder = Folder::new("link");
        let real = folder.0.join("real.pdf");
        let link = folder.0.join("link.pdf");
        fs::write(&real, b"before").expect("seed the real file");
        set_mode(&real, 0o640);
        std::os::unix::fs::symlink(&real, &link).expect("link to it");

        atomic_write(&link, b"after").expect("write through the link");

        let linked = fs::symlink_metadata(&link).expect("the link");
        assert!(linked.file_type().is_symlink(), "the link is still a link");
        assert_eq!(fs::read(&real).expect("the real file"), b"after");
        assert_eq!(mode_of(&real), 0o640, "the real file keeps its mode");
        assert_eq!(folder.names(), ["link.pdf", "real.pdf"]);
    }

    /// A relative link is relative to the folder the link is in, and the
    /// temporary file goes beside the real file, not beside the link: a
    /// rename across folders may cross file systems.
    #[test]
    fn a_relative_link_into_another_folder_is_followed_from_the_link_s_own() {
        let folder = Folder::new("relative");
        fs::create_dir(folder.0.join("store")).expect("the other folder");
        let real = folder.0.join("store").join("real.pdf");
        let link = folder.0.join("link.pdf");
        fs::write(&real, b"before").expect("seed the real file");
        std::os::unix::fs::symlink("store/real.pdf", &link).expect("link to it");

        atomic_write(&link, b"after").expect("write through the link");

        let linked = fs::symlink_metadata(&link).expect("the link");
        assert!(linked.file_type().is_symlink());
        assert_eq!(fs::read(&real).expect("the real file"), b"after");
        assert_eq!(folder.names(), ["link.pdf", "store"]);
    }

    #[test]
    fn a_chain_of_links_is_followed_to_the_real_file() {
        let folder = Folder::new("chain");
        let real = folder.0.join("real.pdf");
        let middle = folder.0.join("middle.pdf");
        let link = folder.0.join("link.pdf");
        fs::write(&real, b"before").expect("seed the real file");
        std::os::unix::fs::symlink("real.pdf", &middle).expect("the middle link");
        std::os::unix::fs::symlink("middle.pdf", &link).expect("the outer link");

        atomic_write(&link, b"after").expect("write through both links");

        for path in [&link, &middle] {
            let linked = fs::symlink_metadata(path).expect("the link");
            assert!(linked.file_type().is_symlink(), "{path:?}");
        }
        assert_eq!(fs::read(&real).expect("the real file"), b"after");
    }

    /// A link whose file is gone still says where the file belongs.
    #[test]
    fn a_dangling_link_gets_its_file_back() {
        let folder = Folder::new("dangling");
        let link = folder.0.join("link.pdf");
        std::os::unix::fs::symlink("real.pdf", &link).expect("a link to nothing");

        atomic_write(&link, b"after").expect("write through the link");

        let linked = fs::symlink_metadata(&link).expect("the link");
        assert!(linked.file_type().is_symlink());
        assert_eq!(
            fs::read(folder.0.join("real.pdf")).expect("the real file"),
            b"after"
        );
    }

    #[test]
    fn links_that_never_reach_a_file_are_refused() {
        let folder = Folder::new("loop");
        let one = folder.0.join("one.pdf");
        let other = folder.0.join("other.pdf");
        std::os::unix::fs::symlink("other.pdf", &one).expect("one way");
        std::os::unix::fs::symlink("one.pdf", &other).expect("and back");

        assert!(atomic_write(&one, b"after").is_err());
        assert_eq!(folder.names(), ["one.pdf", "other.pdf"], "nothing changed");
    }
}
