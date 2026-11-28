//! Moving documents between the window and parked tabs: parking the active
//! one, parking a freshly opened one without ever showing it, and bringing
//! a parked one back.

use gtk::prelude::*;

use super::{normalised, Arrival, ParkedTab, TabId};
use crate::app::document::{
    build_pages, cancel_renders, clear_page_widgets, install_session, parked_session, BuiltPages,
    PageShape,
};
use crate::app::i18n::tr_args;
use crate::app::layout::Zoom;
use crate::app::organize::ORGANIZE_PAGE;
use crate::app::state::{DocumentSession, OpenedDocument, Viewer};
use crate::app::write::{restore_view_state, take_view_state};

/// Parks the active document in its tab and leaves the window with none.
///
/// Its renders are cancelled and its page widgets removed; everything the
/// user made — the edit log, the selections, the zoom — goes with the
/// session. Bumps `generation` and `session_id`, so whatever was still in
/// flight for it is dropped when it lands. A session no tab knows about (a
/// test that installs one directly) gets a tab here.
///
/// `None` when there was no active document to park.
pub(super) fn park_active(viewer: &Viewer) -> Option<TabId> {
    // Before the session leaves the window: where it was being read is
    // remembered for the next run too.
    crate::app::session_state::remember_active(viewer);
    let view = take_view_state(viewer);
    let mut session = viewer.state.borrow_mut().session.take()?;
    cancel_renders(&session);
    session.active.clear();
    session.active_tiles.clear();
    forget_gestures(&mut session);
    let shapes = shapes_of(&session);
    let path = session.origin_path.as_deref().map(normalised);
    let parked_at = super::retire_counters(viewer);
    let id = {
        let mut tabs = viewer.tabs.borrow_mut();
        let id = match tabs.list.active() {
            Some(id) => id,
            None => tabs.list.add(None),
        };
        tabs.list.set_path(id, path);
        tabs.list.deactivate();
        tabs.parked.insert(
            id,
            ParkedTab {
                session,
                shapes,
                view,
                parked_at,
            },
        );
        id
    };
    clear_page_widgets(viewer);
    Some(id)
}

/// Puts a freshly opened document straight into a new tab in the
/// background: the active tab, and whatever the user is doing in it, is not
/// touched. No page widget is built until the tab is first activated
/// ([`unpark`]); everything else — the model, the save backing, the unsaved
/// flag recovery reads — is the session a foreground open would install.
pub(super) fn park_new(viewer: &Viewer, document: OpenedDocument) -> TabId {
    let (session, shapes) = parked_session(viewer, document);
    let name = session.base_name.clone();
    let path = session.origin_path.as_deref().map(normalised);
    let parked_at = {
        let state = viewer.state.borrow();
        (state.session_id, state.generation)
    };
    let id = {
        let mut tabs = viewer.tabs.borrow_mut();
        let id = tabs.list.add(path);
        tabs.parked.insert(
            id,
            ParkedTab {
                session,
                shapes,
                view: None,
                parked_at,
            },
        );
        id
    };
    super::sync::sync(viewer);
    viewer.success(&tr_args(
        "tabs-opened-in-background",
        &[("name", name.into())],
    ));
    id
}

