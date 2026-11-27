//! Several documents open at once, one tab each — the "parked" model.
//!
//! Only the **active** tab's document lives in `ViewerState::session`, so the
//! dozens of places that read the open document keep reading that one field.
//! Every other tab is *parked*: its whole [`DocumentSession`] — pdfium handle
//! still open, edit log, selections — is moved out of the window state into
//! [`Tabs::parked`], with the reading position it was left at. Its page
//! widgets are not kept; switching back rebuilds them from the open handle
//! ([`park::unpark`]), which costs no disk read and no reparse.
//!
//! Switching bumps both `generation` and `session_id`, so every piece of
//! asynchronous work started against the tab being left — a render, a
//! preview refresh, a save's reopen, a note dialog, an armed signature — is
//! recognised as stale by the existing `is_current`, `SessionToken` and
//! `session_id` checks when it lands, and dropped.
//!
//! **The switching rule** ([`guard::busy_refusal`]): the user cannot switch
//! away from — or close — the active tab while its document has work in
//! flight that would land on it (a preview refresh, any job holding a
//! [`JobGuard`], a PDF import) or a dialog that acts on it is open (a modal
//! window, or a chooser holding a [`PromptGuard`]). A click or Ctrl+W is
//! refused at once with a status message — never carried out later.
//!
//! **Opens never take the active tab away from such a flow** ([`open`]).
//! An open lands as the active tab only when it is the first file of a user
//! action and the active tab is idle; every other open lands as a parked
//! tab in the background, without a wait. Opens never cancel each other:
//! each one is queued and lands in its own tab. Dialogs that outlive a
//! switch the user makes later still bind to the tab they were opened for
//! ([`guard::BoundTab`]).
//!
//! Window-level state stays window-level: tool modes, the content clipboard,
//! the drawn-signature store, toolbars and recents are not per tab.
//! Recovery is per tab: every dirty tab, active or parked, has its own
//! autosave entry (`recovery::autosave`).

pub(crate) mod close;
mod guard;
mod list;
mod open;
mod park;
mod strip;
mod sync;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::app::document::PageShape;
use crate::app::i18n::tr;
use crate::app::state::{DocumentSession, Viewer};
use crate::app::write::ViewState;
pub(crate) use guard::busy_refusal;

pub(crate) use guard::{bind_active, ChooserScope, JobGuard, PromptGuard};
pub(crate) use list::TabId;
use list::TabList;
pub(crate) use open::{
    activate_open_path, enqueue_open, finish_open, land_in_new_tab, open_paths, AlreadyOpen,
    Placement,
};
pub(crate) use strip::{build_strip, TabStrip, TABS_CSS};
pub(crate) use sync::{connect, schedule_sync};

/// A tab whose document is not on screen.
struct ParkedTab {
    session: DocumentSession,
    /// The pages to build widgets for when the tab is shown again.
    shapes: Vec<PageShape>,
    /// The zoom and reading position it was left at; `None` for a tab that
    /// was opened in the background and never shown.
    view: Option<ViewState>,
    /// The window's `(session_id, generation)` when the tab was parked —
    /// both only ever grow, so with the edit revision this names the parked
    /// document's state without ever repeating, even when a rebuild started
    /// the edit revision over (see `recovery::autosave`).
    parked_at: (u64, u64),
}

/// The window's tabs — `Viewer::tabs`.
#[derive(Default)]
pub(crate) struct Tabs {
    list: TabList,
    parked: HashMap<TabId, ParkedTab>,
    /// Opens waiting for the one running to end — see [`open`].
    queued_opens: VecDeque<open::Starter>,
    /// Whether an open is running.
    opening: bool,
    /// Whether a [`sync::sync`] is already queued on the main loop.
    sync_scheduled: bool,
    /// Files a write is putting on disk right now — see [`DestinationClaim`].
    /// In a cell of its own so a claim can let go while the tabs are borrowed.
    claims: Rc<RefCell<Vec<(u64, PathBuf)>>>,
}

#[cfg(test)]
impl Tabs {
    pub(crate) fn is_opening(&self) -> bool {
        self.opening
    }

    pub(crate) fn parked_count(&self) -> usize {
        self.parked.len()
    }

    pub(crate) fn tab_count(&self) -> usize {
        self.list.ids().len()
    }

    /// Forgets every tab and queued open without closing a handle — for
    /// fixtures whose sessions hold no real pdfium document, before they
    /// close the window.
    pub(crate) fn clear_for_test(&mut self) {
        *self = Tabs::default();
    }
}

/// Opens `source` the way a user action does and pumps the main loop until
/// it has landed — for tests in other modules that need real tabs.
#[cfg(test)]
pub(crate) fn open_and_wait(
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
    source: crate::app::state::DocumentSource,
) {
    let count = || viewer.tabs.borrow().list.ids().len();
    let before = count();
    crate::app::document::open_initial(window, viewer, source);
    crate::app::test_support::wait_until(|| count() > before);
    assert!(
        count() > before,
        "the open must land: {}",
        viewer.last_said()
    );
}

