//! Runtime facts about wasm32-unknown-unknown that decide how
//! core/pdf-render's actor (a dedicated OS thread, a worker loop parked on a
//! Condvar, and callers blocked on an mpsc receiver) would have to change for
//! the web.
//!
//! Not pinned here because a test cannot observe it without hanging the
//! runner: `mpsc::Receiver::recv()` on an empty channel never returns on this
//! target (a manual probe was killed by `timeout` after 90 s). That is what
//! `JobHandle::wait()` (core/pdf-render/src/actor.rs:286) does, so a caller
//! waiting on a job would freeze the worker instead of panicking.
#![cfg(target_arch = "wasm32")]

use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Condvar, Mutex};
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn spawning_a_thread_fails() {
    // pdf-render's Actor::spawn does `.spawn(..).expect(..)`: it would panic.
    assert!(std::thread::Builder::new().spawn(|| {}).is_err());
}

#[wasm_bindgen_test]
fn uncontended_mutex_works() {
    // pdfium-render's `thread_safe` feature only wraps calls in a Mutex;
    // single-threaded, it is always uncontended, so it is harmless.
    let m = Mutex::new(1);
    *m.lock().unwrap() += 1;
    assert_eq!(*m.lock().unwrap(), 2);
}

#[wasm_bindgen_test]
#[should_panic(expected = "condvar wait not supported")]
fn condvar_wait_panics() {
    // Actor::worker_loop parks on `condvar.wait(queue)` when the queue is
    // empty; std's no-threads Condvar refuses that outright.
    let pair = (Mutex::new(false), Condvar::new());
    let guard = pair.0.lock().unwrap();
    drop(pair.1.wait(guard));
}

#[wasm_bindgen_test]
fn non_blocking_channel_receive_works() {
    // The usable alternative to JobHandle::wait(): poll without blocking.
    let (tx, rx) = mpsc::channel::<u8>();
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    tx.send(7).unwrap();
    assert_eq!(rx.try_recv(), Ok(7));
}
