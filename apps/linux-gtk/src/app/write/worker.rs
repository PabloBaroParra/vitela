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
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::{gio, glib};
use pdf_document::Document;
use pdf_render::{PdfiumRenderer, Priority};

use super::super::document::{close_document_in_background, show_document};
use super::super::perf;
use super::super::state::{FileStamp, OpenedDocument, SessionToken, Viewer};
use super::preview::{restore_view_state, take_view_state};
use super::WriteOperation;

/// Runs `write` on a worker thread and folds its reopened document back into
/// the session.
///
/// The tail of all three destination-taking chains, and the reason they
/// report identically: a stale result is discarded rather than installed
/// (see [`prepare_reopened_session`]), a failure is reported only while the
/// session it belongs to is still the current one, and what is said comes
/// from `operation` rather than from three copies of the same `match`.
///
/// `after` is the continuation an ordinary Save may carry — how the
/// unsaved-changes prompt's Save button gets from "keep this work" to the
/// open it was blocking. Signing and protecting pass `None`: neither is ever
/// something another action is waiting on.
pub(super) fn spawn_write<W, E>(
    viewer: &Viewer,
    operation: WriteOperation,
    token: SessionToken,
    destination: &std::path::Path,
    after: Option<Rc<dyn Fn()>>,
    write: W,
) where
    W: FnOnce() -> Result<OpenedDocument, E> + Send + 'static,
    E: Into<WriteFailure> + Send + 'static,
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
            // A worker that panicked is a failure like any other.
            let written = gio::spawn_blocking(move || {
                Ok::<Result<OpenedDocument, WriteFailure>, String>(write().map_err(Into::into))
            })
            .await;
            let result = save_worker_result(written)
                .map_err(WriteFailure::Failed)
                .and_then(|written| written);
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
                    adopt_written_stamp(&viewer, &reopened);
                    close_document_in_background(reopened.document);
                    viewer.success(&operation.done());
                }
                Err(WriteFailure::Failed(error)) => viewer.fail(&operation.failed(&error)),
                Err(WriteFailure::Refused(refusal)) => viewer.refuse(&refusal.text()),
            }
        }
    });
}

/// Why a write [`spawn_write`] ran did not land.
pub(super) enum WriteFailure {
    /// It was tried and went wrong: said as the operation's failure, with
    /// the error verbatim.
    Failed(String),
    /// It stood down before replacing anything, for a reason the user can
    /// act on: said as a refusal.
    Refused(Refusal),
}

impl From<String> for WriteFailure {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

/// A refusal decided on the worker thread and worded on the main one: the
/// active language is the main thread's (`i18n`), so the worker hands back
/// the message's key and arguments rather than its text.
pub(super) struct Refusal {
    pub(super) key: &'static str,
    pub(super) args: Vec<(&'static str, String)>,
}

impl Refusal {
    fn text(&self) -> String {
        let args: Vec<_> = self
            .args
            .iter()
            .map(|(name, value)| (*name, value.clone().into()))
            .collect();
        crate::app::i18n::tr_args(self.key, &args)
    }
}

/// Tells the session a write overtook that its file is the one that write
/// left: see [`stamp_after_own_write`].
fn adopt_written_stamp(viewer: &Viewer, written: &OpenedDocument) {
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    session.origin_stamp = stamp_after_own_write(
        (session.origin_path.as_deref(), session.origin_stamp),
        (written.origin_path.as_deref(), written.origin_stamp),
    );
}

/// The stamp a session keeps once a write it started has landed without
/// being installed over it — the user went on editing while the worker
/// wrote.
///
/// `session` and `written` are each the path a document goes by and the
/// stamp of the file behind it. When the two go by the same path, the file
/// on disk is this session's own save and is stamped as such: left with the
/// stamp from before the write, the next Save would find a file "something
/// else has written" (`in_place::changed_since`) and ask whether to replace
/// the user's own work. A write that went anywhere else says nothing about
/// the session's file, and the stamp stays as it was.
pub(super) fn stamp_after_own_write(
    session: (Option<&Path>, Option<FileStamp>),
    written: (Option<&Path>, Option<FileStamp>),
) -> Option<FileStamp> {
    match (session.0, written.0) {
        (Some(origin), Some(destination)) if origin == destination => written.1,
        _ => session.1,
    }
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
    write_temporary(&temporary, bytes, replaced)?;
    fs::rename(&temporary, destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        error.to_string()
    })
}

/// Writes `bytes` to a new `temporary` file and syncs them, giving it the
/// `replaced` file's mode when there is one ([`create_temporary`] says why).
/// A temporary file this created is removed again if any of it fails; one
/// that was already there is not this write's to remove.
fn write_temporary(
    temporary: &Path,
    bytes: &[u8],
    replaced: Option<fs::Permissions>,
) -> Result<(), String> {
    let mut file =
        create_temporary(temporary, replaced.is_some()).map_err(|error| error.to_string())?;
    let written = (|| {
        file.write_all(bytes)?;
        // Through the open handle: the mode may be one that would not let
        // this process open the file for writing again. A mode that cannot
        // be applied fails the write — the file is not put in place with
        // another.
        if let Some(permissions) = replaced {
            file.set_permissions(permissions)?;
        }
        file.sync_all()
    })();
    written.map_err(|error| {
        let _ = fs::remove_file(temporary);
        error.to_string()
    })
}

/// Why [`create_new_file`] did not put a file at its destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CreateFailure {
    /// Something is at the destination now — it appeared after the caller
    /// found nothing there. It is left exactly as it is.
    Exists,
    /// The folder cannot make a file appear only where nothing is: its file
    /// system has no hard links. Nothing was written.
    Unsupported,
    /// The folder cannot be written to now: access is denied, or its file
    /// system is read-only. Nothing was written.
    Unwritable,
    /// The write went wrong; the file-system error, untranslated.
    Failed(String),
}

