//! PKCS#12 (`.p12`/`.pfx`) implementation of [`pdf_sign::CertificateSourcePort`].
//! Private keys are imported only to sign within this adapter and are never
//! exposed through the certificate-source boundary.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use p12_keystore::{error::Error as KeyStoreError, KeyStore, KeyStoreEntry, Pkcs12ImportPolicy};
use p256::ecdsa::{Signature as P256Signature, SigningKey as P256SigningKey};
use pdf_sign::{
    CertificateSourcePort, DigestAlgorithm, SignError, SigningAlgorithm, SigningIdentity,
};
use rsa::pkcs1v15::SigningKey as RsaSigningKey;
use rsa::{
    pkcs8::{
        der::{asn1::ObjectIdentifier, Tag, Tagged as _},
        AssociatedOid, DecodePrivateKey as _, EncodePublicKey as _, PrivateKeyInfo,
    },
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
    /// The container opened, but nothing in it can sign. The payload is a
    /// lower-case clause for the person who chose the file — what is wrong
    /// with it and, where there is one, how to export it again — so a shell
    /// shows it rather than asking for the password again.
    #[error("{0}")]
    Unusable(String),
}

/// Why an encrypted part of the container could not be read: a scheme other
/// than RC2-40, 3DES, or PBES2 with AES or 3DES (RC4, RC2-128, Camellia, …).
const UNSUPPORTED_ENCRYPTION: &str = "this certificate file uses an encryption scheme that is \
    not supported; export it again with AES-256 or 3DES encryption";
/// Why a key with a certificate cannot sign: anything but RSA or P-256.
const UNSUPPORTED_KEY: &str = "this certificate file's private key is not an RSA or P-256 \
    key, the only kinds that can sign";
/// Why an elliptic-curve key cannot sign when its curve is spelled out rather
/// than named — RFC 5480 rules that encoding out for certificates, but older
/// OpenSSL and LibreSSL still write it unless asked for a named curve.
const EXPLICIT_CURVE: &str = "this certificate file's elliptic-curve key lists its curve \
    parameters instead of naming the curve; export it again with a named P-256 curve";
/// Why a key cannot sign with the certificate stored beside it.
const KEY_CERTIFICATE_MISMATCH: &str =
    "this certificate file's private key does not match its certificate";
/// Why a key cannot sign when the certificate stored beside it is not X.509.
const CERTIFICATE_UNREADABLE: &str = "the certificate in this file could not be read";

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
    /// A file that holds only certificates opens with no identities.
    ///
    /// # Errors
    ///
    /// Returns [`PfxAdapterError::Pkcs12`] when the container cannot be
    /// parsed, which includes a wrong password, and
    /// [`PfxAdapterError::Unusable`] when it is encrypted with a scheme this
    /// adapter cannot read or when it holds private keys and none can sign.
    pub fn from_pkcs12(bytes: &[u8], password: &str) -> Result<Self, PfxAdapterError> {
        let store = KeyStore::from_pkcs12(bytes, password, Pkcs12ImportPolicy::Strict).map_err(
            |error| match error {
                KeyStoreError::UnsupportedEncryptionScheme => {
                    PfxAdapterError::Unusable(UNSUPPORTED_ENCRYPTION.to_owned())
                }
                error => PfxAdapterError::Pkcs12(error.to_string()),
            },
        )?;
        let mut identities = BTreeMap::new();
        let mut first_rejection = None;
        for (alias, entry) in store.entries() {
            match Self::stored_identity(alias, entry) {
                Ok(Some(stored)) => {
                    identities.insert(stored.identity.id.clone(), stored);
                }
                Ok(None) => {}
                Err(reason) => {
                    first_rejection.get_or_insert(reason);
                }
            }
        }
        match first_rejection {
            Some(reason) if identities.is_empty() => {
                Err(PfxAdapterError::Unusable(reason.to_owned()))
            }
            _ => Ok(Self { identities }),
        }
    }

    /// The identity `entry` signs as, `None` when it is not a private key
    /// with a certificate, or why that key cannot sign.
    fn stored_identity(
        alias: &str,
        entry: &KeyStoreEntry,
    ) -> Result<Option<StoredIdentity>, &'static str> {
        let KeyStoreEntry::PrivateKeyChain(chain) = entry else {
            return Ok(None);
        };
        let certificate_chain_der: Vec<Vec<u8>> = chain
            .certs()
            .iter()
            .map(|certificate| certificate.as_der().to_vec())
            .collect();
        if certificate_chain_der.is_empty() {
            return Ok(None);
        }
        let (key, supported_algorithms) = StoredKey::from_pkcs8(chain.key().as_der())?;
        let leaf = X509Certificate::from_der(&certificate_chain_der[0])
            .map_err(|_| CERTIFICATE_UNREADABLE)?;
        if !key.matches_leaf_public_key(&leaf) {
            return Err(KEY_CERTIFICATE_MISMATCH);
        }
        Ok(Some(StoredIdentity {
            identity: SigningIdentity {
                id: identity_id(alias),
                display_name: alias.to_owned(),
                certificate_chain_der,
                supported_algorithms,
            },
            key,
        }))
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

    fn from_pkcs8(der: &[u8]) -> Result<(Self, Vec<SigningAlgorithm>), &'static str> {
        if let Ok(key) = RsaPrivateKey::from_pkcs8_der(der) {
            return Ok((Self::Rsa(Box::new(key)), rsa_algorithms()));
        }
        P256SigningKey::from_pkcs8_der(der)
            .map(|key| (Self::P256(key), ecdsa_algorithms()))
            .map_err(|_| unsupported_key_reason(der))
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

/// Why the PKCS#8 key in `der` is neither an RSA nor a P-256 key this adapter
/// reads: an elliptic-curve key whose parameters are a `SEQUENCE` spells its
/// curve out rather than naming it with an OID.
fn unsupported_key_reason(der: &[u8]) -> &'static str {
    const EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");

    let explicit_curve = PrivateKeyInfo::try_from(der).is_ok_and(|info| {
        info.algorithm.oid == EC_PUBLIC_KEY
            && info
                .algorithm
                .parameters
                .is_some_and(|parameters| parameters.tag() == Tag::Sequence)
    });
    if explicit_curve {
        EXPLICIT_CURVE
    } else {
        UNSUPPORTED_KEY
    }
}

fn identity_id(alias: &str) -> String {
    use std::fmt::Write;

    alias.bytes().fold(String::from("pfx:"), |mut id, byte| {
        let _ = write!(id, "{byte:02x}");
        id
    })
}
