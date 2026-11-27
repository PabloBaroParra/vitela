//! Opening documents into tabs: one open at a time, queued, so none cancels
//! another — and each landing in a tab of its own, in front or in the
//! background.
//!
//! An open is a [`Starter`] — the file chooser's choice, a recents card, a
//! drop, an "Open With" launch, a crash restore. It runs when the one before
//! it has ended, and every end of an open ([`finish_open`]) starts the next.
//! One at a time rather than all at once because the encrypted-document
//! password prompt is one per window.
//!
//! **Where it lands** ([`land_in_new_tab`]): in front, as the active tab,
//! only when it is the first file of a user action ([`Placement::Front`])
//! and the active tab is idle — nothing in flight, no dialog, no retype box
//! left open. Otherwise it lands parked, in the background, at once: an open
//! never waits, and never takes the active tab away from what the user is
//! doing in it. With no document on screen at all there is nothing to take
//! away, and every open lands in front.

use std::path::{Path, PathBuf};

use gtk::ApplicationWindow;

use super::{activate_then, active_tab, busy_refusal, park, Arrival, TabId};
use crate::app::document::{close_document_in_background, open_source, show_document};
use crate::app::home::{abandon_pending_tool, HOME_PAGE};
use crate::app::i18n::tr;
use crate::app::state::{DocumentSource, OpenedDocument, Viewer};

/// Starts one open. It must end, however it ends, with [`finish_open`] —
/// or with a landing, whose continuation does.
pub(crate) type Starter = Box<dyn FnOnce(&ApplicationWindow, &Viewer)>;

/// What runs once an open has landed, given the tab it landed in — `None`
/// when it was dropped instead (a reload whose tab changed meanwhile). It
/// always runs, so the open always ends.
pub(crate) type Landed = Box<dyn FnOnce(&Viewer, Option<TabId>)>;

/// Where an open may land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placement {
    /// The first file of a user action: the active tab, if that tab is idle.
    Front,
    /// Any other file — the rest of a multi-file pick, an extra restored
    /// orphan: a parked tab in the background.
    Back,
    /// The file tab `.0` shows, read from disk afresh into that same tab, as
    /// a single window did before tabs. It lands only if that tab is still
    /// the active one, still holds that file and still has nothing unsaved;
    /// otherwise it is dropped, never opened beside it.
    Reload(TabId),
    /// [`Placement::Reload`] over unsaved work (and a retype box) the user
    /// chose to discard.
    ReloadOverUnsaved(TabId),
}

/// What an open of a file some tab already holds comes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlreadyOpen {
    /// No tab holds it: open it.
    No,
    /// Its tab was brought forward, or the open refused: nothing to read.
    Handled,
    /// It is the clean file on screen: read it again, into its own tab.
    Reload(TabId),
    /// It is the file on screen, with unsaved changes: ask Save, Discard or
    /// Cancel first, then read it again into its own tab.
    AskThenReload(TabId),
}

/// Runs `start` now when no open is running, or after every open queued
/// before it has ended.
pub(crate) fn enqueue_open(window: &ApplicationWindow, viewer: &Viewer, start: Starter) {
    {
        let mut tabs = viewer.tabs.borrow_mut();
        if tabs.opening {
            tabs.queued_opens.push_back(start);
            return;
        }
        tabs.opening = true;
    }
    start(window, viewer);
}

/// Ends the running open — landed, failed or abandoned — and starts the
/// next queued one.
pub(crate) fn finish_open(window: &ApplicationWindow, viewer: &Viewer) {
    let next = {
        let mut tabs = viewer.tabs.borrow_mut();
        let next = tabs.queued_opens.pop_front();
        if next.is_none() {
            tabs.opening = false;
        }
        next
    };
    if let Some(next) = next {
        next(window, viewer);
    }
}

/// Opens `paths`, one tab each, in order: the first in front, the rest in
/// the background.
pub(crate) fn open_paths(window: &ApplicationWindow, viewer: &Viewer, paths: Vec<PathBuf>) {
    for (index, path) in paths.into_iter().enumerate() {
        let placement = if index == 0 {
            Placement::Front
        } else {
            Placement::Back
        };
        open_source(window, viewer, DocumentSource::File(path), placement);
    }
}

