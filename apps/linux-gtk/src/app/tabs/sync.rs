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
    window.set_title(Some(&window_title(views)));
}

/// The window title for the tabs as drawn: the active document's name, with
/// a marker in front while it has unsaved changes, or the application's own
/// name when no document is open.
///
/// Read from the same [`TabView`]s the strip is drawn from, so the title and
/// the active tab cannot disagree about the name or about what is unsaved.
/// The marker is the strip's own, and it leads so it survives a task
/// switcher cutting a long title short.
fn window_title(views: &[TabView]) -> String {
    let Some(active) = views.iter().find(|view| view.active) else {
        return APPLICATION_NAME.to_owned();
    };
    let key = if active.dirty {
        "tabs-window-title-unsaved"
    } else {
        "tabs-window-title"
    };
    tr_args(key, &[("name", active.name.clone().into())])
}

/// The title of a window with no document. A name, not a sentence: the same
/// in every language.
const APPLICATION_NAME: &str = "Vitela";

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{window_title, TabView};
    use crate::app::i18n::{speak, Language};
    use crate::app::tabs::list::TabList;

    /// Three tabs — `clean.pdf`, `edited.pdf` (unsaved) and `other.pdf` —
    /// with the one at `active` in front.
    fn tabs_with_active(active: usize) -> Vec<TabView> {
        let mut list = TabList::default();
        [
            ("clean.pdf", false),
            ("edited.pdf", true),
            ("other.pdf", false),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (name, dirty))| TabView {
            id: list.add(None),
            name: name.to_owned(),
            path: Some(PathBuf::from("/home/user").join(name)),
            dirty,
            active: index == active,
        })
        .collect()
    }

    #[test]
    fn a_window_with_no_document_is_titled_with_the_application_name() {
        assert_eq!(window_title(&[]), "Vitela");
    }

    #[test]
    fn the_title_names_the_active_document() {
        assert_eq!(window_title(&tabs_with_active(0)), "clean.pdf — Vitela");
        assert_eq!(window_title(&tabs_with_active(2)), "other.pdf — Vitela");
    }

    #[test]
    fn unsaved_changes_in_the_active_document_mark_the_title() {
        assert_eq!(
            window_title(&tabs_with_active(1)),
            "\u{2022} edited.pdf — Vitela"
        );
    }

    /// Only the document in front speaks for the window: unsaved work in a
    /// background tab is the strip's to show.
    #[test]
    fn unsaved_changes_in_a_background_tab_do_not_mark_the_title() {
        assert_eq!(window_title(&tabs_with_active(0)), "clean.pdf — Vitela");
    }

    /// Tabs but none of them active — the instant between parking one
    /// document and installing the next.
    #[test]
    fn tabs_with_none_active_fall_back_to_the_application_name() {
        let mut views = tabs_with_active(0);
        views[0].active = false;

        assert_eq!(window_title(&views), "Vitela");
    }

    #[test]
    fn the_title_keeps_its_shape_in_spanish() {
        let _spanish = speak(Language::Spanish);

        assert_eq!(window_title(&tabs_with_active(0)), "clean.pdf — Vitela");
        assert_eq!(
            window_title(&tabs_with_active(1)),
            "\u{2022} edited.pdf — Vitela"
        );
    }
}
