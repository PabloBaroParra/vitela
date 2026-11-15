//! Offering, on startup, the work a crashed session left behind — and putting
//! it back on screen as the unsaved document it was.
//!
//! Each orphan gets one question, newest first: Restore or Discard. Restoring
//! one ends the questions for this launch, because the window holds one
//! document; the others stay on disk and are asked about next time. Escape or
//! closing the dialog is not a decision — the entry is kept, exactly as the
//! unsaved-changes prompt treats every exit that is not a deliberate choice.

use gtk::{gio, glib, AlertDialog, ApplicationWindow};

use super::autosave;
use super::store::{Meta, Orphan};
use crate::app::document::{
    begin_loading, close_document_in_background, is_current, open_document, show_document,
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
pub(super) fn offer(window: &ApplicationWindow, viewer: &Viewer, mut orphans: Vec<Orphan>) {
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
            Choice::Restore => restore(&window, &viewer, orphan, orphans),
            Choice::Discard => {
                orphan.discard();
                offer(&window, &viewer, orphans);
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
/// An unreadable entry is discarded with a log line and the next one asked
/// about: it can never be restored, and asking again next launch would only
/// repeat the failure.
pub(super) fn restore(
    window: &ApplicationWindow,
    viewer: &Viewer,
    orphan: Orphan,
    rest: Vec<Orphan>,
) {
    // A new open: it supersedes whatever was loading, as any open does.
    let generation = begin_loading(viewer);
    viewer.status.set_text(&tr("open-busy"));
    glib::spawn_future_local({
        let window = window.clone();
        let viewer = viewer.clone();
        async move {
            let (orphan, opened) = gio::spawn_blocking(move || {
                let opened = open_orphan(&orphan);
                (orphan, opened)
            })
            .await
            .expect("the recovery open task does not panic");
            match opened {
                Ok(opened) if is_current(&viewer, generation) => {
                    show_document(&viewer, generation, opened);
                    mark_restored(&viewer, orphan.meta());
                    autosave::adopt(&viewer, orphan);
                }
                // Superseded by another open: keep the entry for next time.
                Ok(opened) => close_document_in_background(opened.document),
                Err(error) => {
                    eprintln!("recovery: discarding an entry that cannot be opened: {error}");
                    orphan.discard();
                    if is_current(&viewer, generation) {
                        viewer.status.set_text(&tr("recovery-unreadable"));
                    }
                    offer(&window, &viewer, rest);
                }
            }
        }
    });
}

fn open_orphan(orphan: &Orphan) -> Result<OpenedDocument, String> {
    let bytes = orphan.read_pdf().map_err(|error| error.to_string())?;
    open_document(&DocumentSource::Bytes(bytes), None).map_err(|error| error.to_string())
}

/// What makes the restored bytes the crashed document rather than a new one:
/// its name, the file a Save should suggest, and — like every edit the bytes
/// hold — unsaved.
fn mark_restored(viewer: &Viewer, meta: &Meta) {
    if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
        session.base_name = meta.name.clone();
        session.origin_path = meta.origin.clone();
        session.unsaved_to_disk = true;
        session.unsaved_base = true;
    }
    crate::app::annotations::update_annotation_controls(viewer);
    viewer.status.set_text(&tr("recovery-restored"));
}

#[cfg(test)]
mod tests {
    use super::*;

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
