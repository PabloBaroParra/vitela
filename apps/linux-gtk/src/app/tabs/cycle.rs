//! Stepping through the document tabs with the keyboard: Ctrl+Tab and
//! Ctrl+Page Down for the next one, Ctrl+Shift+Tab and Ctrl+Page Up for the
//! previous.
//!
//! A step is a click on the neighbouring tab, taken through the same door
//! (`request_activation`): it is refused, with the same message, while the
//! active document has work or a dialog in flight (`guard::busy_refusal`),
//! and it commits an open retype box first.

use gtk::gio;
use gtk::prelude::*;

use super::list::TabId;
use super::{active_tab, request_activation};
use crate::app::shortcuts;
use crate::app::state::Viewer;

/// The tab `step` places along the strip from `active`, wrapping at either
/// end. `None` when there is no other tab to go to.
pub(super) fn neighbour(order: &[TabId], active: Option<TabId>, step: i32) -> Option<TabId> {
    if order.len() < 2 {
        return None;
    }
    let position = order.iter().position(|id| Some(*id) == active)?;
    let count = order.len() as i64;
    let next = (position as i64 + i64::from(step)).rem_euclid(count) as usize;
    (next != position).then(|| order[next])
}

/// Whether there is another tab to step to.
pub(crate) fn has_other_tab(viewer: &Viewer) -> bool {
    viewer.tabs.borrow().list.ids().len() > 1
}

/// Activates the tab `step` places from the active one. Nothing with fewer
/// than two tabs; a refusal is said like a refused click.
pub(crate) fn cycle(viewer: &Viewer, step: i32) {
    let active = active_tab(viewer);
    let target = neighbour(&viewer.tabs.borrow().list.ids(), active, step);
    if let Some(id) = target {
        request_activation(viewer, id);
    }
}

/// Installs `win.next-tab` and `win.previous-tab` and binds their keys.
pub(crate) fn connect_shortcuts(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    for (shortcut, step) in [(shortcuts::NEXT_TAB, 1), (shortcuts::PREVIOUS_TAB, -1)] {
        let action = gio::SimpleAction::new(shortcut.action, None);
        action.connect_activate({
            let viewer = viewer.clone();
            move |_, _| cycle(&viewer, step)
        });
        window.add_action(&action);
        shortcut.install(application);
    }
}

#[cfg(test)]
mod tests {
    use super::super::list::TabList;
    use super::*;

    fn three() -> Vec<TabId> {
        let mut list = TabList::default();
        (0..3).map(|_| list.add(None)).collect()
    }

    #[test]
    fn a_step_goes_to_the_neighbour_in_strip_order() {
        let tabs = three();

        assert_eq!(neighbour(&tabs, Some(tabs[0]), 1), Some(tabs[1]));
        assert_eq!(neighbour(&tabs, Some(tabs[1]), -1), Some(tabs[0]));
    }

    #[test]
    fn a_step_wraps_at_either_end_of_the_strip() {
        let tabs = three();

        assert_eq!(neighbour(&tabs, Some(tabs[2]), 1), Some(tabs[0]));
        assert_eq!(neighbour(&tabs, Some(tabs[0]), -1), Some(tabs[2]));
    }

    #[test]
    fn one_tab_or_none_has_nowhere_to_step() {
        let tabs = three();

        assert_eq!(neighbour(&tabs[..1], Some(tabs[0]), 1), None);
        assert_eq!(neighbour(&[], None, 1), None);
    }

    /// No active tab — between a park and the next install — is not a
    /// position to step from.
    #[test]
    fn with_no_active_tab_there_is_nothing_to_step_from() {
        let tabs = three();

        assert_eq!(neighbour(&tabs, None, 1), None);
    }
}
