//! Closing tabs: one at a time from the strip or Ctrl+W, and all the dirty
//! ones in turn when the window closes.

use std::rc::Rc;

use gtk::{glib, ApplicationWindow};

use super::list::AfterClose;
use super::{
    activate_then, active_tab, busy_refusal, first_dirty_parked, park, sync::sync, Arrival, TabId,
};
use crate::app::document::{
    close_document_in_background, confirm_unsaved_edits, has_unsaved_changes_after_commit,
    retire_active_session, show_no_document, UNSAVED_DETAIL_CLOSING,
};
use crate::app::i18n::{tr, tr_args};
use crate::app::state::Viewer;

/// The unsaved-changes detail when one tab is about to close.
const UNSAVED_DETAIL_CLOSING_TAB: &str = "tabs-unsaved-detail-closing";

/// Closes tab `id`, asking first about unsaved changes in it.
///
/// A clean parked tab closes without being shown. A dirty one is activated
/// first, so the Save the prompt offers saves the document the user is
/// looking at.
pub(super) fn close_tab(window: &ApplicationWindow, viewer: &Viewer, id: TabId) {
    let (is_active, parked_dirty) = {
        let tabs = viewer.tabs.borrow();
        (
            tabs.list.active() == Some(id),
            tabs.parked
                .get(&id)
                .map(|parked| parked.session.unsaved_to_disk),
        )
    };
    if is_active {
        close_active(window, viewer);
        return;
    }
    match parked_dirty {
        None => {}
        Some(false) => drop_parked(viewer, id),
        Some(true) => {
            let window = window.clone();
            let activated = activate_then(
                viewer,
                id,
                Arrival::Strip,
                Box::new(move |viewer: &Viewer| close_active(&window, viewer)),
            );
            if let Err(reason) = activated {
                viewer.status.set_text(&reason);
            }
        }
    }
}

/// Closes the active tab — Ctrl+W — asking first when it has unsaved work.
/// Refused like a switch while the document has work or a dialog in flight.
///
/// The answer is bound to the tab that was asked about: the prompt holds
/// the tab in place while it is up (`PromptGuard`), and Save or Discard
/// closes that tab only if it is still the active one.
pub(crate) fn close_active(window: &ApplicationWindow, viewer: &Viewer) {
    let Some(id) = active_tab(viewer) else {
        viewer.status.set_text(&tr("tabs-no-document"));
        return;
    };
    if let Some(reason) = busy_refusal(viewer) {
        viewer.status.set_text(&reason);
        return;
    }
    if !has_unsaved_changes_after_commit(viewer) {
        discard_tab(viewer, id);
        return;
    }
    confirm_unsaved_edits(window, viewer, UNSAVED_DETAIL_CLOSING_TAB, {
        let viewer = viewer.clone();
        move || {
            discard_tab(&viewer, id);
        }
    });
}

/// Asks about every tab with unsaved changes, one after another — each one
/// brought on screen first — and runs `finish` once none is left. Cancel on
/// any of them stops there: `finish` never runs.
pub(crate) fn resolve_dirty_tabs_then(
    window: &ApplicationWindow,
    viewer: &Viewer,
    finish: impl Fn() + 'static,
) {
    resolve_next(window, viewer, Rc::new(finish));
}

fn resolve_next(window: &ApplicationWindow, viewer: &Viewer, finish: Rc<dyn Fn()>) {
    if let Some(id) = active_tab(viewer).filter(|_| has_unsaved_changes_after_commit(viewer)) {
        confirm_unsaved_edits(window, viewer, UNSAVED_DETAIL_CLOSING, {
            let window = window.clone();
            let viewer = viewer.clone();
            move || {
                // Saved or discarded: either way this tab is settled, and
                // closing it is what keeps the next pass from asking again.
                // The next question waits for the main loop: this runs from
                // inside the answered dialog's callback, while that dialog
                // still counts as open and would refuse the switch to the
                // next dirty tab.
                if discard_tab(&viewer, id) {
                    let window = window.clone();
                    let viewer = viewer.clone();
                    let finish = finish.clone();
                    glib::idle_add_local_once(move || resolve_next(&window, &viewer, finish));
                }
            }
        });
        return;
    }
    if let Some(id) = first_dirty_parked(viewer) {
        let window = window.clone();
        let activated = activate_then(
            viewer,
            id,
            Arrival::Strip,
            Box::new(move |viewer: &Viewer| resolve_next(&window, viewer, finish)),
        );
        // A job of the tab just closed may still be running, or a dialog be
        // up: the next dirty tab cannot be brought forward yet. The window
        // stays open, and says why, rather than closing over unsaved work.
        if activated.is_err() {
            viewer.status.set_text(&tr("tabs-close-held"));
        }
        return;
    }
    finish();
}

/// Closes tab `id` without asking — if it is still the active one. `false`,
/// with a status message and nothing closed, when another tab has become
/// active since the question was asked.
fn discard_tab(viewer: &Viewer, id: TabId) -> bool {
    if active_tab(viewer) != Some(id) {
        viewer.status.set_text(&tr("tabs-gone"));
        return false;
    }
    discard_active(viewer);
    true
}

