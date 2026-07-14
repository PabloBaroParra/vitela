//! Certificate identity discovery and digest-signing boundary.

use std::fmt;

use crate::{DocumentDigest, SignError};

/// Hash algorithm used to produce the digest passed to a certificate source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DigestAlgorithm {
    /// SHA-256.
    Sha256,
    /// SHA-384.
    Sha384,
    /// SHA-512.
    Sha512,
}

impl DigestAlgorithm {
    /// Returns the digest output length in bytes.
    ///
    /// This is the `expected` length [`DocumentDigest::new`] reports through
    /// [`SignError::InvalidDigestLength`] when supplied bytes do not match
    /// the requested algorithm.
    #[must_use]
    pub const fn digest_len(self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }
}

impl fmt::Display for DigestAlgorithm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Sha256 => "SHA-256",
            Self::Sha384 => "SHA-384",
            Self::Sha512 => "SHA-512",
        })
    }
}

/// Public-key signature scheme and digest combination requested from a key.
///
/// The certificate source signs a precomputed digest. Keeping the digest
/// algorithm in this value prevents a platform adapter from having to infer
/// the signature mechanism from the digest length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SigningAlgorithm {
    /// RSASSA-PKCS1-v1_5 with the given digest algorithm.
    RsaPkcs1v15(DigestAlgorithm),
    /// ECDSA with the given digest algorithm.
    Ecdsa(DigestAlgorithm),
}

impl SigningAlgorithm {
    /// Returns the digest algorithm carried by this signature scheme.
    #[must_use]
    pub const fn digest_algorithm(self) -> DigestAlgorithm {
        match self {
            Self::RsaPkcs1v15(digest) | Self::Ecdsa(digest) => digest,
        }
    }
}

impl fmt::Display for SigningAlgorithm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RsaPkcs1v15(digest) => write!(formatter, "RSA PKCS#1 v1.5 with {digest}"),
            Self::Ecdsa(digest) => write!(formatter, "ECDSA with {digest}"),
        }
    }
}

/// A certificate and private-key identity exposed by a platform adapter.
///
/// `id` is opaque to `pdf-sign` and is only passed back to the adapter that
/// produced it. `certificate_chain_der` is ordered leaf first so a later CMS
/// builder can embed it without accessing the platform certificate store
/// again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SigningIdentity {
    /// Stable adapter-local identifier for the certificate/private-key pair.
    pub id: String,
    /// Human-readable label suitable for an identity picker.
    pub display_name: String,
    /// DER-encoded X.509 certificates, ordered from leaf to root.
    pub certificate_chain_der: Vec<Vec<u8>>,
    /// Signature mechanisms supported by this identity's private key.
    pub supported_algorithms: Vec<SigningAlgorithm>,
}

/// Boundary implemented by platform certificate stores and file adapters.
///
/// Implementations retain private keys and perform the signature operation;
/// `pdf-sign` only receives public certificate bytes and the resulting
/// signature. The trait is object-safe so shells can install platform-specific
/// implementations behind `dyn CertificateSourcePort`.
pub trait CertificateSourcePort: Send + Sync {
    /// Lists identities currently available for offline signing.
    ///
    /// An empty vector means that no usable certificate is available.
    fn list_identities(&self) -> Vec<SigningIdentity>;

    /// Raw signing hook implemented by platform adapters.
    ///
    /// Only [`sign_digest`](Self::sign_digest) calls this method; it has
    /// already checked that `digest` was produced by the algorithm carried in
    /// `algorithm`, so adapters can forward the bytes to their key store
    /// without re-validating them. For [`SigningAlgorithm::Ecdsa`], a
    /// successful result must be DER-encoded `ECDSA-Sig-Value`: a sequence of
    /// two positive ASN.1 INTEGER values (`r` and `s`). The CMS builder rejects
    /// any other ECDSA representation, including fixed-width raw `r || s`
    /// bytes. RSA results are the signature octets returned by the key store.
    ///
    /// # Errors
    ///
    /// Returns [`SignError`] when the identity disappears, the requested
    /// algorithm is unsupported, the user cancels a platform prompt, or the
    /// backing store fails.
    fn sign_digest_raw(
        &self,
        identity_id: &str,
        digest: &[u8],
        algorithm: SigningAlgorithm,
    ) -> Result<Vec<u8>, SignError>;