/// Makes the parked tab `id` the active document again: rebuilds its page
/// widgets from its still-open pdfium handle, re-installs the session, and
/// puts back its zoom and reading position.
///
/// Where the user ends up depends on `arrival`. A strip click keeps the
/// screen on show — Organize too, unless this document refuses it. An open
/// of a file the tab already holds lands on the editor like any open, and
/// the tool a Home card armed for it applies last, over that screen.
///
/// The window must hold no session — [`park_active`] or a close ran first.
/// `false` when `id` is not parked.
pub(super) fn unpark(viewer: &Viewer, id: TabId, arrival: Arrival) -> bool {
    let parked = {
        let mut tabs = viewer.tabs.borrow_mut();
        let parked = tabs.parked.remove(&id);
        if parked.is_some() {
            tabs.list.activate(id);
        }
        parked
    };
    let Some(ParkedTab {
        mut session,
        shapes,
        view,
        ..
    }) = parked
    else {
        return false;
    };

    // The editor stays on show for the rebuild — the measuring in
    // `build_pages` and the scroll restore read allocations the stack only
    // hands the page it is showing — and the screen is put back after.
    let screen = viewer.view_stack.visible_child_name();
    crate::app::home::show_editor(viewer);
    clear_page_widgets(viewer);

    // The pdfium handle is the same one, so what was read off its pages is
    // still true of them: carried over rather than loaded again. A tab that
    // was parked on arrival has none to carry.
    let caches: Vec<_> = session
        .pages
        .drain(..)
        .map(|page| (page.characters, page.content))
        .collect();
    let BuiltPages {
        mut slots,
        page_heights,
        fit,
    } = build_pages(viewer, &shapes);
    for (slot, (characters, content)) in slots.iter_mut().zip(caches) {
        // A load that was in flight when the tab was parked may never land;
        // asking again is cheaper than a page that never learns its text.
        slot.characters_requested = characters.is_some();
        slot.characters = characters;
        slot.content = content;
    }
    session.pages = slots;
    session.page_heights = page_heights;
    session.physical_width = fit.available_width;
    session.physical_height = fit.available_height;
    session.scale_factor = fit.scale_factor;
    // The pages were just laid out at fit-width; the view restore below
    // takes them to the zoom the tab was left at.
    session.zoom = Zoom::FitWidth;
    session.zoom_generation += 1;
    session.last_visible = None;

    let (_, generation) = super::retire_counters(viewer);
    install_session(viewer, session);
    // The adjustment still holds the previous tab's offset; a tab never read
    // starts at the top, and a read one is scrolled from there.
    viewer.scroll.vadjustment().set_value(0.0);
    restore_view_state(viewer, generation, view);
    // A tab opened in the background has never been shown and has no view
    // of its own yet: like a document opened in front, it comes back where
    // an earlier run left it.
    if view.is_none() {
        crate::app::session_state::restore_view(viewer, generation);
    }
    // The arrows scroll the document that has just come on screen.
    crate::app::canvas_keys::focus_on_arrival(viewer);

    if arrival == Arrival::Strip {
        if let Some(screen) = screen {
            restore_screen(viewer, &screen);
        }
    }
    crate::app::home::apply_pending_tool(viewer);
    true
}

/// The drawn size and turn of every page `session` has widgets for.
fn shapes_of(session: &DocumentSession) -> Vec<PageShape> {
    session
        .pages
        .iter()
        .map(|page| PageShape {
            width_pt: page.width_pt,
            height_pt: page.height_pt,
            rotation: page.rotation,
        })
        .collect()
}

/// Puts `screen` back on show after the rebuild. Organize goes through the
/// same refusal its own door asks (`organize::open_refusal`); a document
/// that refuses it stays on the editor and says why.
fn restore_screen(viewer: &Viewer, screen: &str) {
    if viewer.view_stack.visible_child_name().as_deref() == Some(screen) {
        return;
    }
    if screen == ORGANIZE_PAGE {
        if let Some(refusal) = crate::app::organize::open_refusal(viewer) {
            viewer.refuse(&refusal);
            return;
        }
        viewer.view_stack.set_visible_child_name(screen);
        crate::app::organize::refresh_if_visible(viewer);
        return;
    }
    viewer.view_stack.set_visible_child_name(screen);
}

/// Drops every half-finished gesture and transient pointer state: they were
/// made against page widgets that are about to be destroyed. What the user
/// *chose* — the selected annotation, field, image or text block — stays.
fn forget_gestures(session: &mut DocumentSession) {
    session.placement = None;
    session.annotation_drag = None;
    session.form_placement = None;
    session.form_field_drag = None;
    session.image_drag = None;
    session.path_drag = None;
    session.text_drag = None;
    session.selection_gesture = None;
    session.content_pointer = None;
    session.content_hover_pointer = None;
    session.selection = None;
    session.search = None;
    session.last_visible = None;
    crate::app::content_edit::hover::forget(session);
}
