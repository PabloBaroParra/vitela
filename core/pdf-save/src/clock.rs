//! Deterministic clock + ID-generator injectable hooks (T-036).
//!
//! Per `design.md`'s "Deterministic writer hooks": `pdf-save` takes its clock
//! and trailer-`/ID` generator as **injectable dependencies**, not hardcoded
//! `SystemTime::now()`/global counters. Production wiring
//! ([`SystemClock`]/[`RandomIdGenerator`]) uses real time/randomness; CI
//! wiring ([`FixedClock`]/[`SequentialIdGenerator`]) injects fixed values so
//! [`crate::save_document`] can be asserted byte-identical across runs given
//! the same input + edit sequence (T-038).
//!
//! Object numbering itself (lopdf's `Document::add_object`/`new_object_id`)
//! is already deterministic given a fixed call order — this crate never
//! reorders its own writes based on wall-clock time or randomness, so the
//! only real non-determinism sources in the writer are `/ModDate` (this
//! module's [`Clock`]) and the trailer `/ID` second element (this module's
//! [`IdGenerator`]).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Produces the `/ModDate` (and, for newly-created documents, `/CreationDate`)
/// string written into a saved PDF's `/Info` dictionary.
pub trait Clock: Send + Sync {
    /// Returns the current time as a PDF date string, e.g. `D:20260713000000Z`
    /// (PDF 32000-1:2008 §7.9.4).
    fn pdf_date_string(&self) -> String;
}

/// Production clock: wraps `SystemTime::now()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn pdf_date_string(&self) -> String {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        pdf_date_string_from_unix_secs(secs)
    }
}

/// CI/test clock: always returns the same fixed instant, injected at
/// construction — see T-038's byte-identical determinism check.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    unix_secs: u64,
}

impl FixedClock {
    pub fn new(unix_secs: u64) -> Self {
        Self { unix_secs }
    }
}

impl Clock for FixedClock {
    fn pdf_date_string(&self) -> String {
        pdf_date_string_from_unix_secs(self.unix_secs)
    }
}

/// Formats a Unix timestamp as a PDF date string (UTC, no sub-second
/// precision — sufficient for `/ModDate`/`/CreationDate`).
fn pdf_date_string_from_unix_secs(unix_secs: u64) -> String {
    const SECS_PER_DAY: u64 = 86_400;
    let days_since_epoch = unix_secs / SECS_PER_DAY;
    let secs_of_day = unix_secs % SECS_PER_DAY;
    let (year, month, day) = civil_from_days(days_since_epoch as i64);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("D:{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}Z")
}

/// Howard Hinnant's `civil_from_days` algorithm (days-since-epoch -> proleptic
/// Gregorian y/m/d), avoiding a chrono dependency for this one conversion.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// Produces the raw bytes used for the trailer `/ID` array (both elements on
/// first save; the second element on every subsequent incremental update,
/// per PDF 32000-1:2008 §14.4).
pub trait IdGenerator: Send + Sync {
    /// Returns a fresh id value. Called once per save.
    fn next_id(&self) -> Vec<u8>;
}

/// Production id generator: process-lifetime monotonic counter mixed with
/// the current time, run through `DefaultHasher` — "random enough" for a
/// file identifier without pulling in a `rand` dependency for one call site.
#[derive(Debug, Default)]
pub struct RandomIdGenerator {
    counter: AtomicU64,
}

impl RandomIdGenerator {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IdGenerator for RandomIdGenerator {
    fn next_id(&self) -> Vec<u8> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let seq = self.counter.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        let mut hasher = DefaultHasher::new();
        seq.hash(&mut hasher);
        now.hash(&mut hasher);
        hasher.finish().to_be_bytes().to_vec()
    }
}

/// CI/test id generator: deterministic, incrementing from a fixed seed — see
/// T-038's byte-identical determinism check.
#[derive(Debug)]
pub struct SequentialIdGenerator {
    next: AtomicU64,
}

impl SequentialIdGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            next: AtomicU64::new(seed),
        }
    }
}

impl IdGenerator for SequentialIdGenerator {
    fn next_id(&self) -> Vec<u8> {
        let value = self.next.fetch_add(1, Ordering::Relaxed);
        value.to_be_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_stable_across_calls() {
        let clock = FixedClock::new(1_752_364_800); // 2025-07-13T00:00:00Z
        assert_eq!(clock.pdf_date_string(), clock.pdf_date_string());
        assert!(clock.pdf_date_string().starts_with("D:2025"));
    }

    #[test]
    fn fixed_clock_formats_known_epoch_correctly() {
        // 2000-01-01T00:00:00Z = 946684800
        let clock = FixedClock::new(946_684_800);
        assert_eq!(clock.pdf_date_string(), "D:20000101000000Z");
    }

    #[test]
    fn sequential_id_generator_increments_deterministically() {
        let generator = SequentialIdGenerator::new(42);
        assert_eq!(generator.next_id(), 42u64.to_be_bytes().to_vec());
        assert_eq!(generator.next_id(), 43u64.to_be_bytes().to_vec());
    }

    #[test]
    fn two_sequential_generators_with_same_seed_produce_identical_sequences() {
        let a = SequentialIdGenerator::new(7);
        let b = SequentialIdGenerator::new(7);
        for _ in 0..5 {
            assert_eq!(a.next_id(), b.next_id());
        }
    }

    #[test]
    fn random_id_generator_produces_distinct_ids() {
        let generator = RandomIdGenerator::new();
        let a = generator.next_id();
        let b = generator.next_id();
        assert_ne!(a, b);
    }
}
