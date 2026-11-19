//! Keeping the strip, its visibility and the window title in step with the
//! tabs and their documents.

use gtk::glib;
use gtk::prelude::*;

use super::strip::{self, TabView};
use super::{session_of, Tabs};
use crate::app::home::HOME_PAGE;
use crate::app::i18n::tr_args;
use crate::app::state::{DocumentSession, Viewer};

/// Wires the strip to the window: shown only on the screens that show a
/// document, and redrawn when a status message may report a change.
///
/// The status line is one of two ways a redraw is asked for: every command,
/// undo, redo and reopen also asks through
/// `annotations::update_annotation_controls`, which all of them reach, so the
/// unsaved marker does not depend on an edit happening to say something.
pub(crate) fn connect(viewer: &Viewer) {
    viewer.view_stack.connect_visible_child_name_notify({
        let viewer = viewer.clone();
        move |_| update_visibility(&viewer)
    });
    viewer.status.connect_label_notify({
        let viewer = viewer.clone();
        move |_| schedule_sync(&viewer)
    });
}

/// Rebuilds the strip, the window title and the strip's visibility from the
/// tabs and their sessions.
pub(super) fn sync(viewer: &Viewer) {
    let views = {
        let state = viewer.state.borrow();
        let tabs = viewer.tabs.borrow();
        tab_views(state.session.as_ref(), &tabs)
    };
    strip::render(viewer, &views);
    update_title(viewer, &views);
    update_visibility(viewer);
}

/// Queues one [`sync`] on the main loop, however many times it is asked for
/// before it runs. Deferred because the asker often still holds the window
/// state.
pub(crate) fn schedule_sync(viewer: &Viewer) {
    {
        let Ok(mut tabs) = viewer.tabs.try_borrow_mut() else {
            return;
        };
        if tabs.sync_scheduled {
            return;
        }
        tabs.sync_scheduled = true;
    }
    glib::idle_add_local_once({
        let viewer = viewer.clone();
        move || {
            viewer.tabs.borrow_mut().sync_scheduled = false;
            sync(&viewer);
        }
    });
}

/// What each tab shows, in strip order. A tab with no session — the active
/// one in the instant between parking and installing — is skipped.
fn tab_views(active_session: Option<&DocumentSession>, tabs: &Tabs) -> Vec<TabView> {
    let active = tabs.list.active();
    tabs.list
        .ids()
        .into_iter()
        .filter_map(|id| {
            let session = session_of(active_session, tabs, id)?;
            Some(TabView {
                id,
                name: session.base_name.clone(),
                path: session.origin_path.clone(),
                dirty: session.unsaved_to_disk,
                active: active == Some(id),
            })
        })
        .collect()
}

/// The strip is shown on the screens that show the active document — the
/// editor and Organize — and only once there is a document.
fn update_visibility(viewer: &Viewer) {
    let has_tabs = !viewer.tabs.borrow().list.is_empty();
    let on_home = viewer.view_stack.visible_child_name().as_deref() == Some(HOME_PAGE);
    viewer.tab_strip.root.set_visible(has_tabs && !on_home);
}

fn update_title(viewer: &Viewer, views: &[TabView]) {
    let Some(window) = viewer.window() else {
        return;
    };
    let title = views.iter().find(|view| view.active).map_or_else(
        || "Vitela".to_owned(),
        |view| tr_args("tabs-window-title", &[("name", view.name.clone().into())]),
    );
    window.set_title(Some(&title));
}
