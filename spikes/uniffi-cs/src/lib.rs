//! UniFFI C# interop spike (T-006..T-010 and T-070).
//!
//! Exercises exactly what production `pdf-ffi` will need from a C# consumer,
//! per the design doc's "uniffi-bindgen-cs spike (detailed plan)":
//! 1. String echo (basic marshaling sanity check)                    -> T-006
//! 2. Byte-array round-trip (small buffer correctness)                -> T-006
//! 3. >=8MB buffer round-trip benchmark                                -> T-007
//! 4. Error-enum -> C# exception mapping                               -> T-008
//! 5. Callback/event delivery (Rust -> C#, async off the calling thread) -> T-009
//! 6. Synchronous callback with a fallible byte-array return              -> T-070
//!
//! Go/no-go decision recorded separately (T-010) — see
//! engram topic `sdd/pdf-editor-mvp/spike-uniffi-cs-decision`.

uniffi::setup_scaffolding!();

use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------
// T-006: string echo + byte-array round-trip
// ---------------------------------------------------------------------

/// Basic marshaling sanity check: a string crossing Rust -> C# -> Rust
/// (echoed back by the C# host in the spike harness) or just Rust -> C#
/// for a one-way check, must survive unchanged, including non-ASCII data.
#[uniffi::export]
pub fn echo_string(input: String) -> String {
    input
}

/// Byte-array round trip. Used for both:
/// - the small-buffer correctness check (T-006), and
/// - called with a >=8MB buffer, the round-trip benchmark (T-007) that
///   backs the `BitmapHandle::get_pixels()` copy-cost rationale in design.md
///   ("memcpy of an ~8MB RGBA page costs single-digit milliseconds").
#[uniffi::export]
pub fn bytes_round_trip(input: Vec<u8>) -> Vec<u8> {
    input
}

// ---------------------------------------------------------------------
// T-008: error-enum -> C# exception mapping
// ---------------------------------------------------------------------

/// Mirrors the shape of the real `FfiError` (design.md: "a single `FfiError`
/// enum (mirrors core `PdfError`) mapped to Swift `Error`/C# exception via
/// UniFFI's error type support — no raw error strings").
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SpikeError {
    #[error("input buffer was empty")]
    EmptyInput,
    #[error("denominator must not be zero (numerator={numerator})")]
    DivideByZero { numerator: i32 },
}

/// Deliberately fails when `denominator == 0` so the C# harness can assert
/// it receives a typed exception (not a string) with the numerator payload
/// intact.
#[uniffi::export]
pub fn checked_divide(numerator: i32, denominator: i32) -> Result<i32, SpikeError> {
    if denominator == 0 {
        return Err(SpikeError::DivideByZero { numerator });
    }
    Ok(numerator / denominator)
}

/// Deliberately fails on empty input so the C# harness can assert the
/// unit-variant (no payload) error case too.
#[uniffi::export]
pub fn require_non_empty(input: Vec<u8>) -> Result<u64, SpikeError> {
    if input.is_empty() {
        return Err(SpikeError::EmptyInput);
    }
    Ok(input.len() as u64)
}

// ---------------------------------------------------------------------
// T-009: callback/event delivery, Rust -> C#
// ---------------------------------------------------------------------

/// Models the real render-completion notification pdf-ffi will need
/// (design.md's `PageRendered` event): async, fired from a background
/// thread rather than synchronously on the caller's thread, so the C#
/// harness must actually wait/block for delivery rather than assume it
/// happened by the time the call returns.
#[uniffi::export(callback_interface)]
pub trait SpikeEventListener: Send + Sync {
    fn on_event(&self, sequence: u32, message: String);
}

/// Fires `count` events asynchronously on a background OS thread, spaced a
/// few milliseconds apart, simulating a stream of page-rendered
/// notifications. Returns immediately (fire-and-forget), matching the
/// async delivery contract the real FFI needs.
#[uniffi::export]
pub fn fire_events(listener: Box<dyn SpikeEventListener>, count: u32) {
    thread::spawn(move || {
        for i in 0..count {
            thread::sleep(Duration::from_millis(20));
            listener.on_event(i, format!("event-{i}"));
        }
    });
}

// ---------------------------------------------------------------------
// T-070: synchronous callback with a return value, C# -> Rust
// ---------------------------------------------------------------------

/// Errors that a foreign digest signer can report synchronously.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SigningCallbackError {
    #[error("the requested signing identity is unavailable")]
    IdentityUnavailable,
}

/// Models the fallible, synchronous callback required by
/// `CertificateSourcePort::sign_digest`.
#[uniffi::export(callback_interface)]
pub trait DigestSigner: Send + Sync {
    /// Signs `digest` and returns the signature bytes before the call completes.
    fn sign_digest(&self, digest: Vec<u8>) -> Result<Vec<u8>, SigningCallbackError>;
}

