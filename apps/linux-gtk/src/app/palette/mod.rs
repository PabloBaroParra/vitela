//! Command palette (Ctrl+K), batch B27 — see `docs/batch-command-palette.md`.
//!
//! `matcher` scores a query, `entries` is the catalog and runs an entry,
//! `recent` remembers what ran this session, `view` builds the widgets.
//! This module owns the behaviour in between: opening over the window,
//! the keys, running an entry, and handing the cursor back on close.

mod entries;
mod matcher;
mod recent;
mod view;

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib, ApplicationWindow, EventSequenceState, Overlay, PropagationPhase};

use crate::app::state::Viewer;
use entries::{Availability, PaletteEntry};
use recent::Recent;
use view::{Key, View};

pub(crate) use view::PALETTE_CSS;

/// The palette over one window. Cloned into its own signal handlers; every
/// clone is the same palette.
#[derive(Clone)]
pub(crate) struct Palette {
    inner: Rc<Inner>,
}

struct Inner {
    view: View,
    window: ApplicationWindow,
    viewer: Viewer,
    /// The rows on screen, in order, with the availability each was drawn
    /// with.
    shown: RefCell<Vec<(PaletteEntry, Availability)>>,
    recent: RefCell<Recent>,
    /// The widget that had the cursor when the palette opened.
    previous_focus: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
}

/// Builds the palette, hidden, as an overlay of `root` — the overlay that
/// holds the window's whole content — and wires its own signals. The
/// `win.command-palette` action that opens it is registered with the other
/// window shortcuts (`connect_standard_shortcuts`, decision 6).
pub(crate) fn install(window: &ApplicationWindow, viewer: &Viewer, root: &Overlay) -> Palette {
    let palette = Palette {
        inner: Rc::new(Inner {
            view: view::build(),
            window: window.clone(),
            viewer: viewer.clone(),
            shown: RefCell::new(Vec::new()),
            recent: RefCell::new(Recent::default()),
            previous_focus: RefCell::new(None),
        }),
    };
    let view = &palette.inner.view;
    root.add_overlay(&view.layer);

    view.search.connect_changed({
        let palette = palette.clone();
        move |_| palette.refill()
    });
    view.search.connect_activate({
        let palette = palette.clone();
        move |_| palette.activate_selected()
    });
    view.list.connect_row_activated({
        let palette = palette.clone();
        move |_, row| palette.activate(row_index(row))
    });

    // Capture, on the card: the arrows and Esc are the palette's before the
    // search field's text handling sees them.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(PropagationPhase::Capture);
    keys.connect_key_pressed({
        let palette = palette.clone();
        move |_, key, _, _| palette.handle_key(key)
    });
    view.card.add_controller(keys);

    // A press anywhere on the layer that is not on the card closes it.
    let outside = gtk::GestureClick::new();
    outside.set_button(0);
    outside.connect_pressed({
        let palette = palette.clone();
        move |gesture, _, x, y| {
            let view = &palette.inner.view;
            let on_card = view
                .layer
                .pick(x, y, gtk::PickFlags::DEFAULT)
                .is_some_and(|widget| widget == view.card || widget.is_ancestor(&view.card));
            if !on_card {
                gesture.set_state(EventSequenceState::Claimed);
                palette.close();
            }
        }
    });
    view.layer.add_controller(outside);

    palette
}

/// Records whether the palette is up and recomputes the one action that
/// shares its key: Esc's `win.clear-content-selection` is off while it is,
/// so the first Esc closes the palette and leaves a content selection alone.
/// Every way of closing (Esc, Ctrl+K, a click outside, running an entry)
/// goes through [`Palette::close`], so the action always comes back.
fn set_open(viewer: &Viewer, open: bool) {
    viewer.state.borrow_mut().palette_open = open;
    crate::app::content_edit::multi::update_clear_action(viewer);
}

fn row_index(row: &gtk::ListBoxRow) -> usize {
    usize::try_from(row.index()).unwrap_or(usize::MAX)
}

impl Palette {
    pub(crate) fn is_open(&self) -> bool {
        self.inner.view.layer.is_visible()
    }

    /// What Ctrl+K does: opens the palette, or closes it when it is open.
    pub(crate) fn toggle(&self) {
        if self.is_open() {
            self.close();
        } else {
            self.open();
        }
    }

    /// Shows the palette with an empty query and the cursor in its search
    /// field, remembering who had the cursor before.
    pub(crate) fn open(&self) {
        if self.is_open() {
            return;
        }
        let inner = &self.inner;
        *inner.previous_focus.borrow_mut() =
            RootExt::focus(&inner.window).map(|widget| widget.downgrade());
        inner.view.search.set_text("");
        // `set_text` on an already empty field changes nothing and so
        // refills nothing; the list must still be the current one.
        self.refill();
        inner.view.layer.set_visible(true);
        set_open(&inner.viewer, true);
        inner.view.search.grab_focus();
    }

