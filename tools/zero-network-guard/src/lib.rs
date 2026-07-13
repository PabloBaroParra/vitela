//! Zero-network CI enforcement (T-005, [Offline]).
//!
//! Spec.md's "Offline-First, No Telemetry" NFR requires that MVP operations
//! make zero outbound network calls. This crate provides:
//! - [`assert_offline`]: deterministic detection logic — reports whether a
//!   given `host:port` is reachable ("a network leak") or not, given a
//!   timeout. Testable entirely against loopback, independent of the actual
//!   CI network policy (see `tests/offline_detection.rs`).
//! - An `#[ignore]`d integration test (`tests/real_world_egress.rs`) that
//!   asserts well-known public endpoints are unreachable; this is the actual
//!   proof of zero-network enforcement, meant to run ONLY inside the
//!   network-isolated step of `core.yml` (see that workflow for how the
//!   isolation itself — `unshare --net` on the Linux CI runner — is applied;
//!   this crate cannot create that isolation, only verify it holds).
//!
//! This is deliberately a workspace-internal CI tool, not part of the
//! production `pdf-editor` shells or core crates.

use std::fmt;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Reported when a target expected to be unreachable (in a zero-network
/// environment) actually accepted a connection — i.e. network egress leaked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkLeak {
    pub host: String,
    pub port: u16,
}

impl fmt::Display for NetworkLeak {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "network leak detected: {}:{} was reachable, but zero-network was expected",
            self.host, self.port
        )
    }
}

impl std::error::Error for NetworkLeak {}

/// Attempts to connect to `host:port` within `timeout`.
///
/// Returns `Ok(())` if the connection attempt fails (timed out, refused, or
/// could not resolve) — i.e. the target is confirmed unreachable, which is
/// the desired state in a zero-network test environment. Returns
/// `Err(NetworkLeak)` if a connection is actually established, proving
/// outbound network egress is possible (a violation of the Offline-First
/// NFR in a context where it must not be).
pub fn assert_offline(host: &str, port: u16, timeout: Duration) -> Result<(), NetworkLeak> {
    // Resolution itself requires network/DNS in the general case; treat any
    // resolution failure the same as "unreachable" (still a pass for our
    // purposes — either way, no outbound connection was established).
    let mut addrs = match (host, port).to_socket_addrs() {
        Ok(addrs) => addrs,
        Err(_) => return Ok(()),
    };

    let leak = NetworkLeak {
        host: host.to_string(),
        port,
    };

    for addr in addrs.by_ref() {
        if TcpStream::connect_timeout(&addr, timeout).is_ok() {
            return Err(leak);
        }
    }

    Ok(())
}

/// Convenience wrapper: `assert_offline` against several well-known,
/// normally-always-reachable public endpoints. Used by the CI-only
/// enforcement test — succeeding here (all targets unreachable) is strong
/// evidence the test process has no outbound network path at all, since
/// these hosts are reachable from virtually any unrestricted network.
pub fn assert_offline_against_public_internet(timeout: Duration) -> Vec<NetworkLeak> {
    const PUBLIC_TARGETS: &[(&str, u16)] = &[
        ("1.1.1.1", 443), // Cloudflare DNS-over-HTTPS / well-known always-up host
        ("8.8.8.8", 53),  // Google public DNS
        ("9.9.9.9", 53),  // Quad9 public DNS
    ];

    PUBLIC_TARGETS
        .iter()
        .filter_map(|&(host, port)| assert_offline(host, port, timeout).err())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_leak_display_is_informative() {
        let leak = NetworkLeak {
            host: "example.test".to_string(),
            port: 443,
        };
        let msg = leak.to_string();
        assert!(msg.contains("example.test"));
        assert!(msg.contains("443"));
    }

    #[test]
    fn unresolvable_host_counts_as_offline() {
        // A syntactically invalid/unresolvable host name should never
        // report a "leak" — there's nothing to leak to.
        let result = assert_offline(
            "this-host-does-not-resolve.invalid",
            1,
            Duration::from_millis(200),
        );
        assert!(result.is_ok());
    }
}