/// What to do with an open of `path` when a tab may already hold it.
///
/// Held by a background tab: that tab is brought forward, onto the editor
/// with the tool a Home card armed, when `placement` asks for the front.
/// Held by the tab on screen: it is read again from disk, in place — after
/// the unsaved-changes question when it has unsaved work. A file a write is
/// still putting on disk is refused. A background open of a held file
/// changes nothing.
pub(crate) fn activate_open_path(
    viewer: &Viewer,
    path: &Path,
    placement: Placement,
) -> AlreadyOpen {
    if super::being_written(viewer, path) {
        abandon_pending_tool(viewer);
        viewer.refuse(&tr("tabs-path-being-written"));
        return AlreadyOpen::Handled;
    }
    if super::being_written(viewer, path) {
        abandon_pending_tool(viewer);
        viewer.refuse(&tr("tabs-path-being-written"));
        return AlreadyOpen::Handled;
    }
    let Some(id) = super::find_open_tab(viewer, path) else {
        return AlreadyOpen::No;
    };
    if placement == Placement::Back {
        return AlreadyOpen::Handled;
    }
    if viewer.tabs.borrow().list.active() != Some(id) {
        if let Err(reason) = activate_then(viewer, id, Arrival::Open, Box::new(|_| {})) {
            abandon_pending_tool(viewer);
            viewer.refuse(&reason);
        }
        return AlreadyOpen::Handled;
    }
    crate::app::home::show_editor(viewer);
    if let Some(reason) = busy_refusal(viewer) {
        abandon_pending_tool(viewer);
        viewer.refuse(&reason);
        return AlreadyOpen::Handled;
    }
    // An open retype box is committed first, like at every other entry
    // point: one with nothing typed in it changes nothing and leaves the
    // document clean. One the core refuses is unsaved work.
    let committed = crate::app::content_edit::editor::commit_open_retype(viewer);
    let dirty = !committed
        || viewer
            .state
            .borrow()
            .session
            .as_ref()
            .is_some_and(|session| session.unsaved_to_disk);
    if dirty {
        AlreadyOpen::AskThenReload(id)
    } else {
        AlreadyOpen::Reload(id)
    }
}

/// Installs a freshly opened document in a new tab — in front or in the
/// background, see the module doc — then runs `after` with that tab, which
/// ends the open.
///
/// In front, the document that was active is parked first, its open retype
/// box committed like any other edit. A tool a Home card armed applies only
/// to a document landing in front; one landing in the background drops it.
pub(crate) fn land_in_new_tab(
    viewer: &Viewer,
    document: OpenedDocument,
    placement: Placement,
    after: Landed,
) {
    if let Placement::Reload(holder) | Placement::ReloadOverUnsaved(holder) = placement {
        let discarding = matches!(placement, Placement::ReloadOverUnsaved(_));
        if reloads_in_place(viewer, holder, &document, discarding) {
            // Into the tab it was read for; discarded work goes with the
            // session it replaces — retype box and recovery entry included.
            if discarding {
                if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
                    session.content_editor = None;
                }
                crate::app::recovery::forget(viewer);
            }
            let (_, generation) = super::retire_counters(viewer);
            show_document(viewer, generation, document);
            after(viewer, Some(holder));
        } else {
            close_document_in_background(document.document);
            viewer.refuse(&tr("tabs-reload-skipped"));
            after(viewer, None);
        }
        return;
    }
    let id = if lands_in_front(viewer, placement) {
        park::park_active(viewer);
        let (_, generation) = super::retire_counters(viewer);
        show_document(viewer, generation, document);
        active_tab(viewer)
    } else {
        abandon_pending_tool(viewer);
        // Opened from Home, the new tab would otherwise appear in a strip the
        // user cannot see: show the editor — the busy active tab, and the
        // strip with the new one beside it — before the tab says it is there.
        if viewer.view_stack.visible_child_name().as_deref() == Some(HOME_PAGE) {
            crate::app::home::show_editor(viewer);
        }
        // Nothing of this document is painted until its tab is switched to,
        // which is not the end of the open the trace was started for.
        crate::app::perf::forget(crate::app::perf::Mark::Open(document.document));
        Some(park::park_new(viewer, document))
    };
    after(viewer, id);
}

/// Whether a reload read for tab `holder` may still replace what it shows:
/// the read took time, and meanwhile the user may have switched away, closed
/// the tab, saved it elsewhere or started editing. Only the same tab, still
/// active and idle, still showing the file just read, with nothing unsaved —
/// unless `discarding` it was confirmed.
fn reloads_in_place(
    viewer: &Viewer,
    holder: TabId,
    document: &OpenedDocument,
    discarding: bool,
) -> bool {
    if active_tab(viewer) != Some(holder) || busy_refusal(viewer).is_some() {
        return false;
    }
    let state = viewer.state.borrow();
    state.session.as_ref().is_some_and(|session| {
        (discarding || (!session.unsaved_to_disk && session.content_editor.is_none()))
            && session.origin_path.as_deref().map(super::normalised)
                == document.origin_path.as_deref().map(super::normalised)
    })
}

/// Whether an open placed at `placement` may become the active tab now.
fn lands_in_front(viewer: &Viewer, placement: Placement) -> bool {
    if viewer.state.borrow().session.is_none() {
        return true;
    }
    placement == Placement::Front
        && busy_refusal(viewer).is_none()
        // Committed only once nothing else holds the tab; a box the core
        // refuses, or a commit that starts a preview refresh, keeps it.
        && crate::app::content_edit::editor::commit_open_retype(viewer)
        && busy_refusal(viewer).is_none()
}