    /// Hides the palette and gives the cursor back explicitly.
    ///
    /// The cursor moves **before** the layer is hidden: hiding the focused
    /// search field would leave GTK to pick the next focus by itself, and
    /// the canvas viewport scrolls to reveal whatever it picks — the jump
    /// the content editor's close once had (`content_edit::editor`'s
    /// `release_focus`). It goes back to the widget that had it; if that one
    /// is gone or off screen, to nothing at all, which is the shell's
    /// existing safe answer (`release_focus` does the same). Both canvas
    /// adjustments are pinned across the grab as well, as
    /// `focus_without_scrolling` does for the editor.
    pub(crate) fn close(&self) {
        if !self.is_open() {
            return;
        }
        let inner = &self.inner;
        let previous = inner
            .previous_focus
            .take()
            .and_then(|weak| weak.upgrade())
            .filter(|widget| {
                widget.is_mapped()
                    && widget
                        .root()
                        .is_some_and(|root| root == *inner.window.upcast_ref::<gtk::Root>())
                    && !widget.is_ancestor(&inner.view.layer)
            });
        let scroll = &inner.viewer.scroll;
        let (horizontal, vertical) = (scroll.hadjustment(), scroll.vadjustment());
        let (left, top) = (horizontal.value(), vertical.value());
        if !previous.is_some_and(|widget| widget.grab_focus()) {
            RootExt::set_focus(&inner.window, None::<&gtk::Widget>);
        }
        horizontal.set_value(left);
        vertical.set_value(top);
        inner.view.layer.set_visible(false);
        set_open(&inner.viewer, false);
    }

    /// Rebuilds the list for the current query: ranked by the matcher, or,
    /// with an empty query, this session's commands first and then the
    /// catalog (decision 8). The first row is selected.
    fn refill(&self) {
        let inner = &self.inner;
        let query = inner.view.search.text();
        let found: Vec<(PaletteEntry, Availability)> = entries::search(&inner.viewer, &query)
            .into_iter()
            .map(|(entry, availability, _)| (entry, availability))
            .collect();
        let found = if query.trim().is_empty() {
            inner.recent.borrow().order(found, |(entry, _)| entry.id)
        } else {
            found
        };

        let list = &inner.view.list;
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        for (entry, availability) in &found {
            list.append(&view::row(entry, availability));
        }
        inner.view.empty.set_visible(found.is_empty());
        inner.view.scroller.set_visible(!found.is_empty());
        *inner.shown.borrow_mut() = found;
        self.select(0);
    }

    fn select(&self, index: usize) {
        let view = &self.inner.view;
        let Some(row) = i32::try_from(index)
            .ok()
            .and_then(|index| view.list.row_at_index(index))
        else {
            return;
        };
        view.list.select_row(Some(&row));
        view::reveal(view, &row);
    }

    fn selected(&self) -> Option<usize> {
        self.inner
            .view
            .list
            .selected_row()
            .map(|row| row_index(&row))
    }

    /// Moves the selection `step` rows, stopping at either end. Refused rows
    /// are selectable like any other: selecting one is how its reason is
    /// read.
    fn move_selection(&self, step: isize) {
        let len = self.inner.shown.borrow().len();
        let Some(last) = len.checked_sub(1) else {
            return;
        };
        let current = self.selected().unwrap_or(0);
        let next = current.saturating_add_signed(step).min(last);
        self.select(next);
    }

    fn handle_key(&self, key: gdk::Key) -> glib::Propagation {
        match view::key_action(key) {
            Some(Key::Up) => self.move_selection(-1),
            Some(Key::Down) => self.move_selection(1),
            Some(Key::Close) => self.close(),
            None => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    /// Enter: runs the selected row.
    fn activate_selected(&self) {
        if let Some(index) = self.selected() {
            self.activate(index);
        }
    }

    /// Runs the row at `index`. A refused row does nothing and the palette
    /// stays open, its reason on screen. Otherwise the palette closes
    /// **first**, cursor handed back, so a flow that opens a dialog or a
    /// chooser, or switches screens, starts from the window as it was; and
    /// the entry counts as recent only once it actually ran.
    fn activate(&self, index: usize) {
        let Some((entry, availability)) = self.inner.shown.borrow().get(index).cloned() else {
            return;
        };
        if let Availability::Refused(_) = availability {
            return;
        }
        self.close();
        match entry.run(&self.inner.window, &self.inner.viewer) {
            Ok(()) => self.inner.recent.borrow_mut().record(entry.id),
            // Refused between drawing the row and running it: say so where
            // the shell reports everything else.
            Err(reason) => self.inner.viewer.status.set_text(&reason),
        }
    }
}

#[cfg(test)]
mod tests;
