//! Error type shared by the signing port and signature serialization.

use thiserror::Error;

use crate::{DigestAlgorithm, SigningAlgorithm};

/// Errors reported while preparing a signature placeholder or asking a
/// certificate source to sign a digest.
#[derive(Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum SignError {
    /// The selected identity no longer exists or its private key is unavailable.
    #[error("signing identity `{identity_id}` is unavailable")]
    IdentityUnavailable {
        /// Opaque identifier originally returned by the certificate source.
        identity_id: String,
    },
    /// The selected identity cannot use the requested signature mechanism.
    #[error("signing identity `{identity_id}` does not support {algorithm}")]
    UnsupportedAlgorithm {
        /// Opaque identifier originally returned by the certificate source.
        identity_id: String,
        /// Mechanism rejected by the certificate source.
        algorithm: SigningAlgorithm,
    },
    /// The digest length does not match the requested digest algorithm.
    #[error("invalid digest length for {algorithm}: expected {expected} bytes, received {actual}")]
    InvalidDigestLength {
        /// Digest algorithm whose output size was expected.
        algorithm: DigestAlgorithm,
        /// Required digest length in bytes.
        expected: usize,
        /// Supplied digest length in bytes.
        actual: usize,
    },
    /// The user cancelled an operating-system signing prompt.
    #[error("signing was cancelled by the user")]
    UserCancelled,
    /// The platform certificate source failed for another recoverable reason.
    #[error("certificate source failed: {message}")]
    Backend {
        /// Adapter-provided diagnostic detail.
        message: String,
    },
    /// A signature placeholder was requested with no room for a CMS value.
    #[error("signature placeholder capacity must be greater than zero")]
    InvalidPlaceholderCapacity,
    /// The serialized PDF does not contain the expected signature placeholder.
    #[error("serialized PDF does not contain a signature placeholder")]
    PlaceholderNotFound,
    /// More than one unsigned placeholder was found, so choosing one is unsafe.
    #[error("serialized PDF contains {count} signature placeholders; expected exactly one")]
    AmbiguousPlaceholder {
        /// Number of unsigned placeholders present in the serialized PDF.
        count: usize,
    },
    /// The placeholder exists but its `/Contents` token is malformed.
    #[error("serialized signature placeholder has malformed /Contents bytes")]
    MalformedPlaceholder,
    /// An offset cannot fit in the fixed-width `/ByteRange` slots.
    #[error("serialized PDF length {length} exceeds the supported signature offset range")]
    DocumentTooLarge {
        /// Serialized document length in bytes.
        length: usize,
    },
}
