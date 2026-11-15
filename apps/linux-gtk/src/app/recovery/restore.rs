//! Offering, on startup, the work a crashed session left behind — and putting
//! it back on screen as the unsaved document it was.
//!
//! Each orphan gets one question, newest first: Restore or Discard. Restoring
//! one ends the questions for this launch, because the window holds one
//! document; the others stay on disk and are asked about next time. Escape or
//! closing the dialog is not a decision — the entry is kept, exactly as the
//! unsaved-changes prompt treats every exit that is not a deliberate choice.

use std::io;
use std::path::{Path, PathBuf};

use gtk::{gio, glib, AlertDialog, ApplicationWindow};
use pdf_render::RenderError;

use super::autosave;
use super::meta::Meta;
use super::store::Orphan;
use crate::app::document::{
    begin_loading, close_document_in_background, is_current, open_document, open_file,
    show_document,
};
use crate::app::i18n::{tr, tr_args};
use crate::app::state::{DocumentSource, OpenedDocument, Viewer};

/// What the user answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Restore,
    Discard,
    /// Any exit that is not a button: ask again next launch.
    Later,
}

/// The buttons are `[Discard, Restore]`, in that order.
fn choice(response: Option<i32>) -> Choice {
    match response {
        Some(0) => Choice::Discard,
        Some(1) => Choice::Restore,
        _ => Choice::Later,
    }
}

/// Asks about the first of `orphans`, then — unless it was restored — about
/// the rest.
pub(super) fn offer(
    window: &ApplicationWindow,
    viewer: &Viewer,
    mut orphans: Vec<Orphan>,
    launch_file: Option<PathBuf>,
) {
    if orphans.is_empty() {
        return;
    }
    let orphan = orphans.remove(0);
    let dialog = AlertDialog::builder()
        .message(tr("recovery-title"))
        .detail(detail(orphan.meta()))
        .buttons([tr("recovery-discard"), tr("recovery-restore")])
        .default_button(1)
        .modal(true)
        .build();
    dialog.choose(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |response| match choice(response.ok()) {
            Choice::Restore => restore(&window, &viewer, orphan, orphans, launch_file.clone()),
            Choice::Discard => {
                orphan.discard();
                offer(&window, &viewer, orphans, launch_file.clone());
            }
            // Dropping the claims releases their locks; the files stay.
            Choice::Later => viewer.status.set_text(&tr("recovery-kept")),
        }
    });
}

fn detail(meta: &Meta) -> String {
    tr_args(
        "recovery-detail",
        &[
            ("name", meta.name.clone().into()),
            ("time", saved_at_text(meta.saved_at).into()),
        ],
    )
}

/// When the copy was written, in the user's locale and time zone.
fn saved_at_text(saved_at: i64) -> String {
    glib::DateTime::from_unix_local(saved_at)
        .and_then(|time| time.format("%x %X"))
        .map_or_else(|_| saved_at.to_string(), |text| text.to_string())
}

/// Opens the orphan's bytes as an unsaved document named and placed like the
/// one that crashed, then hands the entry to this window's autosave.
///
/// A failure keeps the entry unless it proves the bytes can never be opened
/// (see [`Failure`]), then asks about the next one. Either way, a restore
/// that took the place of the file the application was launched to open
/// opens that file after all — see [`open_launch_file`].
pub(super) fn restore(
    window: &ApplicationWindow,
    viewer: &Viewer,
    orphan: Orphan,
    rest: Vec<Orphan>,
    launch_file: Option<PathBuf>,
) {
    // A new open: it supersedes whatever was loading, as any open does.
    let generation = begin_loading(viewer);
    viewer.status.set_text(&tr("open-busy"));
    glib::spawn_future_local({
        let window = window.clone();
        let viewer = viewer.clone();
        async move {
            let opened = gio::spawn_blocking(move || {
                let opened = open_orphan(&orphan);
                (orphan, opened)
            })
            .await;
            let failure = match opened {
                Ok((orphan, Ok(opened))) if is_current(&viewer, generation) => {
                    show_document(&viewer, generation, opened);
                    mark_restored(&viewer, orphan.meta());
                    autosave::adopt(&viewer, orphan);
                    return;
                }
                // Superseded by another open: keep the entry for next time.
                Ok((_orphan, Ok(opened))) => {
                    close_document_in_background(opened.document);
                    return;
                }
                Ok((orphan, Err(Failure::Unrecoverable(error)))) => {
                    eprintln!("recovery: discarding an entry that cannot be opened: {error}");
                    orphan.discard();
                    "recovery-unreadable"
                }
                // Dropping the claim releases its lock; the files stay.
                Ok((_orphan, Err(Failure::Transient(error)))) => {
                    eprintln!("recovery: keeping an entry that could not be opened: {error}");
                    "recovery-restore-failed"
                }
                // The open panicked, and the claim went with it: lock
                // released, files kept.
                Err(_) => {
                    eprintln!("recovery: the restore stopped unexpectedly; keeping the entry");
                    "recovery-restore-failed"
                }
            };
            if is_current(&viewer, generation) {
                viewer.status.set_text(&tr(failure));
                open_launch_file(&viewer, launch_file.as_deref());
            }
            offer(&window, &viewer, rest, launch_file);
        }
    });
}

