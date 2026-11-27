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
use std::os::unix::fs::OpenOptionsExt;
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
    viewer.info(&operation.busy());
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
                    viewer.success(&operation.done());
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
                    viewer.success(&operation.done());
                }
                Err(error) => viewer.fail(&operation.failed(&error)),
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
/// written to a temporary file beside it, which then takes its place in one
/// rename. A write that fails leaves whatever was there.
///
/// It replaces exactly the path it is given. A destination that is a
/// symbolic link is replaced as a link would be by any rename — the link
/// goes, a file takes its name — because the path was chosen by the caller
/// and checked on the main thread, and following a link here would write
/// somewhere neither of those saw. The one caller that means "the file this
/// link points at" resolves it first (`in_place::through_links`) and passes
/// that.
///
/// A file that is replaced hands its permission mode to the one replacing
/// it, so a document its owner kept private does not come back readable by
/// whatever the umask allows. See [`create_temporary`] for how the temporary
/// file avoids being wider than either on the way.
pub(super) fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = temporary_beside(destination)?;
    // Read before anything is created; `None` for a file that is new.
    let replaced = fs::metadata(destination)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut created_temporary = false;
    let result = (|| {
        let mut file =
            create_temporary(&temporary, replaced.is_some()).map_err(|error| error.to_string())?;
        created_temporary = true;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        // Through the open handle: the mode may be one that would not let
        // this process open the file for writing again. A mode that cannot
        // be applied fails the write — the file is not put in place with
        // another.
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

/// The temporary file [`atomic_write`] writes before renaming it over
/// `destination`: a sibling, so the rename never crosses a file system.
pub(crate) fn temporary_beside(destination: &Path) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "destination has no parent directory".to_string())?;
    Ok(parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document.pdf"),
        std::process::id()
    )))
}

/// Creates [`atomic_write`]'s temporary file, which must not exist yet.
///
/// When it is `replacing` a file it starts out readable by its owner alone:
/// it holds the document from the first byte, and the file it will replace
/// may be private. `atomic_write` gives it that file's mode once the bytes
/// are in. With nothing to replace it is created as any new file is — under
/// the umask, which is the mode the finished file keeps, so it is never
/// wider than what it becomes.
fn create_temporary(temporary: &Path, replacing: bool) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    if replacing {
        options.mode(0o600);
    }
    options.open(temporary)
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        atomic_write, create_temporary, next_generation_if_current, reopened_matches_model,
        save_worker_result, temporary_beside,
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
        let temporary = temporary_beside(&destination).expect("a sibling");
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

    /// The path it is given is the path it replaces. A link is replaced as
    /// a rename replaces one — it does not send the bytes to whatever the
    /// link points at, which nothing upstream chose or checked.
    #[test]
    fn an_atomic_write_replaces_a_link_rather_than_following_it() {
        let folder = Folder::new("link");
        let real = folder.0.join("real.pdf");
        let link = folder.0.join("link.pdf");
        fs::write(&real, b"before").expect("seed the real file");
        std::os::unix::fs::symlink(&real, &link).expect("link to it");

        atomic_write(&link, b"after").expect("replace the link");

        let replaced = fs::symlink_metadata(&link).expect("the destination");
        assert!(
            replaced.file_type().is_file(),
            "a file took the link's name"
        );
        assert_eq!(fs::read(&link).expect("the destination"), b"after");
        assert_eq!(
            fs::read(&real).expect("the real file"),
            b"before",
            "what the link pointed at is untouched"
        );
        assert_eq!(folder.names(), ["link.pdf", "real.pdf"]);
    }

    /// A link to nothing is no different: the write lands on its name.
    #[test]
    fn an_atomic_write_does_not_create_what_a_dangling_link_names() {
        let folder = Folder::new("dangling");
        let link = folder.0.join("link.pdf");
        std::os::unix::fs::symlink("real.pdf", &link).expect("a link to nothing");

        atomic_write(&link, b"after").expect("replace the link");

        assert_eq!(folder.names(), ["link.pdf"]);
        assert_eq!(fs::read(&link).expect("the destination"), b"after");
    }

    /// The temporary file holds the document from its first byte, so while
    /// it stands in for a file that may be private it is its owner's alone —
    /// whatever the umask would have allowed.
    #[test]
    fn the_temporary_file_for_a_replacement_starts_out_private() {
        let folder = Folder::new("temporary-private");
        let temporary = folder.0.join(".document.pdf.tmp");

        let file = create_temporary(&temporary, true).expect("create it");

        assert_eq!(mode_of(&temporary), 0o600);
        drop(file);
    }

    /// With nothing to replace there is nothing to be more private than:
    /// the file is created as any new one, under the umask.
    #[test]
    fn the_temporary_file_for_a_new_destination_is_an_ordinary_new_file() {
        let folder = Folder::new("temporary-new");
        let ordinary = folder.0.join("ordinary");
        fs::write(&ordinary, b"").expect("an ordinary new file");
        let temporary = folder.0.join(".document.pdf.tmp");

        let file = create_temporary(&temporary, false).expect("create it");

        assert_eq!(mode_of(&temporary), mode_of(&ordinary));
        drop(file);
    }

    #[test]
    fn a_new_destination_is_created_as_any_new_file_is() {
        let folder = Folder::new("new-mode");
        let ordinary = folder.0.join("ordinary");
        fs::write(&ordinary, b"").expect("an ordinary new file");
        let destination = folder.0.join("document.pdf");

        atomic_write(&destination, b"after").expect("create the destination");

        assert_eq!(mode_of(&destination), mode_of(&ordinary));
    }

    #[test]
    fn the_temporary_file_is_a_hidden_sibling_named_for_this_process() {
        assert_eq!(
            temporary_beside(std::path::Path::new("/home/user/report.pdf")),
            Ok(std::path::PathBuf::from(format!(
                "/home/user/.report.pdf.{}.tmp",
                std::process::id()
            )))
        );
    }
}
