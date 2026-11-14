//! Keeping the buttons in step with the open editor's selection.
//!
//! What the toggles show and act on narrows to the runs the editor's
//! selection covers (`selection`), so they are redrawn when the selection
//! moves, not only when the focus does. The caret and the selection move
//! on every key typed, and a redraw reads the page (`model::run_states`):
//! each move only works out which run the buttons would show, cheaply, and
//! redraws them when that is not what they show already.
//!
//! The handlers live on the editor's own field, so they go with it when
//! the editor closes, like the field's other handlers. They only read: a
//! redraw sets the buttons, never the field, so it cannot move the
//! selection again.

use std::cell::Cell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use pdf_document::{ContentItemId, PageId};

use crate::app::state::{EditorField, Viewer};

use super::{Scope, Target};

/// Which run the buttons show, and for what: the first targeted run is the
/// one whose state they draw, and the scope decides who has the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    scope: Scope,
    page: PageId,
    run: ContentItemId,
}

pub(super) fn key(target: &Target) -> Key {
    let first = &target.runs[0];
    Key {
        scope: target.scope,
        page: first.page,
        run: first.id,
    }
}

/// The key the buttons were last drawn for; `None` when they show no
/// target. Shared by every clone of the toggles.
#[derive(Clone, Default)]
pub(crate) struct Shown {
    key: Rc<Cell<Option<Key>>>,
    /// Whether a look once the state's borrow is over is already queued.
    retry: Rc<Cell<bool>>,
}

impl Shown {
    pub(super) fn set(&self, key: Option<Key>) {
        self.key.set(key);
    }

    #[cfg(test)]
    pub(super) fn retry_pending(&self) -> bool {
        self.retry.get()
    }
}

/// Redraws the buttons whenever `field`'s selection moves to another target.
pub(crate) fn wire(viewer: &Viewer, field: &EditorField) {
    match field {
        EditorField::Line(entry) => {
            entry.connect_cursor_position_notify({
                let viewer = viewer.clone();
                move |_| selection_moved(&viewer)
            });
            entry.connect_selection_bound_notify({
                let viewer = viewer.clone();
                move |_| selection_moved(&viewer)
            });
        }
        EditorField::Paragraph { view, .. } => {
            let buffer = view.buffer();
            buffer.connect_mark_set({
                let viewer = viewer.clone();
                move |buffer, _, mark| {
                    if *mark == buffer.get_insert() || *mark == buffer.selection_bound() {
                        selection_moved(&viewer);
                    }
                }
            });
            // Typing carries the caret along without setting a mark, and
            // the first character typed turns a partial selection into
            // `Scope::TextEdited`.
            buffer.connect_changed({
                let viewer = viewer.clone();
                move |_| selection_moved(&viewer)
            });
        }
    }
}

/// Redraws the buttons when the target they would show is not the one they
/// show. A move made while the state is borrowed for writing (the editor
/// being set up or torn down) is looked at once that borrow is over: one
/// look for however many moves were made meanwhile — a caret move alone
/// notifies twice, the caret and the selection bound.
fn selection_moved(viewer: &Viewer) {
    let Ok(state) = viewer.state.try_borrow() else {
        let retry = &viewer.style_toggles.shown.retry;
        if !retry.replace(true) {
            let viewer = viewer.clone();
            glib::idle_add_local_once(move || {
                viewer.style_toggles.shown.retry.set(false);
                selection_moved(&viewer);
            });
        }
        return;
    };
    let now = super::shown_target(&state).as_ref().map(key);
    drop(state);
    if now != viewer.style_toggles.shown.key.get() {
        super::update(viewer);
    }
}
