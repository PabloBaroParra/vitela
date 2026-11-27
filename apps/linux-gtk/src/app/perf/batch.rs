//! Timing a batch of independent jobs to its last landing — the Organize
//! screen's thumbnails, each rendered by a future of its own.

use std::time::{Duration, Instant};

/// One batch at a time: started, joined by each job as it is dispatched,
/// and finished by whichever job leaves last.
pub(super) struct Batch {
    epoch: u64,
    open: Option<Open>,
}

struct Open {
    started: Instant,
    joined: usize,
    outstanding: usize,
}

/// A job's membership of the batch it joined. A ticket from a batch that
/// has since been restarted is ignored when it leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket(u64);

impl Batch {
    pub(super) const fn new() -> Self {
        Self {
            epoch: 0,
            open: None,
        }
    }

    /// Opens a new batch timed from `started`, abandoning one still open.
    pub(super) fn start(&mut self, started: Instant) {
        self.epoch += 1;
        self.open = Some(Open {
            started,
            joined: 0,
            outstanding: 0,
        });
    }

    /// Adds a job to the open batch; `None` when no batch is open.
    pub(super) fn join(&mut self) -> Option<Ticket> {
        let open = self.open.as_mut()?;
        open.joined += 1;
        open.outstanding += 1;
        Some(Ticket(self.epoch))
    }

    /// A job is done. `Some` with the batch's duration and size when it was
    /// the last one out, which also closes the batch.
    pub(super) fn leave(&mut self, ticket: Ticket, now: Instant) -> Option<(Duration, usize)> {
        if ticket.0 != self.epoch {
            return None;
        }
        let open = self.open.as_mut()?;
        open.outstanding = open.outstanding.saturating_sub(1);
        if open.outstanding > 0 {
            return None;
        }
        let open = self.open.take()?;
        Some((now.saturating_duration_since(open.started), open.joined))
    }

    /// Closes a batch nothing is outstanding in — the dispatching pass is
    /// over and every job was answered without one (all cached). A batch
    /// with jobs still out stays open for them.
    pub(super) fn seal(&mut self) {
        if self.open.as_ref().is_some_and(|open| open.outstanding == 0) {
            self.open = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn later(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn the_last_job_out_reports_the_whole_batch() {
        let start = Instant::now();
        let mut batch = Batch::new();
        batch.start(start);
        let first = batch.join().expect("the batch is open");
        let second = batch.join().expect("the batch is open");
        assert_eq!(batch.leave(first, later(start, 10)), None);
        assert_eq!(
            batch.leave(second, later(start, 30)),
            Some((Duration::from_millis(30), 2))
        );
    }

    #[test]
    fn nothing_joins_when_no_batch_is_open() {
        let mut batch = Batch::new();
        assert_eq!(batch.join(), None);
    }

    #[test]
    fn a_finished_batch_takes_no_more_jobs() {
        let start = Instant::now();
        let mut batch = Batch::new();
        batch.start(start);
        let only = batch.join().expect("the batch is open");
        assert!(batch.leave(only, later(start, 5)).is_some());
        assert_eq!(batch.join(), None);
    }

    #[test]
    fn a_job_from_a_restarted_batch_does_not_finish_the_new_one() {
        let start = Instant::now();
        let mut batch = Batch::new();
        batch.start(start);
        let stale = batch.join().expect("the batch is open");
        batch.start(later(start, 100));
        let current = batch.join().expect("the batch is open");
        assert_eq!(batch.leave(stale, later(start, 110)), None);
        assert_eq!(
            batch.leave(current, later(start, 150)),
            Some((Duration::from_millis(50), 1))
        );
    }

    #[test]
    fn sealing_closes_a_batch_nothing_joined() {
        let mut batch = Batch::new();
        batch.start(Instant::now());
        batch.seal();
        assert_eq!(batch.join(), None);
    }

    #[test]
    fn sealing_leaves_a_batch_with_jobs_out_open() {
        let start = Instant::now();
        let mut batch = Batch::new();
        batch.start(start);
        let only = batch.join().expect("the batch is open");
        batch.seal();
        assert_eq!(
            batch.leave(only, later(start, 8)),
            Some((Duration::from_millis(8), 1))
        );
    }
}
