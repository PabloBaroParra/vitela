//! Lets a test stop the autosave worker between its last pre-write check
//! and the write itself, and see when it has finished.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(20);

/// The worker's half: travels with the job.
pub(crate) struct PauseHook {
    held: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    finished: Arc<AtomicBool>,
}

impl PauseHook {
    pub(super) fn hold(&self) {
        let _ = self.held.send(());
        let _ = self.release.recv_timeout(PATIENCE);
    }
}

/// Dropped with the job — after the write, its cleanup, or a failure.
impl Drop for PauseHook {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::SeqCst);
    }
}

/// The test's half.
pub(crate) struct Pause {
    held: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
    finished: Arc<AtomicBool>,
}

impl Pause {
    /// Blocks until the worker holds: its checks passed, nothing written
    /// yet.
    pub(crate) fn wait_until_held(&self) {
        self.held
            .recv_timeout(PATIENCE)
            .expect("the worker reaches the write");
    }

    /// What lets the worker go on — callable from another thread.
    pub(crate) fn releaser(&self) -> impl FnOnce() + Send + 'static {
        let release = self.release.clone();
        move || {
            let _ = release.send(());
        }
    }

    /// Whether the worker is done with the job.
    pub(crate) fn worker_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

pub(super) fn pair() -> (Pause, PauseHook) {
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let finished = Arc::new(AtomicBool::new(false));
    (
        Pause {
            held: held_rx,
            release: release_tx,
            finished: Arc::clone(&finished),
        },
        PauseHook {
            held: held_tx,
            release: release_rx,
            finished,
        },
    )
}
