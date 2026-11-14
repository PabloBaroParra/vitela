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
pub(crate) struct Shown(Rc<Cell<Option<Key>>>);

impl Shown {
    pub(super) fn set(&self, key: Option<Key>) {
        self.0.set(key);
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
            view.buffer().connect_mark_set({
                let viewer = viewer.clone();
                move |buffer, _, mark| {
                    if *mark == buffer.get_insert() || *mark == buffer.selection_bound() {
                        selection_moved(&viewer);
                    }
                }
            });
        }
    }
}

/// Redraws the buttons when the target they would show is not the one they
/// show. A move made while the state is borrowed for writing (the editor
/// being set up or torn down) is looked at once that borrow is over.
fn selection_moved(viewer: &Viewer) {
    let Ok(state) = viewer.state.try_borrow() else {
        let viewer = viewer.clone();
        glib::idle_add_local_once(move || selection_moved(&viewer));
        return;
    };
    let now = super::shown_target(&state).as_ref().map(key);
    drop(state);
    if now != viewer.style_toggles.shown.0.get() {
        super::update(viewer);
    }
}
