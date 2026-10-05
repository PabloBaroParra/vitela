//! Certificate-source adapters for tokens and native system stores.

use crate::{FfiError, SigningCertificate};
use pdf_sign::{
    CertificateSourcePort, DigestAlgorithm, SignError, SigningAlgorithm, SigningIdentity,
};
use std::sync::Arc;

/// Native-store mechanisms. Digests are SHA-256; ECDSA results use ASN.1 DER.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum FfiSigningAlgorithm {
    /// RSA PKCS#1 v1.5 / SHA-256.
    RsaSha256,
    /// ECDSA / SHA-256.
    EcdsaSha256,
}

impl FfiSigningAlgorithm {
    fn core(self) -> SigningAlgorithm {
        match self {
            Self::RsaSha256 => SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256),
            Self::EcdsaSha256 => SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256),
        }
    }
}

/// Public identity metadata from a platform certificate store.
#[derive(Clone, Debug, uniffi::Record)]
pub struct FfiPlatformSigningIdentity {
    /// Store-local stable identifier.
    pub id: String,
    /// Certificate subject for presentation.
    pub display_name: String,
    /// Public certificates ordered leaf first.
    pub certificate_chain_der: Vec<Vec<u8>>,
    /// Mechanism the native key supports.
    pub algorithm: FfiSigningAlgorithm,
}

/// Platform adapter retaining private keys and signing digests offline.
#[uniffi::export(callback_interface)]
pub trait PlatformSigningSource: Send + Sync {
    /// Public identities available to this source.
    fn identities(&self) -> Result<Vec<FfiPlatformSigningIdentity>, FfiError>;
    /// Sign a SHA-256 digest with the previously advertised identity.
    fn sign_digest(
        &self,
        identity_id: String,
        digest: Vec<u8>,
        algorithm: FfiSigningAlgorithm,
    ) -> Result<Vec<u8>, FfiError>;
}

struct PlatformSource {
    callback: Box<dyn PlatformSigningSource>,
    identities: Vec<FfiPlatformSigningIdentity>,
}

impl CertificateSourcePort for PlatformSource {
    fn list_identities(&self) -> Vec<SigningIdentity> {
        self.identities
            .iter()
            .map(|identity| SigningIdentity {
                id: identity.id.clone(),
                display_name: identity.display_name.clone(),
                certificate_chain_der: identity.certificate_chain_der.clone(),
                supported_algorithms: vec![identity.algorithm.core()],
            })
            .collect()
    }

    fn sign_digest_raw(
        &self,
        id: &str,
        digest: &[u8],
        algorithm: SigningAlgorithm,
    ) -> Result<Vec<u8>, SignError> {
        let identity = self
            .identities
            .iter()
            .find(|identity| identity.id == id)
            .ok_or_else(|| SignError::IdentityUnavailable {
                identity_id: id.to_owned(),
            })?;
        if identity.algorithm.core() != algorithm {
            return Err(SignError::UnsupportedAlgorithm {
                identity_id: id.to_owned(),
                algorithm,
            });
        }
        self.callback
            .sign_digest(id.to_owned(), digest.to_vec(), identity.algorithm)
            .map_err(|error| SignError::Backend {
                message: error.to_string(),
            })
    }
}

/// Opens a native-store source without transferring private keys.
///
/// # Errors
/// Returns the platform's discovery failure.
#[uniffi::export]
pub fn open_platform_signing_source(
    source: Box<dyn PlatformSigningSource>,
) -> Result<Arc<SigningCertificate>, FfiError> {
    let identities = source.identities()?;
    Ok(SigningCertificate::from_source(Box::new(PlatformSource {
        callback: source,
        identities,
    })))
}

/// Opens a PKCS#11 card/token module. A missing PIN permits protected
/// authentication paths; signing failures are surfaced by the adapter.
///
/// # Errors
/// Returns a readable module initialization error.
#[uniffi::export]
pub fn open_token_signing_source(
    module_path: String,
    pin: Option<String>,
) -> Result<Arc<SigningCertificate>, FfiError> {
    pdf_sign_pkcs11::Pkcs11CertificateSource::load(module_path, pin)
        .map(|source| SigningCertificate::from_source(Box::new(source)))
        .map_err(|error| FfiError::UnsupportedOperation {
            detail: error.to_string(),
        })
}
