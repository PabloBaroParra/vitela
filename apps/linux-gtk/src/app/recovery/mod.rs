//! Autosave and crash recovery.
//!
//! While a document has unsaved changes, its window periodically writes the
//! document's bytes — every pending edit applied — to a private recovery
//! directory under the user's state directory. A clean exit, a Save, or a
//! Discard deletes them; a crash does not, and the next launch offers to
//! restore them as the unsaved document they were.
//!
//! Split by responsibility: [`store`] is the directory on disk and nothing
//! else, [`policy`] is the rule each tick applies (and the rule that keeps
//! encrypted documents out entirely), [`autosave`] runs the timer and the
//! worker, and [`restore`] asks the question on startup and reopens the
//! answer.
//!
//! Recovery restores **document bytes only**. The undo history is not
//! recovered — the core has no serialization of the edit log — so a restored
//! document starts a fresh one, exactly as if its bytes had been opened.

mod autosave;
mod policy;
mod restore;
mod store;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::rc::Rc;

use gtk::ApplicationWindow;

use crate::app::state::Viewer;
pub(crate) use autosave::Autosave;
use store::RecoveryDir;

/// A window's autosave state — `Viewer::recovery`. Disabled until
/// [`install`] runs, which only the production startup does.
pub(crate) type Recovery = Rc<RefCell<Autosave>>;

/// Starts autosave for the application's window and offers whatever a
/// crashed session left behind. Called once, after the window is presented.
///
/// Nothing happens on a machine with no usable state directory: no entry can
/// be written, and none can have been.
pub(crate) fn install(window: &ApplicationWindow, viewer: &Viewer) {
    let Some(dir) = RecoveryDir::in_user_state_dir() else {
        return;
    };
    // Scanned before this window owns an entry, so its own is never offered.
    let orphans = dir.orphans();
    autosave::start(window, viewer, dir);
    restore::offer(window, viewer, orphans);
}

/// Deletes the window's recovery entry: its unsaved work was saved to disk,
/// or the document holding it was replaced after the user chose to discard.
pub(crate) fn forget(viewer: &Viewer) {
    autosave::forget(viewer);
}
