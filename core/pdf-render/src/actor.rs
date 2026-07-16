//! Generic single-thread priority-queue actor (T-015, T-020).
//!
//! `Actor<S>` is the reusable scheduling engine: it owns exactly one OS
//! thread and a shared mutable state value `S`, and executes jobs submitted
//! from any number of caller threads strictly one at a time, in priority
//! order. `pdfium`'s process-global, library-wide thread-unsafety (see
//! `design.md` "Threading — pdfium single-actor model") is the reason this
//! exists — `PdfiumActor` (in `renderer.rs`) is `Actor<PdfiumState>`.
//!
//! Kept generic over `S` deliberately: scheduling behavior (priority
//! reordering, cancel-at-dequeue) is pure queueing logic that does not need a
//! real pdfium binding to test, which is why T-020's scheduling tests run
//! fast and without the pdfium dynamic library present (see
//! `tests/actor_scheduling.rs`), while a smaller set of tests exercises the
//! real pdfium-backed executor end to end.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::error::RenderError;
use crate::options::Priority;

/// Type-erased unit of work: given `&mut S` and whether this job was
/// cancelled before being dequeued, does whatever it needs to do — including
/// delivering its own result to the caller's [`JobHandle`]. Cancellation
/// handling lives inside the closure built by [`Actor::submit`] so the queue
/// itself stays generic over the result type of each job.
type BoxedJob<S> = Box<dyn FnOnce(&mut S, bool) + Send>;

struct QueuedJob<S> {
    priority: Priority,
    sequence: u64,
    run: BoxedJob<S>,
}

impl<S> PartialEq for QueuedJob<S> {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl<S> Eq for QueuedJob<S> {}

impl<S> PartialOrd for QueuedJob<S> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<S> Ord for QueuedJob<S> {
    /// `BinaryHeap::pop()` returns the *greatest* element, so "greater" here
    /// means "more urgent, should run next":
    ///
    /// - Lower [`Priority`] value (e.g. `Visible`) always beats higher.
    /// - Within `Priority::Visible`, **newer submissions win** (LIFO): this
    ///   is what makes "user scrolls to a new page" jump the new page's
    ///   render ahead of a still-queued, now-stale `Visible` request for a
    ///   page that's no longer on screen (`spec.md` "Priority reordering on
    ///   new scroll target"). A plain `BinaryHeap` has no in-place
    ///   decrease-key/requeue operation, so rather than mutating already
    ///   queued entries, newer same-tier submissions are simply favored —
    ///   this satisfies the scenario without needing a requeue mechanism.
    /// - Within any other tier (`Prefetch`, `Thumbnail`), **older
    ///   submissions win** (FIFO) — fairness among non-urgent background
    ///   work, since there's no "staleness" concern for prefetch/thumbnail
    ///   jobs the way there is for the visible page.
    fn cmp(&self, other: &Self) -> Ordering {
        match other.priority.cmp(&self.priority) {
            Ordering::Equal => {
                if self.priority == Priority::Visible {
                    self.sequence.cmp(&other.sequence) // newer (larger) wins
                } else {
                    other.sequence.cmp(&self.sequence) // older (smaller) wins
                }
            }
            ord => ord,
        }
    }
}

struct Shared<S> {
    heap: Mutex<BinaryHeap<QueuedJob<S>>>,
    condvar: Condvar,
    shutdown: AtomicBool,
    next_sequence: std::sync::atomic::AtomicU64,
}

/// A single dedicated-OS-thread priority-queue actor over shared state `S`.
pub struct Actor<S> {
    shared: Arc<Shared<S>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl<S: Send + 'static> Actor<S> {
    /// Spawns the actor's worker thread, which owns `initial_state` for the
    /// lifetime of the actor.
    pub fn spawn(initial_state: S) -> Self {
        let shared: Arc<Shared<S>> = Arc::new(Shared {
            heap: Mutex::new(BinaryHeap::new()),
            condvar: Condvar::new(),
            shutdown: AtomicBool::new(false),
            next_sequence: std::sync::atomic::AtomicU64::new(0),
        });

        let worker_shared = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("pdfium-actor".to_string())
            .spawn(move || Self::worker_loop(worker_shared, initial_state))
            .expect("failed to spawn pdfium actor thread");

        Actor {
            shared,
            worker: Mutex::new(Some(worker)),
        }
    }

