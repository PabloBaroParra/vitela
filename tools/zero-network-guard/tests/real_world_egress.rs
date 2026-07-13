//! CI-only zero-network enforcement proof (T-005, [Offline]).
//!
//! This test is `#[ignore]`d by default: on a normal developer machine (or
//! any environment with real internet access) these public endpoints WILL be
//! reachable, so asserting otherwise would be a false failure. It is meant
//! to be run explicitly, ONLY after network egress has been blocked for the
//! test process — e.g. inside `core.yml`'s `unshare --net` step on the
//! Linux CI runner:
//!
//! ```sh
//! sudo unshare --net --map-root-user cargo test --workspace --locked -- --ignored
//! ```
//!
//! If network isolation is NOT actually active, this test fails loudly
//! (rather than silently passing), which is exactly the point: it is the
//! CI's proof that the zero-network guarantee holds, not just an assumption.

use std::time::Duration;

use zero_network_guard::assert_offline_against_public_internet;

#[test]
#[ignore = "run only inside the network-isolated CI step (see module docs)"]
fn public_internet_is_unreachable_in_ci_network_namespace() {
    let leaks = assert_offline_against_public_internet(Duration::from_secs(2));

    assert!(
        leaks.is_empty(),
        "zero-network CI enforcement failed — these endpoints were reachable, meaning \
         outbound network egress leaked from the test environment: {leaks:?}"
    );
}