/// Closes the active tab without asking: its document's handle is closed,
/// its recovery entry deleted, and the tab beside it — or Home, when it was
/// the last — takes its place.
fn discard_active(viewer: &Viewer) {
    let name = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .map(|session| session.base_name.clone());
    retire_active_session(viewer);
    // The entry protected exactly this document's unsaved work, which was
    // just saved or discarded.
    crate::app::recovery::forget(viewer);
    // Whatever was in flight for the closed document is stale now.
    super::retire_counters(viewer);
    let after = {
        let mut tabs = viewer.tabs.borrow_mut();
        let closed = tabs.list.active().and_then(|id| tabs.list.close(id));
        match closed {
            Some(AfterClose::Activate(next)) => Some(next),
            // An untracked session (only tests install one) closes onto
            // whichever tab is first.
            _ => tabs
                .list
                .ids()
                .into_iter()
                .find(|id| tabs.parked.contains_key(id)),
        }
    };
    let shown = after.is_some_and(|next| {
        crate::app::document::clear_page_widgets(viewer);
        park::unpark(viewer, next, Arrival::Strip)
    });
    if !shown {
        show_no_document(viewer);
        crate::app::home::show_home(viewer);
        sync(viewer);
    }
    if let Some(name) = name {
        viewer
            .status
            .set_text(&tr_args("tabs-closed", &[("name", name.into())]));
    }
}

/// Closes a clean parked tab: it was never on screen, so only its handle
/// and its place in the strip go.
fn drop_parked(viewer: &Viewer, id: TabId) {
    let parked = {
        let mut tabs = viewer.tabs.borrow_mut();
        tabs.list.close(id);
        tabs.parked.remove(&id)
    };
    let Some(parked) = parked else {
        return;
    };
    let name = parked.session.base_name.clone();
    close_document_in_background(parked.session.document);
    sync(viewer);
    viewer
        .status
        .set_text(&tr_args("tabs-closed", &[("name", name.into())]));
}

#[cfg(test)]
mod tests {
    use super::close_active;
    use crate::app::document::SAMPLE_PDF;
    use crate::app::i18n::tr;
    use crate::app::test_support::{pump_for, wait_until, FileDocument};

    /// Save in the close prompt, for a document that came from a file: the
    /// file is written and the tab closes, with no chooser and no overwrite
    /// question after the one dialog that was answered.
    #[gtk::test]
    fn gtk_ui_save_in_the_close_prompt_writes_the_file_and_closes_the_tab() {
        let closing = FileDocument::new("close-save");
        let path = closing.open_file();
        closing.mark_dirty();
        std::fs::write(&path, b"not what the save writes").expect("stage the file");

        close_active(&closing.built.window, &closing.built.viewer);
        closing.answer(&tr("open-unsaved-save"));

        assert!(
            wait_until(|| closing.tab_count() == 0),
            "the tab closes once its work is saved: {}",
            closing.status()
        );
        assert!(!closing.chooser_is_up(), "no chooser was raised");
        assert!(closing.dialogs().is_empty(), "nothing else was asked");
        let written = std::fs::read(&path).expect("the file is still there");
        assert!(written.starts_with(b"%PDF-"), "the file holds the document");
    }

    /// A save that fails keeps the tab: closing it anyway would discard the
    /// work Save was chosen to keep.
    #[gtk::test]
    fn gtk_ui_a_failed_save_in_the_close_prompt_keeps_the_tab() {
        let closing = FileDocument::new("close-failed");
        let path = closing.open_file();
        closing.mark_dirty();
        // The temporary name the write creates, already taken.
        let temporary = closing
            .folder
            .join(format!(".report.pdf.{}.tmp", std::process::id()));
        std::fs::write(&temporary, b"in the way").expect("block the temporary file");

        close_active(&closing.built.window, &closing.built.viewer);
        closing.answer(&tr("open-unsaved-save"));

        assert!(
            wait_until(|| closing.said("Could not save PDF")),
            "the failure is reported: {}",
            closing.status()
        );
        pump_for(std::time::Duration::from_millis(50));
        assert_eq!(closing.tab_count(), 1, "the tab stays open");
        assert!(closing.dirty(), "with its work still unsaved");
        assert_eq!(std::fs::read(&path).expect("the file"), SAMPLE_PDF);
    }

    /// A document with no file to go back to still has to be given one:
    /// Save in the close prompt raises the chooser, and the tab waits on it.
    #[gtk::test]
    fn gtk_ui_save_in_the_close_prompt_asks_where_to_go_without_a_file() {
        use crate::app::state::DocumentSource;

        let closing = FileDocument::new("close-no-file");
        closing.open(DocumentSource::Embedded(SAMPLE_PDF));
        closing.mark_dirty();

        close_active(&closing.built.window, &closing.built.viewer);
        closing.answer(&tr("open-unsaved-save"));

        assert!(
            wait_until(|| closing.chooser_is_up()),
            "the chooser is raised"
        );
        assert_eq!(closing.tab_count(), 1, "the tab waits for the answer");
        assert!(closing.dirty());
    }
}