    fn worker_loop(shared: Arc<Shared<S>>, mut state: S) {
        loop {
            let job = {
                let mut heap = shared
                    .heap
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                loop {
                    if let Some(job) = heap.pop() {
                        break Some(job);
                    }
                    if shared.shutdown.load(AtomicOrdering::SeqCst) {
                        break None;
                    }
                    heap = shared
                        .condvar
                        .wait(heap)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
            };

            match job {
                Some(job) => (job.run)(&mut state, false),
                None => return,
            }
        }
    }

    /// Submits a job at the given priority. `job` receives `&mut S` and must
    /// return the job's result; cancellation (via the returned
    /// [`JobHandle::cancel`]) is checked at dequeue time, before `job` is
    /// ever invoked — matching `spec.md`'s "Cancel scrolled-past page at
    /// dequeue" scenario.
    pub fn submit<T, F>(&self, priority: Priority, job: F) -> JobHandle<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut S) -> Result<T, RenderError> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let job_cancel = Arc::clone(&cancel);

        let run: BoxedJob<S> = Box::new(move |state: &mut S, dequeue_cancelled: bool| {
            let result = if dequeue_cancelled || job_cancel.load(AtomicOrdering::SeqCst) {
                Err(RenderError::Cancelled)
            } else {
                job(state)
            };
            let _ = tx.send(result);
        });

        let sequence = self
            .shared
            .next_sequence
            .fetch_add(1, AtomicOrdering::SeqCst);

        {
            let mut heap = self
                .shared
                .heap
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            heap.push(QueuedJob {
                priority,
                sequence,
                run,
            });
        }
        self.shared.condvar.notify_one();

        JobHandle {
            receiver: rx,
            cancel,
        }
    }

