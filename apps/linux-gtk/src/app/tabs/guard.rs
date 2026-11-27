//! What holds the active tab in place, and what binds a dialog to the tab it
//! was opened for.
//!
//! Two answers to two questions. [`busy_refusal`] says whether the active
//! tab may be left *now* — counting [`JobGuard`]s, [`PromptGuard`]s and
//! visible modal windows — and so whether a user switch or close goes ahead
//! and whether an open may land in front. [`BoundTab`] (and the
//! [`ChooserScope`] built on it) says, when a dialog is finally answered,
//! whether the tab it was opened for is still the one on screen: a switch
//! the user made in between would otherwise make it act on whichever
//! document happens to be active.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;

use super::{active_tab, TabId};
use crate::app::i18n::tr;
use crate::app::state::Viewer;

/// Holds the active tab in place while a dialog that acts on it is open but
/// owns no window of its own the check below can see — a file chooser
/// (which may be the desktop portal's), the unsaved-changes prompt, the
/// import-warnings question.
///
/// Counted rather than flagged, since one can open another (the prompt's
/// Save opens the chooser), and released on drop, so every way a dialog
/// ends — a button, Escape, its window closing — lets go of it.
pub(crate) struct PromptGuard(Rc<Cell<u32>>);

impl PromptGuard {
    pub(crate) fn hold(viewer: &Viewer) -> Self {
        let count = Rc::clone(&viewer.blocking_prompts);
        count.set(count.get() + 1);
        Self(count)
    }
}

impl Drop for PromptGuard {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_sub(1));
    }
}

/// Holds the active tab in place while a job started on its document — a
/// save, sign or protect write, a compression, an extraction, a split, an
/// image export, a print snapshot — runs to its continuation.
///
/// Every such job ends by reading the window back (its status line, the
/// session it was started on); a switch or a front landing in between would
/// retire the session it checks against and drop its result. One counter for
/// all of them, released on drop, so every exit path lets go.
pub(crate) struct JobGuard(Rc<Cell<u32>>);

impl JobGuard {
    pub(crate) fn hold(viewer: &Viewer) -> Self {
        let count = Rc::clone(&viewer.jobs_in_flight);
        count.set(count.get() + 1);
        Self(count)
    }
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_sub(1));
    }
}

/// Why the active tab cannot be left right now, or `None` when it can.
///
/// Work in flight is read off the window state. Dialogs are found the one
/// way that cannot fall out of step with the code that opens them: any
/// visible modal window transient for this one — a password prompt, a
/// signing identity, Protect, Compress, Split, a confirmation — holds the
/// tab, however it was opened and however it will close. A [`PromptGuard`]
/// covers the rest.
pub(crate) fn busy_refusal(viewer: &Viewer) -> Option<String> {
    {
        let state = viewer.state.borrow();
        if state.preview_refresh_in_flight
            || viewer.jobs_in_flight.get() > 0
            || state.import_cancellation.is_some()
        {
            return Some(tr("tabs-busy"));
        }
    }
    let dialog_open = viewer.blocking_prompts.get() > 0 || modal_dialog_open(viewer);
    dialog_open.then(|| tr("tabs-dialog-open"))
}

/// Whether a visible modal window is transient for this viewer's window.
fn modal_dialog_open(viewer: &Viewer) -> bool {
    let Some(window) = viewer.window() else {
        return false;
    };
    let window: gtk::Window = window.upcast();
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
        .any(|dialog| {
            dialog.is_visible()
                && dialog.is_modal()
                && dialog.transient_for().as_ref() == Some(&window)
        })
}

/// The tab a dialog was opened for.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BoundTab(Option<TabId>);

/// Binds whatever is about to ask a question to the active tab.
pub(crate) fn bind_active(viewer: &Viewer) -> BoundTab {
    BoundTab(active_tab(viewer))
}

impl BoundTab {
    /// Whether the tab this was bound to is still the active one — or, for a
    /// question asked with no document open, whether there is still none.
    /// When not, reports the refusal on the status line: the caller then does
    /// nothing, rather than act on a document the user did not ask about.
    pub(crate) fn ensure_active_or_report(self, viewer: &Viewer) -> bool {
        let still = active_tab(viewer) == self.0;
        if !still {
            viewer.refuse(&tr("tabs-document-changed"));
        }
        still
    }
}

/// What every file chooser in the shell holds while it is up: the active tab
/// in place ([`PromptGuard`] — a desktop-portal chooser is no window the
/// modal check can see), and the tab the answer is for.
///
/// Created right before a `FileDialog` opens and moved into its callback,
/// whose first act is [`ChooserScope::settle`].
pub(crate) struct ChooserScope {
    guard: PromptGuard,
    bound: Option<BoundTab>,
}

impl ChooserScope {
    /// For a chooser whose answer acts on the active document.
    pub(crate) fn for_active(viewer: &Viewer) -> Self {
        Self {
            guard: PromptGuard::hold(viewer),
            bound: Some(bind_active(viewer)),
        }
    }

    /// For a chooser whose answer acts on no open document — opening files.
    pub(crate) fn unbound(viewer: &Viewer) -> Self {
        Self {
            guard: PromptGuard::hold(viewer),
            bound: None,
        }
    }

    /// The answer is in: lets go of the tab, and answers whether the answer
    /// may still be acted on — reporting the refusal when it may not.
    pub(crate) fn settle(self, viewer: &Viewer) -> bool {
        drop(self.guard);
        self.bound
            .is_none_or(|bound| bound.ensure_active_or_report(viewer))
    }
}
