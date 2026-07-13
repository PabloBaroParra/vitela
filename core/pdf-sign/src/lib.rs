//! Offline PDF cryptographic signing and structural signature verification.
//!
//! This crate owns the cryptographic formats used by PDF signatures while
//! leaving private-key access to platform adapters. It is an isolated
//! workspace leaf: viewer, render, manipulation, annotation, and save crates
//! do not depend on it, so builds that omit signing do not pull this crate's
//! cryptographic dependency graph.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod port;
mod signature;

pub use error::SignError;
pub use port::{CertificateSourcePort, DigestAlgorithm, SigningAlgorithm, SigningIdentity};
pub use signature::{
    prepare_signature_bytes, ByteRange, PreparedSignature, SignatureFieldBuilder,
    SignaturePlaceholder, DEFAULT_SIGNATURE_CAPACITY,
};

#[cfg(test)]
mod tests {
    use cms::content_info::ContentInfo;
    use der::Decode;
    use sha2::{Digest, Sha256, Sha384, Sha512};
    use signature::hazmat::PrehashSigner;
    use spki::AlgorithmIdentifierOwned;
    use x509_cert::Certificate;

    #[test]
    fn cms_types_use_the_selected_der_surface() {
        assert!(ContentInfo::from_der(&[]).is_err());
    }

    #[test]
    fn x509_types_use_the_selected_der_surface() {
        assert!(Certificate::from_der(&[]).is_err());
    }

    #[test]
    fn spki_types_use_the_selected_der_surface() {
        assert!(AlgorithmIdentifierOwned::from_der(&[]).is_err());
    }

    #[test]
    fn sha256_is_available() {
        assert_eq!(Sha256::digest(b"pdf-sign").len(), 32);
    }

    #[test]
    fn sha384_is_available() {
        assert_eq!(Sha384::digest(b"pdf-sign").len(), 48);
    }

    #[test]
    fn sha512_is_available() {
        assert_eq!(Sha512::digest(b"pdf-sign").len(), 64);
    }

    struct EchoSigner;

    impl PrehashSigner<Vec<u8>> for EchoSigner {
        fn sign_prehash(&self, prehash: &[u8]) -> signature::Result<Vec<u8>> {
            Ok(prehash.to_vec())
        }
    }

    #[test]
    fn prehash_signature_trait_accepts_external_signature_bytes() {
        let signature = EchoSigner
            .sign_prehash(b"digest")
            .expect("test signer should succeed");

        assert_eq!(signature, b"digest");
    }
}