    /// Signs a precomputed digest synchronously with the identity whose
    /// [`SigningIdentity::id`] equals `identity_id`.
    ///
    /// Only the opaque identifier crosses the boundary; the adapter already
    /// holds the certificate and key material it advertised through
    /// [`list_identities`](Self::list_identities). Adapters must not override
    /// this method: it is the validation layer in front of
    /// [`sign_digest_raw`](Self::sign_digest_raw).
    ///
    /// # Errors
    ///
    /// Returns [`SignError::DigestAlgorithmMismatch`] before reaching the
    /// adapter when `digest` was produced by a different algorithm than
    /// `algorithm` carries. Otherwise returns any error produced by
    /// [`sign_digest_raw`](Self::sign_digest_raw).
    fn sign_digest(
        &self,
        identity_id: &str,
        digest: &DocumentDigest,
        algorithm: SigningAlgorithm,
    ) -> Result<Vec<u8>, SignError> {
        let expected = algorithm.digest_algorithm();
        if digest.algorithm() != expected {
            return Err(SignError::DigestAlgorithmMismatch {
                digest: digest.algorithm(),
                signing: algorithm,
            });
        }
        self.sign_digest_raw(identity_id, digest.as_bytes(), algorithm)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn identity() -> SigningIdentity {
        SigningIdentity {
            id: "test-key".to_owned(),
            display_name: "Test signer".to_owned(),
            certificate_chain_der: vec![vec![0x30, 0x00]],
            supported_algorithms: vec![SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256)],
        }
    }

    fn sha256_digest() -> DocumentDigest {
        DocumentDigest::new(DigestAlgorithm::Sha256, vec![0xA5; 32])
            .expect("test digest should wrap")
    }

    struct DeterministicCertificateSource;

    impl CertificateSourcePort for DeterministicCertificateSource {
        fn list_identities(&self) -> Vec<SigningIdentity> {
            vec![identity()]
        }

        fn sign_digest_raw(
            &self,
            _identity_id: &str,
            digest: &[u8],
            _algorithm: SigningAlgorithm,
        ) -> Result<Vec<u8>, SignError> {
            Ok(digest.to_vec())
        }
    }

    struct UnavailableCertificateSource;

    impl CertificateSourcePort for UnavailableCertificateSource {
        fn list_identities(&self) -> Vec<SigningIdentity> {
            Vec::new()
        }

        fn sign_digest_raw(
            &self,
            identity_id: &str,
            _digest: &[u8],
            _algorithm: SigningAlgorithm,
        ) -> Result<Vec<u8>, SignError> {
            Err(SignError::IdentityUnavailable {
                identity_id: identity_id.to_owned(),
            })
        }
    }

    struct RecordingCertificateSource {
        calls: AtomicUsize,
    }

    impl CertificateSourcePort for RecordingCertificateSource {
        fn list_identities(&self) -> Vec<SigningIdentity> {
            Vec::new()
        }

        fn sign_digest_raw(
            &self,
            _identity_id: &str,
            digest: &[u8],
            _algorithm: SigningAlgorithm,
        ) -> Result<Vec<u8>, SignError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(digest.to_vec())
        }
    }

    #[test]
    fn sign_digest_preserves_typed_adapter_error() {
        let error = UnavailableCertificateSource
            .sign_digest(
                &identity().id,
                &sha256_digest(),
                SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256),
            )
            .expect_err("unavailable identity should fail");

        assert_eq!(
            error,
            SignError::IdentityUnavailable {
                identity_id: "test-key".to_owned()
            }
        );
    }

    #[test]
    fn sign_digest_rejects_algorithm_mismatch_before_calling_adapter() {
        let source = RecordingCertificateSource {
            calls: AtomicUsize::new(0),
        };
        let algorithm = SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha384);

        let error = source
            .sign_digest("test-key", &sha256_digest(), algorithm)
            .expect_err("SHA-256 digest signed as SHA-384 should fail");

        assert_eq!(
            (error, source.calls.load(Ordering::Relaxed)),
            (
                SignError::DigestAlgorithmMismatch {
                    digest: DigestAlgorithm::Sha256,
                    signing: algorithm,
                },
                0,
            )
        );
    }

    #[test]
    fn sign_digest_dispatches_matching_digest_to_adapter() {
        let source = RecordingCertificateSource {
            calls: AtomicUsize::new(0),
        };
        let digest = sha256_digest();

        let signature = source
            .sign_digest(
                "test-key",
                &digest,
                SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256),
            )
            .expect("matching digest should reach adapter");

        assert_eq!(
            (signature, source.calls.load(Ordering::Relaxed)),
            (digest.as_bytes().to_vec(), 1)
        );
    }

    #[test]
    fn certificate_source_port_is_object_safe() {
        let source: &dyn CertificateSourcePort = &DeterministicCertificateSource;

        assert_eq!(source.list_identities().len(), 1);
    }

    #[test]
    fn signing_algorithm_reports_its_digest_algorithm() {
        let algorithm = SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha384);

        assert_eq!(algorithm.digest_algorithm(), DigestAlgorithm::Sha384);
    }

    #[test]
    fn digest_algorithms_report_their_output_lengths() {
        assert_eq!(DigestAlgorithm::Sha256.digest_len(), 32);
        assert_eq!(DigestAlgorithm::Sha384.digest_len(), 48);
        assert_eq!(DigestAlgorithm::Sha512.digest_len(), 64);
    }
}
