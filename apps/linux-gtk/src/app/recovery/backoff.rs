//! When autosave tries again after a write failed.
//!
//! A full disk or a read-only state directory fails every snapshot the same
//! way, and each attempt costs a full serialization of the document first.
//! So a failure pushes the next attempt out — one interval, then twice that,
//! doubling up to [`MAX_DELAY`] — and the first success resets the schedule.
//! Pure: the caller supplies the clock, so the rule is tested without one.

use std::time::{Duration, Instant};

/// The wait after the first failure: one ordinary autosave interval.
const FIRST_DELAY: Duration = Duration::from_secs(super::autosave::INTERVAL_SECONDS as u64);

/// The longest a failing autosave waits between attempts.
const MAX_DELAY: Duration = Duration::from_secs(15 * 60);

/// How long to wait after the `failures`-th failure in a row.
fn delay(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(16);
    (FIRST_DELAY * 2u32.pow(doublings)).min(MAX_DELAY)
}

/// The failure streak autosave is backing off from, if any.
#[derive(Debug, Default)]
pub(crate) struct Backoff {
    failures: u32,
    retry_at: Option<Instant>,
}

impl Backoff {
    /// Whether an attempt may be made at `now`.
    pub(crate) fn ready(&self, now: Instant) -> bool {
        self.retry_at.is_none_or(|retry_at| now >= retry_at)
    }

    /// Records a failed attempt at `now`. True for the first failure of a
    /// streak — the one the user is told about.
    pub(crate) fn failed(&mut self, now: Instant) -> bool {
        self.failures = self.failures.saturating_add(1);
        self.retry_at = Some(now + delay(self.failures));
        self.failures == 1
    }

    /// A write landed: the next failure starts a new streak.
    pub(crate) fn succeeded(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_delay_doubles_from_one_interval() {
        assert_eq!(delay(1), FIRST_DELAY);
        assert_eq!(delay(2), FIRST_DELAY * 2);
        assert_eq!(delay(3), FIRST_DELAY * 4);
    }

    #[test]
    fn the_delay_stops_growing_at_the_cap() {
        assert_eq!(delay(5), MAX_DELAY, "16 intervals is past the cap");
        assert_eq!(delay(64), MAX_DELAY, "no overflow however long it fails");
        assert_eq!(delay(u32::MAX), MAX_DELAY);
    }

    #[test]
    fn nothing_failed_means_always_ready() {
        assert!(Backoff::default().ready(Instant::now()));
    }

    #[test]
    fn a_failure_holds_attempts_back_until_its_delay_has_passed() {
        let start = Instant::now();
        let mut backoff = Backoff::default();

        backoff.failed(start);

        assert!(!backoff.ready(start));
        assert!(!backoff.ready(start + FIRST_DELAY - Duration::from_secs(1)));
        assert!(backoff.ready(start + FIRST_DELAY));
    }

    #[test]
    fn each_further_failure_waits_longer() {
        let start = Instant::now();
        let mut backoff = Backoff::default();
        backoff.failed(start);

        let second = start + FIRST_DELAY;
        backoff.failed(second);

        assert!(!backoff.ready(second + FIRST_DELAY));
        assert!(backoff.ready(second + FIRST_DELAY * 2));
    }

    #[test]
    fn only_the_first_failure_of_a_streak_is_reported() {
        let now = Instant::now();
        let mut backoff = Backoff::default();

        assert!(backoff.failed(now));
        assert!(!backoff.failed(now));
        assert!(!backoff.failed(now));
    }

    #[test]
    fn a_success_ends_the_streak() {
        let now = Instant::now();
        let mut backoff = Backoff::default();
        backoff.failed(now);
        backoff.failed(now);

        backoff.succeeded();

        assert!(backoff.ready(now));
        assert!(backoff.failed(now), "a new streak is reported again");
        assert!(
            !backoff.ready(now + FIRST_DELAY - Duration::from_secs(1)),
            "and starts from the shortest delay"
        );
        assert!(backoff.ready(now + FIRST_DELAY));
    }
}
