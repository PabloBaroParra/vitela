//! Start marks for latencies that end in a different callback than the one
//! they began in — an open that ends when its first page is painted, a zoom
//! step that ends when the sharp render lands.
//!
//! A mark is set under a key and taken exactly once, so "the first page
//! after X" needs no flag of its own: the first landing takes the mark and
//! every later one finds nothing.

use std::time::Instant;

/// Pending start marks, bounded so a mark nothing ever takes (an open the
/// user closed before it painted) cannot accumulate.
pub(super) struct Marks<K> {
    pending: Vec<(K, Instant)>,
    capacity: usize,
}

impl<K: PartialEq> Marks<K> {
    pub(super) const fn new(capacity: usize) -> Self {
        Self {
            pending: Vec::new(),
            capacity,
        }
    }

    /// Records `at` under `key`, replacing a mark already there. At
    /// capacity, the mark set longest ago is dropped to make room.
    pub(super) fn set(&mut self, key: K, at: Instant) {
        if let Some(slot) = self.pending.iter_mut().find(|(held, _)| *held == key) {
            slot.1 = at;
            return;
        }
        if self.pending.len() >= self.capacity && !self.pending.is_empty() {
            self.pending.remove(0);
        }
        self.pending.push((key, at));
    }

    /// Removes and returns the mark under `key`.
    pub(super) fn take(&mut self, key: &K) -> Option<Instant> {
        let index = self.pending.iter().position(|(held, _)| held == key)?;
        Some(self.pending.remove(index).1)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_mark_is_taken_once() {
        let at = Instant::now();
        let mut marks = Marks::new(4);
        marks.set("open", at);
        assert_eq!(marks.take(&"open"), Some(at));
        assert_eq!(marks.take(&"open"), None);
    }

    #[test]
    fn a_key_that_was_never_set_has_no_mark() {
        let mut marks: Marks<&str> = Marks::new(4);
        assert_eq!(marks.take(&"zoom"), None);
    }

    #[test]
    fn setting_a_key_again_replaces_its_mark() {
        let first = Instant::now();
        let second = first + Duration::from_millis(5);
        let mut marks = Marks::new(4);
        marks.set("zoom", first);
        marks.set("zoom", second);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks.take(&"zoom"), Some(second));
    }

    #[test]
    fn at_capacity_the_oldest_mark_makes_room() {
        let at = Instant::now();
        let mut marks = Marks::new(2);
        marks.set(1, at);
        marks.set(2, at);
        marks.set(3, at);
        assert_eq!(marks.len(), 2);
        assert_eq!(marks.take(&1), None);
        assert_eq!(marks.take(&2), Some(at));
        assert_eq!(marks.take(&3), Some(at));
    }
}
