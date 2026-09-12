//! Which of the Organize screen's two views is on show, and the selector
//! that says so.
//!
//! Its own module rather than three helpers in [`super`]: "Documents or
//! Pages?" is asked by the selector, by [`super::show`] on the way in, and
//! by both refresh entry points, and every one of them must get the same
//! answer. Keeping the question, the widget that asks it and the two
//! repopulate calls together is what makes that hard to get wrong.

use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, Orientation, ToggleButton};

use crate::app::state::Viewer;

use super::documents::{self, DOCUMENTS_HINT, DOCUMENTS_VIEW, PAGES_HINT, PAGES_VIEW};
use super::grid::fill_grid;
use super::motion;

/// The `Documents | Pages` selector, and the two toggles for `OrganizePanel`.
///
/// Grouped toggles rather than a `StackSwitcher`: the switcher names its
/// buttons from the stack's own page titles and leaves this module no say in
/// their order or styling, and the two views are not peers of equal weight —
/// Organize opens on Documents, which is why that one starts active.
pub(super) fn build_switch() -> (GtkBox, ToggleButton, ToggleButton) {
    let switch = GtkBox::new(Orientation::Horizontal, 0);
    switch.add_css_class("linked");
    switch.add_css_class("organize-view-switch");
    switch.set_halign(gtk::Align::Start);

    let documents = ToggleButton::with_label("Documents");
    documents.set_active(true);
    let pages = ToggleButton::with_label("Pages");
    pages.set_group(Some(&documents));
    switch.append(&documents);
    switch.append(&pages);

    (switch, documents, pages)
}

/// Wires both toggles. Only the activating half of each acts: the group
/// deactivates the other one on its own, and reacting to that too would
/// populate the view being left.
pub(super) fn connect(viewer: &Viewer) {
    for (toggle, view) in [
        (&viewer.organize.documents_toggle, DOCUMENTS_VIEW),
        (&viewer.organize.pages_toggle, PAGES_VIEW),
    ] {
        toggle.connect_toggled({
            let viewer = viewer.clone();
            move |toggle| {
                if toggle.is_active() {
                    show(&viewer, view);
                }
            }
        });
    }
}

/// Switches the screen to one of its two views and fills it.
///
/// Populating on the way in rather than keeping both current is what lets a
/// refresh only ever pay for the view on show. What that costs is bounded
/// twice over: `super::cache` keeps a view the user has already been in from
/// asking pdfium for a single render again, and `grid::fill_grid` keeps it
/// from rebuilding the cards themselves when the page order has not moved.
///
/// This is also the one path that animates — see [`motion`]'s header for why
/// the entrance belongs to the view switch and not to `populate`.
pub(super) fn show(viewer: &Viewer, view: &str) {
    // Before the switch, so it acts on the view being *left*: its cards can
    // outlive the visit now, and a card that kept its entrance class would
    // arrive next time with the animation already spent. See
    // [`motion::clear_entrance`].
    motion::clear_entrance(viewer);
    viewer.organize.views.set_visible_child_name(view);
    viewer.organize.hint.set_text(if view == DOCUMENTS_VIEW {
        DOCUMENTS_HINT
    } else {
        PAGES_HINT
    });
    populate_visible(viewer);
    motion::run_entrance(viewer);
}

/// Fills whichever view is on show.
///
/// The Documents view is rebuilt outright every time and the Pages view only
/// when it has to be, which is not an inconsistency: a block list is one card
/// per imported PDF — two or three, on any document a user assembles by hand
/// — while the grid is one card per page, and four hundred of them is the
/// case `fill_grid` exists for.
pub(super) fn populate_visible(viewer: &Viewer) {
    if showing_documents(viewer) {
        documents::populate(viewer);
    } else {
        fill_grid(viewer);
    }
}

pub(super) fn showing_documents(viewer: &Viewer) -> bool {
    viewer.organize.views.visible_child_name().as_deref() == Some(DOCUMENTS_VIEW)
}

/// The line under the header describing the gesture the view on show offers.
pub(super) fn build_hint() -> Label {
    let hint = Label::new(Some(DOCUMENTS_HINT));
    hint.set_xalign(0.0);
    hint.add_css_class("recent-meta");
    hint
}
