//! Offline PDF cryptographic signing and structural signature verification.
//!
//! This crate owns the cryptographic formats used by PDF signatures while
//! leaving private-key access to platform adapters. It is an isolated
//! workspace leaf: viewer, render, manipulation, annotation, and save crates
//! do not depend on it, so builds that omit signing do not pull this crate's
//! cryptographic dependency graph.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod cms;
mod digest;
mod encoding;
mod error;
mod orchestrate;
mod port;
mod signature;

pub use cms::CmsSignedDataBuilder;
pub use digest::{digest_byte_ranges, DocumentDigest};
pub use encoding::{der_encode_ecdsa_signature, der_integer, der_length, rsa_digest_info};
pub use error::SignError;
pub use orchestrate::sign_document;
pub use port::{CertificateSourcePort, DigestAlgorithm, SigningAlgorithm, SigningIdentity};
pub use signature::{
    append_signature_bytes, prepare_signature_bytes, ByteRange, PreparedSignature,
    SignatureFieldBuilder, SignaturePlaceholder, DEFAULT_SIGNATURE_CAPACITY,
};

#[cfg(test)]
mod tests {
    use cms::content_info::ContentInfo;
    use der::Decode;
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
