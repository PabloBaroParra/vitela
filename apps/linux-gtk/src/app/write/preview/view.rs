//! Where the user was *looking*, carried across a preview refresh's reopen:
//! the screen on show, the zoom, and the reading position.
//!
//! The other half of what [`super::refresh_preview`] lifts over
//! `document::show_document` — see [`super::edits`] for what the user had
//! edited, and for why each of these is a `take`/`restore` pair.
//!
//! These three are also the reason the restores have an order.
//! [`restore_screen`] runs last because the stack only hands allocations to
//! the page it is showing, and [`restore_view_state`] reads them.

use gtk::glib;
use gtk::prelude::*;

use crate::app::document::is_current;
use crate::app::state::Viewer;

/// Reads which view-stack page is on show, so [`refresh_preview`](super::refresh_preview) can put it
/// back after the rebuild.
///
/// `document::show_document` always lands on the editor page, which is right for an
/// open — the user asked for a document and a document is what they get — and
/// wrong for a refresh, which rebuilds the document already on screen rather
/// than navigating anywhere. Every page operation run from the Organize
/// screen goes `organize::command` -> [`refresh_preview`](super::refresh_preview), so without this a
/// single reorder or delete threw the user back to the editor mid-organize
/// and made them re-enter the screen to move the next page.
pub(super) fn take_screen(viewer: &Viewer) -> Option<glib::GString> {
    viewer.view_stack.visible_child_name()
}

/// Puts [`take_screen`]'s result back.
///
/// Runs *after* [`restore_view_state`], not before: `show_document`'s
/// measuring and the zoom/scroll restore both read allocations the stack only
/// hands to the page it is showing, so the editor has to stay on show for the
/// whole rebuild and step aside only once it is done.
///
/// Unconditional on the session, unlike the two restores above it — a screen
/// is a widget, and where the user is standing in the app has no business
/// depending on whether a model survived the reopen.
pub(super) fn restore_screen(viewer: &Viewer, preserved: Option<glib::GString>) {
    let Some(screen) = preserved else {
        return;
    };
    if viewer.view_stack.visible_child_name().as_deref() != Some(screen.as_str()) {
        viewer.view_stack.set_visible_child_name(&screen);
    }
}

/// Where the user was looking, as opposed to what they had edited
/// ([`EditState`](super::edits::EditState)) or what is being rendered.
///
/// `document::show_document` resets both halves for the same reason: a *different*
/// document has no business inheriting the previous one's zoom or scroll
/// position. Re-showing the same one does.
#[derive(Clone, Copy)]
pub(super) struct ViewState {
    zoom: crate::app::layout::Zoom,
    reading: crate::app::layout::ReadingPosition,
}

/// Reads the current zoom and reading position. Pure observation — unlike
/// `edits::take_edit_state` there is nothing to move out, because
/// `document::show_document` rebuilds these from scratch rather than carrying them.
pub(super) fn take_view_state(viewer: &Viewer) -> Option<ViewState> {
    // Read before the borrow: `vadjustment()` touches the widget tree, not
    // `viewer.state`, but keeping the two apart is what lets the borrow below
    // stay as short as it is.
    let offset = viewer.scroll.vadjustment().value();
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    Some(ViewState {
        zoom: session.zoom,
        reading: crate::app::layout::reading_position(&session.page_heights, offset),
    })
}

/// Puts the zoom and reading position back after `document::show_document` has reset
/// them to "freshly opened": fit-width, scrolled to the top.
///
/// Order matters. `set_zoom` recomputes every page's box and with it
/// `page_heights`, so the reading position must be resolved *after* it —
/// against the stacking the user will actually be scrolling through, not the
/// fit-width one `show_document` left behind.
pub(super) fn restore_view_state(viewer: &Viewer, generation: u64, preserved: Option<ViewState>) {
    let Some(preserved) = preserved else {
        return;
    };
    // A no-op when the zoom already matches `show_document`'s fresh default;
    // a full `refresh_layout` when it does not (see its own guard).
    crate::app::layout::set_zoom(viewer, preserved.zoom);

    let target = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return;
        };
        crate::app::layout::position_offset(&session.page_heights, preserved.reading)
    };
    if target <= 0.0 {
        return;
    }

    // The borrow above must end before this: `set_value` synchronously emits
    // `value_changed`, whose handler borrows the state again — the same
    // sequencing `search::scroll_to_current_match` documents.
    viewer.scroll.vadjustment().set_value(target);

    // Then again on the next idle, because one attempt cannot be enough here.
    // `set_value` clamps to the adjustment's `upper`, and `upper` only tracks
    // the page widgets `show_document` just rebuilt as of the next
    // size-allocate — so the call above lands when the adjustment still holds
    // the pre-rebuild extent (the common case: same document, same page
    // sizes, so the old extent is also the right one) and is clamped short
    // when it does not. Whether GTK clamps synchronously or on its next
    // configure is not something worth depending on, so this re-asserts
    // rather than testing for it. Setting it eagerly first is what keeps the
    // restore from visibly flickering through the top of page 1.
    glib::idle_add_local_once({
        let viewer = viewer.clone();
        move || {
            // A document opened in the meantime owns the scroll position now;
            // this one's is stale.
            if !is_current(&viewer, generation) {
                return;
            }
            let adjustment = viewer.scroll.vadjustment();
            // Only ever scrolls further down, never back up: if the eager
            // call already landed, this is a no-op, and if something else
            // moved past `target` in between, that is more current than what
            // this closure captured.
            if adjustment.value() < target {
                adjustment.set_value(target);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{restore_screen, take_screen};
    use crate::app::home::{show_editor, EDITOR_PAGE};
    use crate::app::organize::ORGANIZE_PAGE;
    use crate::app::ui_tests::built_ui;
    use gtk::prelude::*;

    /// A preview refresh rebuilds the document that is already on screen; it
    /// is not a navigation, so it must leave the user on the screen they were
    /// using. `show_document` — which every refresh runs through — always
    /// switches to the editor page, and that is what used to eject the user
    /// from Organize on every single page move.
    #[gtk::test]
    fn gtk_ui_a_rebuild_leaves_the_user_on_the_screen_they_were_using() {
        let built = built_ui();
        built
            .viewer
            .view_stack
            .set_visible_child_name(ORGANIZE_PAGE);

        let preserved = take_screen(&built.viewer);
        // Stands in for `show_document`, whose one effect on the view stack
        // this is; the rest of it needs an opened pdfium document.
        show_editor(&built.viewer);
        assert_eq!(
            built.viewer.view_stack.visible_child_name().as_deref(),
            Some(EDITOR_PAGE),
            "the rebuild is expected to land on the editor — that is the behavior being undone"
        );
        restore_screen(&built.viewer, preserved);

        assert_eq!(
            built.viewer.view_stack.visible_child_name().as_deref(),
            Some(ORGANIZE_PAGE)
        );
        built.window.close();
    }

    /// The editor is the common case, and putting it back must be a no-op
    /// rather than a second switch to the page already on show.
    #[gtk::test]
    fn gtk_ui_a_rebuild_started_from_the_editor_stays_on_the_editor() {
        let built = built_ui();
        built.viewer.view_stack.set_visible_child_name(EDITOR_PAGE);

        let preserved = take_screen(&built.viewer);
        show_editor(&built.viewer);
        restore_screen(&built.viewer, preserved);

        assert_eq!(
            built.viewer.view_stack.visible_child_name().as_deref(),
            Some(EDITOR_PAGE)
        );
        built.window.close();
    }
}
