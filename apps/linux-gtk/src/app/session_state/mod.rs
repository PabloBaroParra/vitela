//! What the shell remembers between runs: the window's size and whether it
//! was maximized, which side panels were open and how wide the tools panel
//! was, and — for each of the last documents read — the page, the place on
//! it and the zoom.
//!
//! Three parts, one responsibility each:
//!
//! * [`model`] — the values, the file's text, the clamping. Pure.
//! * [`store`] — the file: where it is, reading it, replacing it atomically.
//! * this module — the window: applying what was remembered when it is
//!   built, noticing when something worth remembering changes, and writing.
//!
//! ## When it is written
//!
//! A change — a resize, a divider drag, a scroll, a zoom — starts one timer
//! ([`WRITE_DELAY`]) if none is running. When it fires, the window is read
//! once and, if anything differs from what was last written, the file is
//! replaced on a worker thread. A burst of changes is one write; a window
//! left alone writes nothing. The window closing writes once more,
//! directly, so the last second is not lost.
//!
//! Three moments do not wait for the timer, because by the time it fires
//! the document they concern is no longer the one on screen: a tab being
//! left (`tabs::park`), a tab being closed (`tabs::close`), and the window
//! closing. Each records the active document's view at once
//! ([`remember_active`]).
//!
//! ## What can go wrong
//!
//! Nothing that the user sees. A machine with no state directory, an
//! unreadable or damaged file and a write that fails all leave the shell
//! running on its defaults, with at most a line on standard error. No open
//! and no close waits on this module or asks it anything that can fail.
//!
//! ## Tests
//!
//! Off until [`install`], which only `run` calls — so a window built by a
//! test remembers nothing and starts no timer. The cases here turn it on
//! against a scratch file of their own ([`install_at`]).

mod model;
mod store;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gio, glib, ApplicationWindow, Paned, ToggleButton};

use model::{clamp_reading, DocumentView, Panels, RecordMark, State, WindowGeometry};
pub(crate) use store::Store;

use super::document::is_current;
use super::layout::{page_top, position_offset, ReadingPosition};
use super::side_panel::{self, Column};
use super::state::Viewer;
use super::write::{restore_view_state, take_view_state, ViewState};

/// How long after a change the file is written. Long enough that a drag or
/// a scroll is one write, short enough that a crash costs a moment's
/// movement.
const WRITE_DELAY: Duration = Duration::from_secs(1);

/// The widgets of the editor's layout whose state is remembered: the two
/// dividers and the toolbar toggles that say whether each column is open.
#[derive(Clone)]
pub(crate) struct Chrome {
    /// Pages column | everything else.
    pub(crate) pages_paned: Paned,
    /// Canvas | tools column.
    pub(crate) tools_paned: Paned,
    pub(crate) show_pages: ToggleButton,
    pub(crate) show_tools: ToggleButton,
}

/// The window's remembered state — `Viewer::remembered`. Cheap to clone:
/// every clone is the same state.
#[derive(Clone, Default)]
pub(crate) struct Remembered {
    inner: Rc<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Where it is written; `None` while nothing is remembered at all.
    store: RefCell<Option<Store>>,
    state: RefCell<State>,
    /// What is read to fill `state`. Weak: this must not keep a closed
    /// window alive.
    watched: RefCell<Option<(glib::WeakRef<ApplicationWindow>, Chrome)>>,
    /// The timer of a write that is due.
    pending: RefCell<Option<glib::SourceId>>,
    /// Counts the changes to `state`: what a write is stamped with, so
    /// writes that cross are told apart (`Store::save_revision`).
    revision: Cell<u64>,
    /// The newest revision already given to a writer.
    handed: Cell<u64>,
}

impl Remembered {
    fn enabled(&self) -> bool {
        self.inner.store.borrow().is_some()
    }
}

impl Inner {
    /// `state` now holds something no write has been given.
    fn changed(&self) {
        self.revision.set(self.revision.get() + 1);
    }