/// Puts `bytes` at `destination`, which must not exist: the "nothing there
/// yet" a caller checked on the main thread is asked again by the step that
/// creates the file, so a file that appeared meanwhile is never replaced.
///
/// The bytes go to a temporary file beside it first, written and synced as
/// [`atomic_write`] does, and created as any new file is — under the umask,
/// which is the mode the finished file keeps. That file is then *hard-linked*
/// at the destination rather than renamed over it: `link(2)` fails with
/// `EEXIST` when anything is at the new name, a link to nothing included, so
/// the check and the creation are one step and no gap is left between them.
/// The temporary name is then dropped; the destination is the same inode,
/// already synced.
///
/// A failed link is told apart by its errno ([`link_failure`]), not by its
/// [`std::io::ErrorKind`]: std gives `EPERM` and `EACCES` the same kind, and
/// they mean different things here.
///
/// - `EEXIST`: something is at the destination — [`CreateFailure::Exists`].
///   Except when that something is the temporary file itself: on NFS a link
///   the server made can still answer `EEXIST` when its reply was lost and
///   the call retried. The save has then landed, and is reported as such.
/// - `EPERM`, `EOPNOTSUPP`, `ENOSYS`: the file system makes no hard links
///   (FAT, some network and FUSE mounts answer `EPERM` or `EOPNOTSUPP`).
///   `EXDEV` (which the temporary file beside the destination should never
///   meet) and `EMLINK` (the new file cannot take a second name) say the same
///   of this folder. All are [`CreateFailure::Unsupported`]. `EPERM` from
///   `link(2)` is the file system refusing hard links, or a protection on the
///   file itself, which a file this process has just created does not carry;
///   it is not a missing permission on the folder — that is `EACCES`.
/// - `EACCES`, `EROFS`: the folder cannot be written to now — access is
///   denied (a security module, a sandbox, write permission withdrawn after
///   the temporary file was made) or the file system went read-only. That
///   is [`CreateFailure::Unwritable`].
/// - Anything else (`ENOSPC`, `EDQUOT`, …): [`CreateFailure::Failed`], with
///   the error.
///
/// None of the refusals falls back to a rename: that would bring back the
/// very replacement this exists to rule out.
///
/// On every path but success the temporary file this created is removed and
/// the destination is left as it was found. If the temporary name cannot be
/// dropped after the link, the save has still landed and is reported as
/// such; what is left is the hidden temporary file, which a later write to
/// the same name fails on rather than overwrites — as after any write whose
/// clean-up did not run.
pub(super) fn create_new_file(destination: &Path, bytes: &[u8]) -> Result<(), CreateFailure> {
    create_new_file_with(destination, bytes, |temporary, destination| {
        fs::hard_link(temporary, destination)
    })
}

