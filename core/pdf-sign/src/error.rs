//! Error type shared by digesting, the signing port, and signature serialization.

use thiserror::Error;

use crate::{ByteRange, DigestAlgorithm, SigningAlgorithm};

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
    /// The digest was produced by a different algorithm than the signature
    /// scheme expects.
    #[error("digest was produced with {digest} but the signing algorithm is {signing}")]
    DigestAlgorithmMismatch {
        /// Algorithm that produced the digest bytes.
        digest: DigestAlgorithm,
        /// Signature scheme the caller asked the adapter to use.
        signing: SigningAlgorithm,
    },
    /// A PDF `/ByteRange` points outside the supplied document bytes.
    #[error("invalid PDF byte range {byte_range:?} for document length {document_length}")]
    InvalidByteRange {
        /// The offsets and lengths from the PDF signature dictionary.
        byte_range: ByteRange,
        /// Number of available document bytes.
        document_length: usize,
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
    /// The selected identity did not provide a signer certificate.
    #[error("signing identity `{identity_id}` has no certificate chain")]
    MissingCertificateChain {
        /// Opaque identifier of the identity with no certificates.
        identity_id: String,
    },
    /// One certificate supplied by the selected identity is not valid DER.
    #[error("certificate {index} in the signing identity chain is invalid: {message}")]
    InvalidCertificate {
        /// Zero-based position in the leaf-first certificate chain.
        index: usize,
        /// DER decoder diagnostic.
        message: String,
    },
    /// A CMS value could not be represented or serialized as DER.
    #[error("CMS encoding failed: {message}")]
    CmsEncoding {
        /// DER encoder diagnostic.
        message: String,
    },
    /// The certificate source returned no signature bytes.
    #[error("certificate source returned an empty signature")]
    EmptySignature,
    /// An ECDSA signature returned by a certificate source is not a DER
    /// `ECDSA-Sig-Value` sequence.
    #[error("certificate source returned an invalid DER ECDSA signature")]
    InvalidEcdsaSignature,
    /// The requested signing algorithm does not match the leaf certificate's
    /// SubjectPublicKeyInfo algorithm.
    #[error(
        "signing identity `{identity_id}` has public-key algorithm {public_key_algorithm}, which is incompatible with {signing}"
    )]
    IncompatibleCertificateAlgorithm {
        /// Opaque identifier originally returned by the certificate source.
        identity_id: String,
        /// Object identifier from the leaf certificate SubjectPublicKeyInfo.
        public_key_algorithm: String,
        /// Signature scheme requested for the CMS signer.
        signing: SigningAlgorithm,
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
    /// The DER signature does not fit the placeholder's reserved capacity.
    #[error(
        "signature of {signature_length} bytes exceeds the reserved placeholder capacity of {capacity} bytes"
    )]
    SignatureTooLarge {
        /// DER-encoded CMS signature length in bytes.
        signature_length: usize,
        /// Maximum DER bytes the `/Contents` placeholder can hold.
        capacity: usize,
    },
}
