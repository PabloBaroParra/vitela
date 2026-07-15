//! PKCS#12 (`.p12`/`.pfx`) implementation of [`pdf_sign::CertificateSourcePort`].
//! Private keys are imported only to sign within this adapter and are never
//! exposed through the certificate-source boundary.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use p12_keystore::{KeyStore, KeyStoreEntry, Pkcs12ImportPolicy};
use p256::ecdsa::{Signature as P256Signature, SigningKey as P256SigningKey};
use pdf_sign::{
    CertificateSourcePort, DigestAlgorithm, SignError, SigningAlgorithm, SigningIdentity,
};
use rsa::pkcs1v15::SigningKey as RsaSigningKey;
use rsa::{
    pkcs8::{AssociatedOid, DecodePrivateKey as _, EncodePublicKey as _},
    RsaPrivateKey,
};
use sha2::{Sha256, Sha384, Sha512};
use signature::{hazmat::PrehashSigner, SignatureEncoding};
use thiserror::Error;
use x509_cert::{
    der::{Decode, Encode},
    Certificate as X509Certificate,
};

/// Failure while reading or parsing a PKCS#12/PFX certificate source.
#[derive(Debug, Error)]
pub enum PfxAdapterError {
    /// The `.p12` or `.pfx` file could not be read.
    #[error("failed to read PFX file: {0}")]
    File(String),
    /// The PKCS#12 container could not be decoded with the supplied password.
    #[error("failed to parse PKCS#12 data: {0}")]
    Pkcs12(String),
}

/// Certificate source backed by an in-process `.p12` or `.pfx` file.
pub struct PfxCertificateSource {
    identities: BTreeMap<String, StoredIdentity>,
}

impl fmt::Debug for PfxCertificateSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PfxCertificateSource")
            .field("identity_count", &self.identities.len())
            .finish()
    }
}

struct StoredIdentity {
    identity: SigningIdentity,
    key: StoredKey,
}

enum StoredKey {
    Rsa(Box<RsaPrivateKey>),
    P256(P256SigningKey),
}

impl PfxCertificateSource {
    /// Reads and imports a password-protected `.p12` or `.pfx` file.
    ///
    /// # Errors
    ///
    /// Returns [`PfxAdapterError::File`] when the path is unavailable or
    /// [`PfxAdapterError::Pkcs12`] when the container or password is invalid.
    pub fn from_file(path: impl AsRef<Path>, password: &str) -> Result<Self, PfxAdapterError> {
        let bytes = fs::read(path).map_err(|error| PfxAdapterError::File(error.to_string()))?;
        Self::from_pkcs12(&bytes, password)
    }

    /// Imports a PKCS#12 container from bytes using its password.
    ///
    /// Entries without a supported RSA or P-256 private key, or without a
    /// signer certificate, are ignored so they cannot be selected for signing.
    ///
    /// # Errors
    ///
    /// Returns [`PfxAdapterError::Pkcs12`] when the container cannot be parsed.
    pub fn from_pkcs12(bytes: &[u8], password: &str) -> Result<Self, PfxAdapterError> {
        let store = KeyStore::from_pkcs12(bytes, password, Pkcs12ImportPolicy::Strict)
            .map_err(|error| PfxAdapterError::Pkcs12(error.to_string()))?;
        let identities = store
            .entries()
            .filter_map(|(alias, entry)| Self::stored_identity(alias, entry))
            .map(|stored| (stored.identity.id.clone(), stored))
            .collect();

        Ok(Self { identities })
    }

    fn stored_identity(alias: &str, entry: &KeyStoreEntry) -> Option<StoredIdentity> {
        let KeyStoreEntry::PrivateKeyChain(chain) = entry else {
            return None;
        };
        let certificate_chain_der: Vec<Vec<u8>> = chain
            .certs()
            .iter()
            .map(|certificate| certificate.as_der().to_vec())
            .collect();
        if certificate_chain_der.is_empty() {
            return None;
        }
        let (key, supported_algorithms) = StoredKey::from_pkcs8(chain.key().as_der())?;
        let leaf = X509Certificate::from_der(&certificate_chain_der[0]).ok()?;
        if !key.matches_leaf_public_key(&leaf) {
            return None;
        }
        Some(StoredIdentity {
            identity: SigningIdentity {
                id: identity_id(alias),
                display_name: alias.to_owned(),
                certificate_chain_der,
                supported_algorithms,
            },
            key,
        })
    }
}

