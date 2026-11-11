//! Runtime facts about wasm32-unknown-unknown that decide how
//! core/pdf-render's actor (a dedicated OS thread + Condvar-blocking
//! callers) would have to change for the web.
#![cfg(target_arch = "wasm32")]

use std::sync::{Condvar, Mutex};
use std::time::Duration;
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
#[should_panic]
fn blocking_on_a_condvar_panics() {
    // JobHandle::wait() blocks the caller until the actor answers; on a
    // single-threaded wasm there is no one to answer, and std refuses.
    let pair = (Mutex::new(false), Condvar::new());
    let guard = pair.0.lock().unwrap();
    let _ = pair.1.wait_timeout(guard, Duration::from_millis(1)).unwrap();
}
