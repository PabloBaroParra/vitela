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
use std::path::Path;
use std::rc::Rc;

use gtk::{gio, glib};
use pdf_document::Document;
use pdf_render::{PdfiumRenderer, Priority};

use super::super::document::{close_document_in_background, show_document};
use super::super::state::{OpenedDocument, SessionToken, Viewer};
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
    after: Option<Rc<dyn Fn()>>,
    write: W,
) where
    W: FnOnce() -> Result<OpenedDocument, String> + Send + 'static,
{
    viewer.status.set_text(operation.busy);
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = save_worker_result(gio::spawn_blocking(write).await);
            match result {
                Ok(reopened) if let Some(generation) = prepare_reopened_session(&viewer, token) => {
                    show_document(&viewer, generation, reopened);
                    viewer.status.set_text(operation.done);
                    // Last: the continuation may replace this document, and
                    // its own status text should be what remains on screen.
                    if let Some(after) = after {
                        after();
                    }
                }
                Ok(reopened) => close_document_in_background(reopened.document),
                Err(error) if session_matches(&viewer, token) => viewer
                    .status
                    .set_text(&format!("{}: {error}", operation.failed)),
                Err(_) => {}
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

pub(super) fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), String> {
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
    let mut created_temporary = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        created_temporary = true;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, destination).map_err(|error| error.to_string())
    })();
    if result.is_err() && created_temporary {
        let _ = fs::remove_file(&temporary);
    }
    result
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
        let mut document = Document::blank();
        for index in 0..pages {
            document.pages.push(Page::blank(
                PageId(index as u32),
                PageSize::A4,
                Orientation::Portrait,
            ));
        }
        document
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
}