impl CertificateSourcePort for PfxCertificateSource {
    fn list_identities(&self) -> Vec<SigningIdentity> {
        self.identities
            .values()
            .map(|stored| stored.identity.clone())
            .collect()
    }

    fn sign_digest_raw(
        &self,
        identity_id: &str,
        digest: &[u8],
        algorithm: SigningAlgorithm,
    ) -> Result<Vec<u8>, SignError> {
        let stored =
            self.identities
                .get(identity_id)
                .ok_or_else(|| SignError::IdentityUnavailable {
                    identity_id: identity_id.to_owned(),
                })?;
        if !stored.identity.supported_algorithms.contains(&algorithm) {
            return Err(SignError::UnsupportedAlgorithm {
                identity_id: identity_id.to_owned(),
                algorithm,
            });
        }
        stored
            .key
            .sign_digest(digest, algorithm)
            .map_err(|message| SignError::Backend { message })
    }
}

impl StoredKey {
    fn matches_leaf_public_key(&self, certificate: &X509Certificate) -> bool {
        let Ok(certificate_spki_der) = certificate.tbs_certificate.subject_public_key_info.to_der()
        else {
            return false;
        };
        let public_key_der = match self {
            Self::Rsa(key) => key.to_public_key().to_public_key_der(),
            Self::P256(key) => key.verifying_key().to_public_key_der(),
        };

        public_key_der.is_ok_and(|public_key_der| public_key_der.as_bytes() == certificate_spki_der)
    }

    fn from_pkcs8(der: &[u8]) -> Option<(Self, Vec<SigningAlgorithm>)> {
        if let Ok(key) = RsaPrivateKey::from_pkcs8_der(der) {
            return Some((Self::Rsa(Box::new(key)), rsa_algorithms()));
        }
        P256SigningKey::from_pkcs8_der(der)
            .ok()
            .map(|key| (Self::P256(key), ecdsa_algorithms()))
    }

    fn sign_digest(&self, digest: &[u8], algorithm: SigningAlgorithm) -> Result<Vec<u8>, String> {
        match (self, algorithm) {
            (Self::Rsa(key), SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256)) => {
                sign_rsa::<Sha256>(key, digest)
            }
            (Self::Rsa(key), SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha384)) => {
                sign_rsa::<Sha384>(key, digest)
            }
            (Self::Rsa(key), SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha512)) => {
                sign_rsa::<Sha512>(key, digest)
            }
            (Self::P256(key), SigningAlgorithm::Ecdsa(_)) => sign_p256(key, digest),
            _ => Err("private key does not support the requested signing algorithm".to_owned()),
        }
    }
}

fn sign_rsa<D>(key: &RsaPrivateKey, digest: &[u8]) -> Result<Vec<u8>, String>
where
    D: sha2::Digest + AssociatedOid,
{
    RsaSigningKey::<D>::new(key.clone())
        .sign_prehash(digest)
        .map(|signature| signature.to_vec())
        .map_err(|error| error.to_string())
}

fn sign_p256(key: &P256SigningKey, digest: &[u8]) -> Result<Vec<u8>, String> {
    let signature: P256Signature = key
        .sign_prehash(digest)
        .map_err(|error| error.to_string())?;
    Ok(signature.to_der().as_bytes().to_vec())
}

fn rsa_algorithms() -> Vec<SigningAlgorithm> {
    [
        DigestAlgorithm::Sha256,
        DigestAlgorithm::Sha384,
        DigestAlgorithm::Sha512,
    ]
    .into_iter()
    .map(SigningAlgorithm::RsaPkcs1v15)
    .collect()
}

fn ecdsa_algorithms() -> Vec<SigningAlgorithm> {
    [
        DigestAlgorithm::Sha256,
        DigestAlgorithm::Sha384,
        DigestAlgorithm::Sha512,
    ]
    .into_iter()
    .map(SigningAlgorithm::Ecdsa)
    .collect()
}

fn identity_id(alias: &str) -> String {
    use std::fmt::Write;

    alias.bytes().fold(String::from("pfx:"), |mut id, byte| {
        let _ = write!(id, "{byte:02x}");
        id
    })
}