/// [`create_new_file`], with the step that puts the written temporary file
/// at the destination handed in, so a test can stage what happens there.
fn create_new_file_with(
    destination: &Path,
    bytes: &[u8],
    place: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), CreateFailure> {
    let temporary = temporary_beside(destination).map_err(CreateFailure::Failed)?;
    write_temporary(&temporary, bytes, None).map_err(CreateFailure::Failed)?;
    let placed = place(&temporary, destination).or_else(|error| match link_failure(&error) {
        // Asked before the temporary name is dropped: only then can the two
        // be compared.
        CreateFailure::Exists if is_same_file(&temporary, destination) => Ok(()),
        failure => Err(failure),
    });
    let _ = fs::remove_file(&temporary);
    placed
}

/// What a failed `link(2)` of [`create_new_file`]'s temporary file means for
/// the save, decided from its errno alone ([`create_new_file`] says why each
/// one is mapped as it is). An error without an errno is a plain failure.
fn link_failure(error: &std::io::Error) -> CreateFailure {
    match error.raw_os_error() {
        Some(errno::EEXIST) => CreateFailure::Exists,
        Some(errno::EPERM | errno::EOPNOTSUPP | errno::ENOSYS | errno::EXDEV | errno::EMLINK) => {
            CreateFailure::Unsupported
        }
        Some(errno::EACCES | errno::EROFS) => CreateFailure::Unwritable,
        _ => CreateFailure::Failed(error.to_string()),
    }
}