    /// Number of jobs currently queued (not yet dequeued). Test/diagnostic
    /// helper for scheduling assertions.
    pub fn queue_len(&self) -> usize {
        self.shared
            .heap
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

impl<S> Drop for Actor<S> {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, AtomicOrdering::SeqCst);
        self.shared.condvar.notify_all();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}

/// A cancellable handle to a single submitted job's eventual result.
pub struct JobHandle<T> {
    receiver: Receiver<Result<T, RenderError>>,
    cancel: Arc<AtomicBool>,
}

impl<T> JobHandle<T> {
    /// A handle that was never queued and already holds `error` — lets
    /// fire-and-forget APIs (`render_page`, `text_runs`) report an actor
    /// bring-up failure through the same channel as any job result instead
    /// of panicking.
    pub(crate) fn failed(error: RenderError) -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(Err(error));
        JobHandle {
            receiver: rx,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Requests cancellation. Has no effect if the job has already been
    /// dequeued and started running (mid-raster abort is a documented future
    /// optimization, not MVP behavior — see `spec.md`).
    pub fn cancel(&self) {
        self.cancel.store(true, AtomicOrdering::SeqCst);
    }

    /// Blocks until the job completes (or was cancelled), returning its
    /// result.
    pub fn wait(self) -> Result<T, RenderError> {
        self.receiver
            .recv()
            .unwrap_or(Err(RenderError::ActorShutDown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    fn spawn_recording_actor() -> (Actor<()>, Arc<Mutex<Vec<u32>>>) {
        let actor = Actor::spawn(());
        let order = Arc::new(Mutex::new(Vec::new()));
        (actor, order)
    }

    #[test]
    fn failed_handle_delivers_its_error_without_an_actor() {
        let handle: JobHandle<u32> = JobHandle::failed(RenderError::ActorShutDown);
        assert!(matches!(handle.wait(), Err(RenderError::ActorShutDown)));
    }

    #[test]
    fn jobs_run_and_return_results() {
        let (actor, _order) = spawn_recording_actor();
        let handle = actor.submit(Priority::Visible, |_state: &mut ()| {
            Ok::<_, RenderError>(42)
        });
        assert_eq!(handle.wait().unwrap(), 42);
    }

    #[test]
    fn visible_priority_jumps_ahead_of_queued_thumbnails() {
        let actor = Actor::spawn(());
        let order = Arc::new(Mutex::new(Vec::new()));

        // Hold the actor busy on a first job so subsequent submissions
        // accumulate in the queue before any of them are dequeued.
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let gate_clone = Arc::clone(&gate);
        let blocker = actor.submit(Priority::Visible, move |_state: &mut ()| {
            let (lock, cvar) = &*gate_clone;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = cvar.wait(released).unwrap();
            }
            Ok::<_, RenderError>(())
        });

        let record = |order: Arc<Mutex<Vec<u32>>>, tag: u32| {
            move |_state: &mut ()| {
                order.lock().unwrap().push(tag);
                Ok::<_, RenderError>(())
            }
        };

        let thumb_1 = actor.submit(Priority::Thumbnail, record(Arc::clone(&order), 1));
        let thumb_2 = actor.submit(Priority::Thumbnail, record(Arc::clone(&order), 2));
        // Wait until both thumbnail jobs are actually queued (not just
        // submitted) before submitting the "scroll to a new page" job, so
        // this assertion is about queue ordering, not submission-thread
        // scheduling races.
        while actor.queue_len() < 2 {
            std::thread::yield_now();
        }
        let visible_new_page = actor.submit(Priority::Visible, record(Arc::clone(&order), 99));

        // Release the blocking first job; the actor now drains the queue.
        {
            let (lock, cvar) = &*gate;
            *lock.lock().unwrap() = true;
            cvar.notify_one();
        }
        blocker.wait().unwrap();
        visible_new_page.wait().unwrap();
        thumb_1.wait().unwrap();
        thumb_2.wait().unwrap();

        // The new scroll target (99, Visible) must have been served before
        // both stale, lower-priority thumbnail jobs.
        assert_eq!(*order.lock().unwrap(), vec![99, 1, 2]);
    }

    #[test]
    fn cancelled_job_never_executes_and_reports_cancelled() {
        let actor = Actor::spawn(());
        let executed = Arc::new(AtomicU32::new(0));

        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let gate_clone = Arc::clone(&gate);
        let blocker = actor.submit(Priority::Visible, move |_state: &mut ()| {
            let (lock, cvar) = &*gate_clone;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = cvar.wait(released).unwrap();
            }
            Ok::<_, RenderError>(())
        });

        let executed_clone = Arc::clone(&executed);
        let scrolled_past = actor.submit(Priority::Thumbnail, move |_state: &mut ()| {
            executed_clone.fetch_add(1, AtomicOrdering::SeqCst);
            Ok::<_, RenderError>(())
        });
        scrolled_past.cancel();

        {
            let (lock, cvar) = &*gate;
            *lock.lock().unwrap() = true;
            cvar.notify_one();
        }
        blocker.wait().unwrap();

        let result = scrolled_past.wait();
        assert!(matches!(result, Err(RenderError::Cancelled)));
        assert_eq!(executed.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn concurrent_submissions_all_complete_without_crash() {
        let actor = Arc::new(Actor::spawn(()));
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let actor = Arc::clone(&actor);
                std::thread::spawn(move || {
                    actor
                        .submit(Priority::Visible, move |_state: &mut ()| {
                            Ok::<_, RenderError>(i)
                        })
                        .wait()
                })
            })
            .collect();

        let mut results: Vec<i32> = handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect();
        results.sort();
        assert_eq!(results, vec![0, 1, 2, 3, 4]);
    }
}
