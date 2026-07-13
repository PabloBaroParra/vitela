//! Unit-level tests for the offline-detection logic itself (T-005, TDD).
//!
//! These tests are deterministic and loopback-only — they do NOT depend on
//! the ambient network policy of whatever machine runs `cargo test` (dev
//! laptop or CI), so they always pass/fail the same way regardless of
//! whether outbound internet access happens to be available. They prove the
//! *detection logic* is correct; see `tests/real_world_egress.rs` for the
//! actual CI-only zero-network enforcement proof.

use std::net::TcpListener;
use std::time::Duration;

use zero_network_guard::assert_offline;

const SHORT_TIMEOUT: Duration = Duration::from_millis(300);

#[test]
fn detects_reachable_local_listener_as_a_leak() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    // Keep the listener alive for the duration of the connect attempt.
    let result = assert_offline("127.0.0.1", port, SHORT_TIMEOUT);

    assert!(
        result.is_err(),
        "a genuinely reachable endpoint must be reported as a network leak, not as offline"
    );
    drop(listener);
}

#[test]
fn detects_closed_local_port_as_unreachable() {
    // Bind then immediately drop: the OS will refuse connections to this
    // port until it's reused, which is a deterministic "nothing is
    // listening" signal independent of any real network policy.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let result = assert_offline("127.0.0.1", port, SHORT_TIMEOUT);

    assert!(
        result.is_ok(),
        "a closed port must be confirmed as unreachable (no leak), got: {result:?}"
    );
}