/// Switches to the tab at strip position `index`, as a click on it would.
#[cfg(test)]
pub(crate) fn switch_to(viewer: &Viewer, index: usize) {
    let id = viewer.tabs.borrow().list.ids()[index];
    request_activation(viewer, id);
}

/// How a parked tab comes back on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arrival {
    /// A click on the strip, or the window's close bringing a dirty tab
    /// forward: the user stays on the screen they were on.
    Strip,
    /// An open of a file this tab already holds: it lands on the editor
    /// exactly as a new tab would, and a tool a Home card armed applies.
    Open,
}

/// A session has just been installed as the active document. A session
/// installed with no active tab — the first document, or one opening into a
/// new tab after the previous one was parked — gets a tab of its own.
pub(crate) fn session_installed(viewer: &Viewer) {
    active_tab(viewer);
    sync::sync(viewer);
}

/// The active tab, given a tab first if the window shows a document no tab
/// knows about (a test that installs a session directly). `None` with no
/// document on screen.
pub(crate) fn active_tab(viewer: &Viewer) -> Option<TabId> {
    let has_session = viewer.state.borrow().session.is_some();
    let mut tabs = viewer.tabs.borrow_mut();
    match tabs.list.active() {
        Some(id) => Some(id),
        None if has_session => {
            let id = tabs.list.add(None);
            tabs.list.activate(id);
            Some(id)
        }
        None => None,
    }
}

/// Calls `visit` with every tab's session, in strip order, and whether it
/// is the active one. Must not touch `viewer.state` or `viewer.tabs`.
pub(crate) fn for_each_session(
    viewer: &Viewer,
    mut visit: impl FnMut(TabId, &DocumentSession, bool),
) {
    active_tab(viewer);
    let state = viewer.state.borrow();
    let tabs = viewer.tabs.borrow();
    let active = tabs.list.active();
    for id in tabs.list.ids() {
        if let Some(session) = session_of(state.session.as_ref(), &tabs, id) {
            visit(id, session, active == Some(id));
        }
    }
}

/// Calls `visit` with every tab's session and the pair of window counters
/// that, with its edit revision, identifies what it holds: the live
/// `(session_id, generation)` for the active tab, the pair it was parked at
/// for every other.
pub(crate) fn for_each_revision(
    viewer: &Viewer,
    mut visit: impl FnMut(TabId, &DocumentSession, (u64, u64)),
) {
    active_tab(viewer);
    let state = viewer.state.borrow();
    let tabs = viewer.tabs.borrow();
    let active = tabs.list.active();
    for id in tabs.list.ids() {
        if active == Some(id) {
            if let Some(session) = state.session.as_ref() {
                visit(id, session, (state.session_id, state.generation));
            }
        } else if let Some(parked) = tabs.parked.get(&id) {
            visit(id, &parked.session, parked.parked_at);
        }
    }
}

/// Moves the window's `generation` and `session_id` on, so whatever was in
/// flight for the document leaving the screen is recognised as stale when it
/// lands. Answers the new `(session_id, generation)`.
fn retire_counters(viewer: &Viewer) -> (u64, u64) {
    let mut state = viewer.state.borrow_mut();
    state.generation += 1;
    state.session_id += 1;
    (state.session_id, state.generation)
}

/// Tab `id`'s session: the window's own when `id` is active, its parked one
/// otherwise.
fn session_of<'a>(
    active_session: Option<&'a DocumentSession>,
    tabs: &'a Tabs,
    id: TabId,
) -> Option<&'a DocumentSession> {
    if tabs.list.active() == Some(id) {
        active_session
    } else {
        tabs.parked.get(&id).map(|parked| &parked.session)
    }
}

/// Activates tab `id` on a click, saying on the status line why when it
/// cannot.
fn request_activation(viewer: &Viewer, id: TabId) {
    if let Err(reason) = activate_then(viewer, id, Arrival::Strip, Box::new(|_| {})) {
        viewer.refuse(&reason);
    }
}

/// Gives tab `id`'s session to `edit`, whether it is active or parked.
/// `None` when the tab is gone.
pub(crate) fn with_session_mut<T>(
    viewer: &Viewer,
    id: TabId,
    edit: impl FnOnce(&mut DocumentSession) -> T,
) -> Option<T> {
    if viewer.tabs.borrow().list.active() == Some(id) {
        return viewer.state.borrow_mut().session.as_mut().map(edit);
    }
    viewer
        .tabs
        .borrow_mut()
        .parked
        .get_mut(&id)
        .map(|parked| edit(&mut parked.session))
}

