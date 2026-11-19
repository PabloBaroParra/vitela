//! The window's tab order and which tab is active — pure bookkeeping, no GTK
//! and no documents.
//!
//! Every rule about *which* tab a gesture lands on lives here, so it is
//! tested without a window: a new tab goes last, closing the active tab hands
//! the focus to its right-hand neighbour (or its left one when it was last),
//! and a path that is already open names the tab that holds it.

use std::path::{Path, PathBuf};

/// One tab's identity for the lifetime of the window. Never reused, so a
/// stale id (a click on a tab that has since closed) names nothing rather
/// than some newer tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TabId(u64);

#[derive(Debug)]
struct Entry {
    id: TabId,
    /// The file the tab's document was opened from or last saved to, already
    /// normalised by the caller; `None` for the sample, a blank document or
    /// a restored copy.
    path: Option<PathBuf>,
}

/// What closing a tab leaves active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AfterClose {
    /// The closed tab was not the active one: the active tab stays.
    Unchanged,
    /// The closed tab was active; this one should be activated next.
    Activate(TabId),
    /// The closed tab was the last one.
    Empty,
}

#[derive(Debug, Default)]
pub(crate) struct TabList {
    entries: Vec<Entry>,
    active: Option<TabId>,
    next_id: u64,
}

impl TabList {
    /// Appends a tab at the end of the strip. It is not activated: the
    /// caller decides when the document behind it is on screen.
    pub(crate) fn add(&mut self, path: Option<PathBuf>) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        self.entries.push(Entry { id, path });
        id
    }

    /// Makes `id` the active tab. `false`, and no change, for an id that is
    /// not in the list.
    pub(crate) fn activate(&mut self, id: TabId) -> bool {
        if !self.contains(id) {
            return false;
        }
        self.active = Some(id);
        true
    }

    /// No tab is active: the one that was has been parked, and the next one
    /// is not installed yet.
    pub(crate) fn deactivate(&mut self) {
        self.active = None;
    }

    pub(crate) fn active(&self) -> Option<TabId> {
        self.active
    }

    /// Removes `id`, answering what should become active. `None` when `id`
    /// is not in the list.
    pub(crate) fn close(&mut self, id: TabId) -> Option<AfterClose> {
        let order = self.ids();
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        self.entries.remove(index);
        let after = next_active_after_close(&order, id, self.active);
        match after {
            AfterClose::Unchanged => {}
            AfterClose::Activate(_) | AfterClose::Empty => self.active = None,
        }
        Some(after)
    }

    /// The tab holding `path`, compared exactly — the caller normalises both
    /// sides the same way. When several tabs hold it, the active one wins,
    /// then the first in strip order.
    pub(crate) fn find_by_path(&self, path: &Path) -> Option<TabId> {
        let holders = self.holders(path);
        holders
            .iter()
            .copied()
            .find(|id| Some(*id) == self.active)
            .or_else(|| holders.first().copied())
    }

    /// Every tab holding `path`, in strip order.
    pub(crate) fn holders(&self, path: &Path) -> Vec<TabId> {
        self.entries
            .iter()
            .filter(|entry| entry.path.as_deref() == Some(path))
            .map(|entry| entry.id)
            .collect()
    }

    /// Records where `id`'s document now lives — a Save As moves it.
    pub(crate) fn set_path(&mut self, id: TabId, path: Option<PathBuf>) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.path = path;
        }
    }

    pub(crate) fn contains(&self, id: TabId) -> bool {
        self.entries.iter().any(|entry| entry.id == id)
    }

    /// Every tab, in strip order.
    pub(crate) fn ids(&self) -> Vec<TabId> {
        self.entries.iter().map(|entry| entry.id).collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Which tab takes over when `closing` is closed out of `order` while
/// `active` is the active one.
///
/// The right-hand neighbour first, as browsers and editors do: the strip does
/// not shift under the pointer, so the tab that slides into the closed one's
/// place is the one that becomes active. The left-hand one only when the
/// closed tab was last.
pub(crate) fn next_active_after_close(
    order: &[TabId],
    closing: TabId,
    active: Option<TabId>,
) -> AfterClose {
    if active != Some(closing) {
        return AfterClose::Unchanged;
    }
    let Some(index) = order.iter().position(|id| *id == closing) else {
        return AfterClose::Unchanged;
    };
    order
        .get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|left| order.get(left)))
        .map_or(AfterClose::Empty, |next| AfterClose::Activate(*next))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> (TabList, TabId, TabId, TabId) {
        let mut list = TabList::default();
        let a = list.add(Some(PathBuf::from("/docs/a.pdf")));
        let b = list.add(Some(PathBuf::from("/docs/b.pdf")));
        let c = list.add(None);
        (list, a, b, c)
    }

    #[test]
    fn a_new_tab_goes_last_and_is_not_activated() {
        let (list, a, b, c) = three();

        assert_eq!(list.ids(), [a, b, c]);
        assert_eq!(list.active(), None);
    }

    #[test]
    fn ids_are_never_reused() {
        let (mut list, _, _, c) = three();
        list.close(c);

        let d = list.add(None);

        assert_ne!(d, c);
    }

    #[test]
    fn activating_an_unknown_tab_changes_nothing() {
        let (mut list, a, _, c) = three();
        list.activate(a);
        list.close(c);

        assert!(!list.activate(c));
        assert_eq!(list.active(), Some(a));
    }

    #[test]
    fn closing_an_inactive_tab_keeps_the_active_one() {
        let (mut list, a, b, _) = three();
        list.activate(b);

        assert_eq!(list.close(a), Some(AfterClose::Unchanged));
        assert_eq!(list.active(), Some(b));
    }

    #[test]
    fn closing_the_active_tab_hands_over_to_its_right_neighbour() {
        let (mut list, a, b, _) = three();
        list.activate(a);

        assert_eq!(list.close(a), Some(AfterClose::Activate(b)));
        assert_eq!(
            list.active(),
            None,
            "the caller activates it once installed"
        );
    }

    #[test]
    fn closing_the_last_active_tab_hands_over_to_its_left_neighbour() {
        let (mut list, _, b, c) = three();
        list.activate(c);

        assert_eq!(list.close(c), Some(AfterClose::Activate(b)));
    }

    #[test]
    fn closing_the_only_tab_leaves_the_strip_empty() {
        let mut list = TabList::default();
        let only = list.add(None);
        list.activate(only);

        assert_eq!(list.close(only), Some(AfterClose::Empty));
        assert!(list.is_empty());
    }

    #[test]
    fn closing_a_tab_that_is_gone_answers_none() {
        let (mut list, a, _, _) = three();
        list.close(a);

        assert_eq!(list.close(a), None);
    }

    #[test]
    fn a_path_finds_the_tab_that_holds_it() {
        let (list, _, b, _) = three();

        assert_eq!(list.find_by_path(Path::new("/docs/b.pdf")), Some(b));
        assert_eq!(list.find_by_path(Path::new("/docs/z.pdf")), None);
    }

    #[test]
    fn a_tab_without_a_path_is_never_found_by_one() {
        let mut list = TabList::default();
        list.add(None);

        assert_eq!(list.find_by_path(Path::new("")), None);
    }

    #[test]
    fn a_moved_document_is_found_at_its_new_path() {
        let (mut list, a, _, _) = three();
        list.set_path(a, Some(PathBuf::from("/docs/renamed.pdf")));

        assert_eq!(list.find_by_path(Path::new("/docs/a.pdf")), None);
        assert_eq!(list.find_by_path(Path::new("/docs/renamed.pdf")), Some(a));
    }

    #[test]
    fn of_two_tabs_holding_one_path_the_active_one_is_found() {
        let mut list = TabList::default();
        let first = list.add(Some(PathBuf::from("/docs/same.pdf")));
        let second = list.add(Some(PathBuf::from("/docs/same.pdf")));

        assert_eq!(list.find_by_path(Path::new("/docs/same.pdf")), Some(first));
        list.activate(second);
        assert_eq!(list.find_by_path(Path::new("/docs/same.pdf")), Some(second));
        assert_eq!(list.holders(Path::new("/docs/same.pdf")), [first, second]);
    }

    #[test]
    fn deactivating_parks_the_active_tab_without_closing_it() {
        let (mut list, a, _, _) = three();
        list.activate(a);

        list.deactivate();

        assert_eq!(list.active(), None);
        assert!(list.contains(a));
    }
}
