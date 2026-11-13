//! The commands run from the palette this session (B27 decision 8).
//!
//! Kept in memory only, for as long as the window lives: the list is
//! deliberately not saved to disk in v1. Pure — no GTK — so the ordering is
//! tested on its own.

/// Entry ids, most recently run first, each at most once.
#[derive(Debug, Default)]
pub(crate) struct Recent(Vec<&'static str>);

impl Recent {
    /// Records that `id` just ran: it moves to the front.
    pub(crate) fn record(&mut self, id: &'static str) {
        self.0.retain(|recent| *recent != id);
        self.0.insert(0, id);
    }

    /// `items` with the recently run ones first, most recent first; the
    /// rest keep the order they came in (the catalog's, for an empty query).
    pub(crate) fn order<T>(&self, mut items: Vec<T>, id: impl Fn(&T) -> &'static str) -> Vec<T> {
        // A stable sort: everything not recent shares one key and so keeps
        // its relative order.
        items.sort_by_key(|item| {
            let id = id(item);
            self.0
                .iter()
                .position(|recent| *recent == id)
                .unwrap_or(usize::MAX)
        });
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: [&str; 5] = ["a", "b", "c", "d", "e"];

    fn ordered(recent: &Recent) -> Vec<&'static str> {
        recent.order(CATALOG.to_vec(), |id| id)
    }

    #[test]
    fn nothing_recent_keeps_the_given_order() {
        assert_eq!(ordered(&Recent::default()), CATALOG);
    }

    #[test]
    fn the_most_recent_comes_first_then_the_rest_in_order() {
        let mut recent = Recent::default();
        recent.record("d");
        recent.record("b");
        assert_eq!(ordered(&recent), ["b", "d", "a", "c", "e"]);
    }

    #[test]
    fn running_a_command_again_moves_it_to_the_front_once() {
        let mut recent = Recent::default();
        recent.record("d");
        recent.record("b");
        recent.record("d");
        assert_eq!(ordered(&recent), ["d", "b", "a", "c", "e"]);
    }

    #[test]
    fn an_id_no_longer_in_the_list_is_ignored() {
        let mut recent = Recent::default();
        recent.record("gone");
        recent.record("c");
        assert_eq!(ordered(&recent), ["c", "a", "b", "d", "e"]);
    }
}