/// Activates tab `id`, then runs `then`. `Err` with the reason, and nothing
/// run, when the switch is refused — at once, never later: a switch carried
/// out after a wait would park whatever the user did meanwhile.
///
/// An open retype box is committed first, like any other edit. When that
/// commit starts a preview refresh, the refresh holds the tab and the switch
/// is refused like any other while it runs; the text is in the model.
fn activate_then(
    viewer: &Viewer,
    id: TabId,
    arrival: Arrival,
    then: Box<dyn FnOnce(&Viewer)>,
) -> Result<(), String> {
    {
        let tabs = viewer.tabs.borrow();
        if tabs.list.active() == Some(id) {
            drop(tabs);
            then(viewer);
            return Ok(());
        }
        if !tabs.parked.contains_key(&id) {
            return Err(tr("tabs-gone"));
        }
    }
    if let Some(reason) = busy_refusal(viewer) {
        return Err(reason);
    }
    if !crate::app::content_edit::editor::commit_open_retype(viewer) {
        return Err(tr("tabs-finish-editing"));
    }
    if let Some(reason) = busy_refusal(viewer) {
        return Err(reason);
    }
    let started = crate::app::perf::now();
    park::park_active(viewer);
    park::unpark(viewer, id, arrival);
    trace_switch(viewer, started);
    then(viewer);
    Ok(())
}

/// Reports a tab switch that began at `started` (`perf`): the time to park
/// one session and put the other on screen, and the start of the wait for
/// its first painted page. Nothing when the traces are off.
fn trace_switch(viewer: &Viewer, started: Option<std::time::Instant>) {
    if started.is_none() {
        return;
    }
    let Some((document, page_count)) = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .map(|session| (session.document, session.pages.len()))
    else {
        return;
    };
    crate::app::perf::finish("tab.switch", started, &[("pages", page_count.into())]);
    crate::app::perf::mark(crate::app::perf::Mark::TabSwitch(document), started);
}

/// Whether a parked tab holds changes not written to disk.
pub(crate) fn has_dirty_parked_tab(viewer: &Viewer) -> bool {
    first_dirty_parked(viewer).is_some()
}

/// The first parked tab, in strip order, with unsaved changes.
fn first_dirty_parked(viewer: &Viewer) -> Option<TabId> {
    let tabs = viewer.tabs.borrow();
    tabs.list.ids().into_iter().find(|id| {
        tabs.parked
            .get(id)
            .is_some_and(|parked| parked.session.unsaved_to_disk)
    })
}

/// The tab holding `path`, read against every tab's *current* path — a
/// Save As moves a document, so paths are refreshed from the sessions
/// rather than remembered from the open.
fn find_open_tab(viewer: &Viewer, path: &Path) -> Option<TabId> {
    refresh_paths(viewer);
    viewer.tabs.borrow().list.find_by_path(&normalised(path))
}

/// Whether a tab other than the active one holds `path` — where a Save As
/// must not write, since that tab would go on showing and saving bytes the
/// file no longer has.
pub(crate) fn path_held_by_other_tab(viewer: &Viewer, path: &Path) -> bool {
    let active = viewer.tabs.borrow().list.active();
    held_by_another(viewer, path, active)
}

/// Whether a tab other than `tab` holds `path` — for a document landing in
/// `tab`, active or not, that is about to claim it.
pub(crate) fn path_held_by_other_tab_than(viewer: &Viewer, path: &Path, tab: TabId) -> bool {
    held_by_another(viewer, path, Some(tab))
}

/// Whether a tab other than `except` holds `path`, read against every tab's
/// current path.
fn held_by_another(viewer: &Viewer, path: &Path, except: Option<TabId>) -> bool {
    refresh_paths(viewer);
    viewer
        .tabs
        .borrow()
        .list
        .holders(&normalised(path))
        .into_iter()
        .any(|id| Some(id) != except)
}

/// A file a write is putting on disk right now, held for as long as the
/// write runs (`write::worker::spawn_write`). Opening it meanwhile would read
/// half a file, so [`open::activate_open_path`] refuses it.
pub(crate) struct DestinationClaim {
    claims: Rc<RefCell<Vec<(u64, PathBuf)>>>,
    id: u64,
}

impl Drop for DestinationClaim {
    fn drop(&mut self) {
        self.claims.borrow_mut().retain(|(id, _)| *id != self.id);
    }
}

/// Claims `path` as being written, until the claim is dropped.
pub(crate) fn claim_destination(viewer: &Viewer, path: &Path) -> DestinationClaim {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let claims = Rc::clone(&viewer.tabs.borrow().claims);
    claims.borrow_mut().push((id, normalised(path)));
    DestinationClaim { claims, id }
}

/// Whether a write is putting `path` on disk right now.
fn being_written(viewer: &Viewer, path: &Path) -> bool {
    let target = normalised(path);
    let claims = Rc::clone(&viewer.tabs.borrow().claims);
    let held = claims
        .borrow()
        .iter()
        .any(|(_, claimed)| *claimed == target);
    held
}

fn refresh_paths(viewer: &Viewer) {
    let mut paths = Vec::new();
    for_each_session(viewer, |id, session, _| {
        paths.push((id, session.origin_path.as_deref().map(normalised)));
    });
    let mut tabs = viewer.tabs.borrow_mut();
    for (id, path) in paths {
        tabs.list.set_path(id, path);
    }
}

/// `path` as the tab list compares it: resolved when it exists, so two
/// spellings of one file (a relative path, a symlink) find the same tab.
fn normalised(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