/// Whether `a` and `b` name the same inode. Neither is followed if it is a
/// symbolic link: a link to the file is not the file.
fn is_same_file(a: &Path, b: &Path) -> bool {
    match (fs::symlink_metadata(a), fs::symlink_metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// The `link(2)` errors [`link_failure`] tells apart, by number.
///
/// Defined here rather than taken from `libc`: that crate reaches this one
/// only through other dependencies (cairo and the GTK bindings), and naming
/// it directly would add it to this crate's entry in the lock file.
/// The shell builds for Linux alone, and these are the generic Linux numbers
/// (`asm-generic/errno-base.h`, `asm-generic/errno.h`) that x86, x86-64, Arm,
/// AArch64, RISC-V and PowerPC share; MIPS and SPARC number some of them
/// differently, so building for those is refused below. On Linux `ENOTSUP`
/// is `EOPNOTSUPP`. A test checks each number against the kind this target's
/// std gives it.
mod errno {
    #[cfg(any(
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "mips32r6",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "sparc64"
    ))]
    compile_error!("these errno numbers are the generic Linux ones; this target's differ");

    pub(super) const EPERM: i32 = 1;
    pub(super) const EACCES: i32 = 13;
    pub(super) const EEXIST: i32 = 17;
    pub(super) const EXDEV: i32 = 18;
    pub(super) const EROFS: i32 = 30;
    pub(super) const EMLINK: i32 = 31;
    pub(super) const ENOSYS: i32 = 38;
    pub(super) const EOPNOTSUPP: i32 = 95;
    #[cfg(test)]
    pub(super) const ENOSPC: i32 = 28;
    #[cfg(test)]
    pub(super) const EDQUOT: i32 = 122;
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

    mod own_write {
        use std::path::Path;

        use super::super::stamp_after_own_write;
        use crate::app::state::FileStamp;

        fn stamps() -> (Option<FileStamp>, Option<FileStamp>) {
            let folder = std::env::temp_dir().join(format!(
                "linux-gtk-own-write-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&folder);
            std::fs::create_dir(&folder).expect("create the test folder");
            let file = folder.join("report.pdf");
            std::fs::write(&file, b"before").expect("write the file");
            let before = FileStamp::of(&file);
            std::fs::write(&file, b"after, and longer").expect("rewrite the file");
            let after = FileStamp::of(&file);
            let _ = std::fs::remove_dir_all(&folder);
            assert!(before.is_some() && before != after);
            (before, after)
        }

        #[test]
        fn a_write_to_the_session_s_own_path_restamps_it() {
            let (before, after) = stamps();
            let path = Path::new("/home/user/report.pdf");

            assert_eq!(
                stamp_after_own_write((Some(path), before), (Some(path), after)),
                after
            );
        }

        #[test]
        fn a_write_somewhere_else_leaves_the_stamp_alone() {
            let (before, after) = stamps();

            assert_eq!(
                stamp_after_own_write(
                    (Some(Path::new("/home/user/report.pdf")), before),
                    (Some(Path::new("/home/user/copy.pdf")), after)
                ),
                before
            );
        }

        #[test]
        fn a_session_with_no_file_is_not_given_one_s_stamp() {
            let (_, after) = stamps();

            assert_eq!(
                stamp_after_own_write((None, None), (Some(Path::new("/tmp/a.pdf")), after)),
                None
            );
        }
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

    mod create_new {
        use std::fs;
        use std::io::{Error, ErrorKind};

        use super::super::{
            create_new_file, create_new_file_with, errno, link_failure, CreateFailure,
        };
        use super::{mode_of, Folder};

        /// Nothing is there: the file is created with the bytes, as any new
        /// file is, and nothing else is left in the folder.
        #[test]
        fn a_missing_destination_is_created_with_the_bytes_and_a_new_file_s_mode() {
            let folder = Folder::new("create-new");
            let ordinary = folder.0.join("ordinary");
            fs::write(&ordinary, b"").expect("an ordinary new file");
            let destination = folder.0.join("document.pdf");

            assert_eq!(create_new_file(&destination, b"the document"), Ok(()));

            assert_eq!(fs::read(&destination).expect("created"), b"the document");
            assert_eq!(mode_of(&destination), mode_of(&ordinary));
            assert_eq!(folder.names(), ["document.pdf", "ordinary"]);
        }

        /// A file appears at the destination after the bytes are written
        /// and before they are put there: it is kept, the save is refused,
        /// and no temporary file is left behind.
        #[test]
        fn a_destination_that_appears_before_the_final_step_is_left_alone() {
            let folder = Folder::new("create-new-appeared");
            let destination = folder.0.join("document.pdf");

            let result = create_new_file_with(&destination, b"the document", |from, to| {
                fs::write(to, b"another program's file").expect("the file appears");
                fs::hard_link(from, to)
            });

            assert_eq!(result, Err(CreateFailure::Exists));
            assert_eq!(
                fs::read(&destination).expect("the file that appeared"),
                b"another program's file"
            );
            assert_eq!(
                folder.names(),
                ["document.pdf"],
                "no temporary file is left"
            );
        }

        /// A link to nothing is something at that name too.
        #[test]
        fn a_dangling_link_at_the_destination_is_not_replaced() {
            let folder = Folder::new("create-new-dangling");
            let destination = folder.0.join("document.pdf");
            std::os::unix::fs::symlink("elsewhere.pdf", &destination).expect("a link");

            assert_eq!(
                create_new_file(&destination, b"the document"),
                Err(CreateFailure::Exists)
            );
            assert!(fs::symlink_metadata(&destination)
                .expect("the link")
                .file_type()
                .is_symlink());
            assert_eq!(folder.names(), ["document.pdf"]);
        }

        /// On NFS a `link` the server carried out can still answer EEXIST,
        /// when its first reply was lost and the call was sent again: the
        /// name is then the temporary file itself. That is a save that
        /// landed, not a file that appeared.
        #[test]
        fn a_link_that_landed_but_answered_eexist_is_a_save() {
            let folder = Folder::new("create-new-landed-eexist");
            let destination = folder.0.join("document.pdf");

            let result = create_new_file_with(&destination, b"the document", |from, to| {
                fs::hard_link(from, to).expect("the link lands");
                Err(Error::from_raw_os_error(errno::EEXIST))
            });

            assert_eq!(result, Ok(()));
            assert_eq!(fs::read(&destination).expect("created"), b"the document");
            assert_eq!(
                folder.names(),
                ["document.pdf"],
                "no temporary file is left"
            );
        }

        /// A file system without hard links (a FAT drive, some network and
        /// FUSE mounts) answers EPERM or EOPNOTSUPP; ENOSYS, EXDEV and
        /// EMLINK say the same of this folder in other words. The save is
        /// refused as such — never turned into a rename that could replace
        /// a file.
        #[test]
        fn a_folder_without_hard_links_refuses_rather_than_replacing() {
            for code in [
                errno::EPERM,
                errno::EOPNOTSUPP,
                errno::ENOSYS,
                errno::EXDEV,
                errno::EMLINK,
            ] {
                let folder = Folder::new("create-new-unsupported");
                let destination = folder.0.join("document.pdf");

                let result = create_new_file_with(&destination, b"the document", |_, _| {
                    Err(Error::from_raw_os_error(code))
                });

                assert_eq!(result, Err(CreateFailure::Unsupported), "errno {code}");
                assert!(!destination.exists(), "errno {code}: nothing is created");
                assert!(
                    folder.names().is_empty(),
                    "errno {code}: no temporary file is left"
                );
            }
        }

        /// Access denied (a security module, a sandbox, a folder that lost
        /// its write permission after the temporary file was made) or a
        /// file system now read-only: the folder cannot be written to, and
        /// the save is refused as that rather than as missing hard links.
        #[test]
        fn a_folder_that_cannot_be_written_now_refuses_as_such() {
            for code in [errno::EACCES, errno::EROFS] {
                let folder = Folder::new("create-new-unwritable");
                let destination = folder.0.join("document.pdf");

                let result = create_new_file_with(&destination, b"the document", |_, _| {
                    Err(Error::from_raw_os_error(code))
                });

                assert_eq!(result, Err(CreateFailure::Unwritable), "errno {code}");
                assert!(!destination.exists(), "errno {code}: nothing is created");
                assert!(
                    folder.names().is_empty(),
                    "errno {code}: no temporary file is left"
                );
            }
        }

        /// Any other failure of the last step — a full disk, a spent quota,
        /// an error with no errno — is a failure, with its error, and still
        /// leaves nothing behind.
        #[test]
        fn another_failure_of_the_final_step_is_reported_and_cleaned_up() {
            for error in [
                Error::from_raw_os_error(errno::ENOSPC),
                Error::from_raw_os_error(errno::EDQUOT),
                Error::other("the disk went away"),
            ] {
                let folder = Folder::new("create-new-failed");
                let destination = folder.0.join("document.pdf");
                let said = error.to_string();

                let result = create_new_file_with(&destination, b"the document", |_, _| Err(error));

                assert_eq!(result, Err(CreateFailure::Failed(said.clone())), "{said}");
                assert!(folder.names().is_empty(), "{said}");
            }
        }

        /// What each errno `link(2)` answers with means for the save,
        /// decided from the errno alone.
        #[test]
        fn each_link_error_is_classified_by_its_errno() {
            for (code, expected) in [
                (errno::EEXIST, CreateFailure::Exists),
                (errno::EPERM, CreateFailure::Unsupported),
                (errno::EOPNOTSUPP, CreateFailure::Unsupported),
                (errno::ENOSYS, CreateFailure::Unsupported),
                (errno::EXDEV, CreateFailure::Unsupported),
                (errno::EMLINK, CreateFailure::Unsupported),
                (errno::EACCES, CreateFailure::Unwritable),
                (errno::EROFS, CreateFailure::Unwritable),
            ] {
                assert_eq!(
                    link_failure(&Error::from_raw_os_error(code)),
                    expected,
                    "errno {code}"
                );
            }
            for code in [errno::ENOSPC, errno::EDQUOT] {
                let error = Error::from_raw_os_error(code);
                assert_eq!(
                    link_failure(&error),
                    CreateFailure::Failed(error.to_string()),
                    "errno {code}"
                );
            }
            // Only the errno decides: an error of a refused kind that has
            // none is an ordinary failure.
            let error = Error::from(ErrorKind::PermissionDenied);
            assert_eq!(
                link_failure(&error),
                CreateFailure::Failed(error.to_string())
            );
        }

        /// The errno numbers defined locally are the ones this target's std
        /// knows by those names (EPERM and EACCES share a kind, so this
        /// cannot tell those two apart).
        #[test]
        fn the_local_errno_numbers_name_this_target_s_errors() {
            for (code, kind) in [
                (errno::EPERM, ErrorKind::PermissionDenied),
                (errno::EACCES, ErrorKind::PermissionDenied),
                (errno::EEXIST, ErrorKind::AlreadyExists),
                (errno::EXDEV, ErrorKind::CrossesDevices),
                (errno::EROFS, ErrorKind::ReadOnlyFilesystem),
                (errno::EMLINK, ErrorKind::TooManyLinks),
                (errno::ENOSYS, ErrorKind::Unsupported),
                (errno::EOPNOTSUPP, ErrorKind::Unsupported),
                (errno::ENOSPC, ErrorKind::StorageFull),
                (errno::EDQUOT, ErrorKind::QuotaExceeded),
            ] {
                assert_eq!(Error::from_raw_os_error(code).kind(), kind, "errno {code}");
            }
        }
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