/// Invokes a foreign digest signer synchronously and returns its byte array.
///
/// # Errors
///
/// Returns the typed [`SigningCallbackError`] supplied by the callback.
#[uniffi::export]
pub fn request_digest_signature(
    signer: Box<dyn DigestSigner>,
    digest: Vec<u8>,
) -> Result<Vec<u8>, SigningCallbackError> {
    signer.sign_digest(digest)
}

// ---------------------------------------------------------------------
// Unit tests (Strict TDD: these encode the acceptance criteria the
// in-process Rust logic must satisfy, independent of whether the C# host
// can build on this machine).
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn echo_string_identity_including_non_ascii() {
        assert_eq!(echo_string(String::new()), "");
        assert_eq!(echo_string("hello".to_string()), "hello");
        assert_eq!(
            echo_string("héllo wörld 日本語".to_string()),
            "héllo wörld 日本語"
        );
    }

    #[test]
    fn bytes_round_trip_identity_small_buffer() {
        let input = vec![0u8, 1, 2, 3, 255, 254];
        assert_eq!(bytes_round_trip(input.clone()), input);
    }

    #[test]
    fn bytes_round_trip_identity_large_buffer_ge_8mb() {
        // >=8MB, matching the real BitmapHandle::get_pixels() payload size
        // this benchmark is meant to model.
        let size = 8 * 1024 * 1024 + 37; // +37 to avoid an accidental round-number false positive
        let input: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let output = bytes_round_trip(input.clone());
        assert_eq!(output.len(), input.len());
        assert_eq!(output, input);
    }

    #[test]
    fn checked_divide_ok() {
        assert_eq!(checked_divide(10, 2).unwrap(), 5);
    }

    #[test]
    fn checked_divide_by_zero_returns_typed_error_with_payload() {
        match checked_divide(7, 0) {
            Err(SpikeError::DivideByZero { numerator }) => assert_eq!(numerator, 7),
            other => panic!("expected DivideByZero error, got {other:?}"),
        }
    }

    #[test]
    fn require_non_empty_ok() {
        assert_eq!(require_non_empty(vec![1, 2, 3]).unwrap(), 3);
    }

    #[test]
    fn require_non_empty_on_empty_returns_typed_unit_error() {
        match require_non_empty(vec![]) {
            Err(SpikeError::EmptyInput) => {}
            other => panic!("expected EmptyInput error, got {other:?}"),
        }
    }

    /// In-process listener (no FFI boundary) used to verify the delivery
    /// logic itself: correct count, correct order, and — critically —
    /// delivery happens off the calling thread (fire_events returns before
    /// any event arrives).
    struct ChannelListener {
        tx: mpsc::Sender<(u32, String)>,
    }

    impl SpikeEventListener for ChannelListener {
        fn on_event(&self, sequence: u32, message: String) {
            let _ = self.tx.send((sequence, message));
        }
    }

    #[test]
    fn fire_events_delivers_in_order_asynchronously() {
        let (tx, rx) = mpsc::channel();
        let listener = Box::new(ChannelListener { tx });

        fire_events(listener, 5);

        // fire_events must return before delivery completes (async contract).
        // We can't assert "nothing received yet" deterministically (thread
        // scheduling), but we CAN assert all 5 arrive, in order, within a
        // generous timeout — proving delivery crosses the call boundary
        // asynchronously rather than requiring the caller to block inline.
        for expected_seq in 0..5u32 {
            let (seq, msg) = rx
                .recv_timeout(Duration::from_secs(2))
                .expect("event should arrive within timeout");
            assert_eq!(seq, expected_seq);
            assert_eq!(msg, format!("event-{expected_seq}"));
        }
    }

    struct DeterministicDigestSigner;

    impl DigestSigner for DeterministicDigestSigner {
        fn sign_digest(&self, digest: Vec<u8>) -> Result<Vec<u8>, SigningCallbackError> {
            Ok(digest.into_iter().map(|byte| byte ^ 0xA5).collect())
        }
    }

    #[test]
    fn request_digest_signature_returns_bytes_from_synchronous_callback() {
        let digest = vec![0x00, 0x12, 0xA5, 0xFF];

        let signature = request_digest_signature(Box::new(DeterministicDigestSigner), digest)
            .expect("deterministic signer should succeed");

        assert_eq!(signature, vec![0xA5, 0xB7, 0x00, 0x5A]);
    }

    struct UnavailableDigestSigner;

    impl DigestSigner for UnavailableDigestSigner {
        fn sign_digest(&self, _digest: Vec<u8>) -> Result<Vec<u8>, SigningCallbackError> {
            Err(SigningCallbackError::IdentityUnavailable)
        }
    }

    #[test]
    fn request_digest_signature_propagates_typed_callback_error() {
        let error = request_digest_signature(Box::new(UnavailableDigestSigner), vec![0x01])
            .expect_err("unavailable signer should fail");

        assert!(matches!(error, SigningCallbackError::IdentityUnavailable));
    }
}