    /// Whether `state` holds something no write has been given.
    fn unwritten(&self) -> bool {
        self.revision.get() > self.handed.get()
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Turns remembering on for the application's window, applying what the
/// last run left. Called once, before the window is first shown.
///
/// Nothing happens on a machine with no usable state directory.
pub(crate) fn install(window: &ApplicationWindow, viewer: &Viewer, chrome: &Chrome) {
    if let Some(store) = Store::in_user_state_dir() {
        install_at(window, viewer, chrome, store);
    }
}

/// [`install`] against `store`.
pub(crate) fn install_at(
    window: &ApplicationWindow,
    viewer: &Viewer,
    chrome: &Chrome,
    store: Store,
) {
    let inner = &viewer.remembered.inner;
    let state = store.load();
    if let Some(geometry) = state.window {
        apply_window(window, geometry);
    }
    if let Some(panels) = state.panels {
        apply_panels(chrome, panels);
    }
    inner.state.replace(state);
    inner.store.replace(Some(store));
    inner
        .watched
        .replace(Some((window.downgrade(), chrome.clone())));
    watch(window, viewer, chrome);
}

fn apply_window(window: &ApplicationWindow, geometry: WindowGeometry) {
    window.set_default_size(geometry.width, geometry.height);
    if geometry.maximized {
        window.maximize();
    }
}

fn apply_panels(chrome: &Chrome, panels: Panels) {
    side_panel::restore(
        &chrome.tools_paned,
        Column::End,
        panels.tools_open,
        panels.tools_width,
    );
    side_panel::restore(&chrome.pages_paned, Column::Start, panels.pages_open, None);
}

/// Starts the timer on everything that changes what is remembered.
fn watch(window: &ApplicationWindow, viewer: &Viewer, chrome: &Chrome) {
    let later = || {
        let viewer = viewer.clone();
        move || schedule(&viewer)
    };
    window.connect_default_width_notify({
        let later = later();
        move |_| later()
    });
    window.connect_default_height_notify({
        let later = later();
        move |_| later()
    });
    window.connect_maximized_notify({
        let later = later();
        move |_| later()
    });
    for toggle in [&chrome.show_pages, &chrome.show_tools] {
        toggle.connect_toggled({
            let later = later();
            move |_| later()
        });
    }
    chrome.tools_paned.connect_position_notify({
        let later = later();
        move |_| later()
    });
    viewer.scroll.vadjustment().connect_value_changed({
        let later = later();
        move |_| later()
    });
    // The readout changes exactly when the effective zoom does.
    viewer.zoom_label.connect_label_notify({
        let later = later();
        move |_| later()
    });
    // The last word: the window is going, and no timer will fire after it.
    // `unrealize` rather than `destroy`: closing a window releases GTK's own
    // reference and unrealizes it, but the widget is only destroyed once
    // every handle to it is gone — and this shell holds several until exit.
    window.connect_unrealize({
        let viewer = viewer.clone();
        move |_| {
            flush(&viewer);
            shut_down(&viewer);
        }
    });
}

/// Starts the write timer unless one is already running.
fn schedule(viewer: &Viewer) {
    let inner = &viewer.remembered.inner;
    if !viewer.remembered.enabled() || inner.pending.borrow().is_some() {
        return;
    }
    let timer = glib::timeout_add_local_once(WRITE_DELAY, {
        let viewer = viewer.clone();
        move || {
            // The source is gone once it has run: forget its id before
            // anything can try to remove it.
            viewer.remembered.inner.pending.borrow_mut().take();
            write_in_background(&viewer);
        }
    });
    inner.pending.replace(Some(timer));
}

/// Reads the window into the state. `None` once the window is gone.
fn capture(viewer: &Viewer) -> Option<()> {
    let inner = &viewer.remembered.inner;
    let (window, chrome) = {
        let watched = inner.watched.borrow();
        let (window, chrome) = watched.as_ref()?;
        (window.upgrade()?, chrome.clone())
    };
    let geometry = window_geometry(&window, inner.state.borrow().window);
    let panels = panels(&chrome, inner.state.borrow().panels);
    {
        let mut state = inner.state.borrow_mut();
        if state.window != geometry || state.panels != panels {
            state.window = geometry;
            state.panels = panels;
            inner.changed();
        }
    }
    capture_document(viewer);
    Some(())
}

/// The window's size as it is worth coming back to.
///
/// GTK keeps the default size in step with the window while it is neither
/// maximized nor full screen, and leaves it alone while it is — so it is
/// the size the window returns to, which is the one to remember beside the
/// maximized flag. A window that was never given a size keeps what was
/// known before.
fn window_geometry(
    window: &ApplicationWindow,
    known: Option<WindowGeometry>,
) -> Option<WindowGeometry> {
    let (width, height) = (window.default_width(), window.default_height());
    if width <= 0 || height <= 0 {
        return known;
    }
    Some(WindowGeometry {
        width,
        height,
        maximized: window.is_maximized(),
    })
}

/// The side columns as they are. Before the editor has ever been laid out
/// the dividers say nothing true, and what was known stands.
fn panels(chrome: &Chrome, known: Option<Panels>) -> Option<Panels> {
    let total = chrome.tools_paned.width();
    if total <= 0 || chrome.pages_paned.width() <= 0 {
        return known;
    }
    let tools_open = chrome.show_tools.is_active();
    let measured = Column::End.width(chrome.tools_paned.position(), total);
    Some(Panels {
        pages_open: chrome.show_pages.is_active(),
        tools_open,
        // A closed column has no width of its own; the one it had is what
        // reopening it should come back to.
        tools_width: if tools_open && measured > 0 {
            Some(measured)
        } else {
            known.and_then(|known| known.tools_width)
        },
    })
}

/// Records where the active document is being read, when it came from a
/// file.
fn capture_document(viewer: &Viewer) {
    let Some(path) = active_path(viewer) else {
        return;
    };
    let Some(view) = take_view_state(viewer) else {
        return;
    };
    let inner = &viewer.remembered.inner;
    let changed = inner.state.borrow_mut().remember_document(
        &path,
        DocumentView {
            zoom: view.zoom(),
            reading: view.reading(),
            seen: now(),
        },
    );
    if changed {
        inner.changed();
    }
}

/// The file the active document came from. A copy restored after a crash
/// names one too, but was not read from it: where it was being read says
/// nothing about that file.
fn active_path(viewer: &Viewer) -> Option<PathBuf> {
    let state = viewer.state.try_borrow().ok()?;
    let session = state.session.as_ref()?;
    if session.unsaved_base || session.pages.is_empty() {
        return None;
    }
    session.origin_path.clone()
}

/// Records the active document's view now, ahead of the timer — for the
/// moments it is about to leave the screen.
pub(crate) fn remember_active(viewer: &Viewer) {
    if !viewer.remembered.enabled() {
        return;
    }
    capture_document(viewer);
    if viewer.remembered.inner.unwritten() {
        schedule(viewer);
    }
}

/// Where the document at `path`, now `page_count` pages long, was last
/// being read — as a view to restore. `None` when it is not remembered.
pub(crate) fn saved_view(
    viewer: &Viewer,
    path: Option<&Path>,
    page_count: usize,
) -> Option<ViewState> {
    if !viewer.remembered.enabled() {
        return None;
    }
    let saved = viewer.remembered.inner.state.borrow().document(path?)?;
    Some(ViewState::new(
        saved.zoom,
        clamp_reading(saved.reading, page_count)?,
    ))
}

/// Puts the document that has just been opened and shown back where it was
/// last being read. `generation` is the one it was shown under.
pub(crate) fn restore_view(viewer: &Viewer, generation: u64) {
    let saved = {
        let Ok(state) = viewer.state.try_borrow() else {
            return;
        };
        let Some(session) = state.session.as_ref() else {
            return;
        };
        saved_view(viewer, session.origin_path.as_deref(), session.pages.len())
    };
    let Some(view) = saved else {
        return;
    };
    restore_view_state(viewer, generation, Some(view));
    follow_layout(viewer, generation, view.reading());
}

/// Where `reading` is against the document's pages as they are laid out
/// now, and how tall those pages are together.
fn laid_out(viewer: &Viewer, reading: ReadingPosition) -> Option<(f64, f64)> {
    let state = viewer.state.try_borrow().ok()?;
    let heights = &state.session.as_ref()?.page_heights;
    let last = heights.len().checked_sub(1)?;
    Some((
        position_offset(heights, reading),
        page_top(heights, last) + f64::from(heights[last]),
    ))
}

/// Scrolls to `reading` once the scroller has caught up with the document.
///
/// `restore_view_state` scrolls at once and again on the next idle. That is
/// enough when the same document is being shown again — the scroller
/// already has room for where it was. A document just opened is another
/// matter: the scroller still has the extent of whatever was there before
/// — nothing at all, in a window that has just started — and clamps the
/// position to it both times. So the position is asked for once more, when
/// the scroller reports an extent that holds every page as now laid out,
/// and then left alone: one scroll, not a tug-of-war with the reader.
///
/// The position is worked out again at that moment rather than carried
/// over: a first open is measured before the window has its size, and the
/// pages are laid out again when it gets one.
fn follow_layout(viewer: &Viewer, generation: u64, reading: ReadingPosition) {
    let adjustment = viewer.scroll.vadjustment();
    // Already room for every page: the scroll above landed, and a handler
    // left waiting would only fire at some later resize, under the reader.
    let has_room =
        laid_out(viewer, reading).is_some_and(|(_, extent)| adjustment.upper() >= extent - 1.0);
    if has_room {
        return;
    }
    let handler = Rc::new(Cell::new(None));
    let id = adjustment.connect_changed({
        let viewer = viewer.clone();
        let handler = handler.clone();
        move |adjustment| {
            let placed = if is_current(&viewer, generation) {
                laid_out(&viewer, reading)
            } else {
                // Another document owns the scroller now.
                None
            };
            if let Some((_, extent)) = placed {
                if adjustment.upper() < extent - 1.0 {
                    return;
                }
            }
            if let Some(id) = handler.take() {
                adjustment.disconnect(id);
            }
            if let Some((target, _)) = placed {
                if adjustment.value() < target - 1.0 {
                    adjustment.set_value(target);
                }
            }
        }
    });
    handler.set(Some(id));
}

/// What a write's outcome changes: on success, the records it carried stop
/// outranking the file (`State::mark_written`); a failure is a log line and
/// leaves them winning, since nothing of them reached the disk.
fn settle_write(inner: &Inner, result: std::io::Result<RecordMark>) {
    match result {
        Ok(mark) => inner.state.borrow_mut().mark_written(mark),
        Err(error) => eprintln!("session state: could not be written: {error}"),
    }
}

/// The timer's write: off the main thread, so a slow disk is not a pause
/// in the window. Nothing when no change has come since the last one — and
/// a write that fails is not tried again until there is one, rather than
/// once a second for as long as the disk is full.
fn write_in_background(viewer: &Viewer) {
    let inner = &viewer.remembered.inner;
    let Some(store) = inner.store.borrow().clone() else {
        return;
    };
    capture(viewer);
    if !inner.unwritten() {
        return;
    }
    let revision = inner.revision.get();
    inner.handed.set(revision);
    let state = inner.state.borrow().clone();
    let remembered = viewer.remembered.clone();
    glib::spawn_future_local(async move {
        match gio::spawn_blocking(move || store.save_revision(state, revision)).await {
            Ok(result) => settle_write(&remembered.inner, result),
            Err(_) => eprintln!("session state: the write stopped unexpectedly"),
        }
    });
}

/// Writes now, on this thread, whatever the file does not hold yet: the
/// window is going, or a test wants the file. Cancels the timer, whose
/// write this is, and waits out a worker's write that is under way — the
/// file is current when this returns, or the failure has been logged.
pub(crate) fn flush(viewer: &Viewer) {
    let inner = &viewer.remembered.inner;
    if let Some(timer) = inner.pending.borrow_mut().take() {
        timer.remove();
    }
    let Some(store) = inner.store.borrow().clone() else {
        return;
    };
    capture(viewer);
    let revision = inner.revision.get();
    if revision <= store.written_revision() {
        return;
    }
    inner.handed.set(revision);
    let state = inner.state.borrow().clone();
    settle_write(inner, store.save_revision(state, revision));
}

/// Stops remembering: no more timers, no more writes. Like [`flush`], waits
/// out a worker's write that is under way, so nothing is writing the file
/// — or its folder — once this returns.
fn shut_down(viewer: &Viewer) {
    let inner = &viewer.remembered.inner;
    if let Some(timer) = inner.pending.borrow_mut().take() {
        timer.remove();
    }
    let store = inner.store.replace(None);
    inner.watched.replace(None);
    if let Some(store) = store {
        // Takes the write's turn, which a write under way holds until done.
        store.written_revision();
    }
}

#[cfg(test)]
mod tests;