/// Opens the file the application was launched with, unless it is already
/// what the window shows — the launch's own open may have finished before
/// the restore was chosen. Through the ordinary open, so its unsaved-changes
/// question still guards whatever is on screen.
fn open_launch_file(viewer: &Viewer, launch_file: Option<&Path>) {
    let Some(path) = launch_file else {
        return;
    };
    let shown = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| session.origin_path.as_deref() == Some(path));
    if !shown {
        open_file(viewer, path.to_path_buf());
    }
}

/// Why a restore did not put the document on screen.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    /// The bytes can never be opened: they are not a PDF, or pdfium refuses
    /// them. The entry is discarded — asking again next launch would only
    /// repeat the failure.
    Unrecoverable(String),
    /// Something around the bytes failed — the disk, the renderer — and says
    /// nothing about them. The entry is kept for the next launch: deleting
    /// the user's only copy over a passing fault is the failure to avoid.
    Transient(String),
}

fn read_failure(error: io::Error) -> Failure {
    match error.kind() {
        // `Orphan::read_pdf`'s verdict on the bytes themselves.
        io::ErrorKind::InvalidData => Failure::Unrecoverable(error.to_string()),
        _ => Failure::Transient(error.to_string()),
    }
}

fn open_failure(error: RenderError) -> Failure {
    match error {
        // From bytes there is no file to be missing: pdfium read them and
        // refused the format. And autosave never writes an encrypted
        // snapshot, so bytes that want a password are not one it wrote.
        RenderError::OpenDocument(_) | RenderError::InvalidPassword => {
            Failure::Unrecoverable(error.to_string())
        }
        _ => Failure::Transient(error.to_string()),
    }
}

fn open_orphan(orphan: &Orphan) -> Result<OpenedDocument, Failure> {
    let bytes = orphan.read_pdf().map_err(read_failure)?;
    open_document(&DocumentSource::Bytes(bytes), None).map_err(open_failure)
}

/// What makes the restored bytes the crashed document rather than a new one:
/// its name, the file a Save should suggest, — like every edit the bytes
/// hold — unsaved, and whether those edits broke a signature the original
/// carries.
fn mark_restored(viewer: &Viewer, meta: &Meta) {
    if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
        session.base_name = meta.name.clone();
        session.origin_path = meta.origin.clone();
        session.unsaved_to_disk = true;
        session.unsaved_base = true;
        session.base_breaks_signatures = meta.breaks_signatures;
    }
    crate::app::annotations::update_annotation_controls(viewer);
    viewer.status.set_text(&tr("recovery-restored"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_that_are_not_a_pdf_can_never_be_restored() {
        let error = io::Error::new(io::ErrorKind::InvalidData, "not a PDF");

        assert!(matches!(read_failure(error), Failure::Unrecoverable(_)));
    }

    /// The disk failing to hand the bytes over says nothing about the bytes.
    #[test]
    fn a_read_that_fails_for_any_other_reason_is_transient() {
        for kind in [
            io::ErrorKind::NotFound,
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::Interrupted,
            io::ErrorKind::IsADirectory,
        ] {
            let failure = read_failure(io::Error::new(kind, "disk trouble"));
            assert!(matches!(failure, Failure::Transient(_)), "{kind:?}");
        }
    }

    /// pdfium refusing the bytes: no later launch will read them either. A
    /// recovery snapshot is never encrypted, so a password prompt means the
    /// bytes are not what autosave wrote.
    #[test]
    fn bytes_pdfium_refuses_can_never_be_restored() {
        for error in [
            RenderError::OpenDocument("FormatError".to_owned()),
            RenderError::InvalidPassword,
        ] {
            assert!(matches!(open_failure(error), Failure::Unrecoverable(_)));
        }
    }

    /// The renderer, not the bytes: worth asking about again next launch.
    #[test]
    fn a_renderer_that_cannot_open_anything_is_transient() {
        for error in [
            RenderError::ActorShutDown,
            RenderError::LibraryUnavailable("gone".to_owned()),
            RenderError::LibraryLoad {
                path: PathBuf::from("libpdfium.so"),
                message: "missing".to_owned(),
            },
            RenderError::Cancelled,
        ] {
            assert!(matches!(open_failure(error), Failure::Transient(_)));
        }
    }

    #[test]
    fn the_buttons_answer_in_their_order() {
        assert_eq!(choice(Some(0)), Choice::Discard);
        assert_eq!(choice(Some(1)), Choice::Restore);
    }

    /// Escape, the window's close button, or a dialog torn down: none of them
    /// is the user deciding to lose the work.
    #[test]
    fn any_other_exit_keeps_the_entry_for_later() {
        assert_eq!(choice(None), Choice::Later);
        assert_eq!(choice(Some(7)), Choice::Later);
    }

    #[test]
    fn the_saved_time_falls_back_to_seconds_only_when_it_cannot_be_formatted() {
        assert_ne!(saved_at_text(1_700_000_000), "1700000000");
        assert!(!saved_at_text(1_700_000_000).is_empty());
    }
}
