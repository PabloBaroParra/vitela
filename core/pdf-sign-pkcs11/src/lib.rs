//! Linux PKCS#11 implementation of [`pdf_sign::CertificateSourcePort`].
//! Private-key objects remain in the token; only public certificate DER and
//! token-produced signatures cross this boundary.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

/// Re-exported for adapters (such as `pdf-sign-nss`) that need to build their
/// own module-specific `C_Initialize` argument for
/// [`Pkcs11CertificateSource::load_with_init_args`] — this crate stays
/// `#![forbid(unsafe_code)]`, so constructing anything beyond the plain flags
/// this module uses in [`Pkcs11CertificateSource::load`] is the caller's
/// responsibility (and, for a module like NSS's that requires a raw
/// `pReserved` string, the caller's `unsafe`).
pub use cryptoki::context::{CInitializeArgs, CInitializeFlags};
use cryptoki::{
    context::Pkcs11,
    error::{Error as CryptokiError, RvError},
    mechanism::Mechanism,
    object::{Attribute, AttributeType, CertificateType, KeyType, ObjectClass},
    session::{Session, UserType},
    slot::{Slot, TokenInfo},
    types::AuthPin,
};
use pdf_sign::{
    der_encode_ecdsa_signature, rsa_digest_info, CertificateSourcePort, DigestAlgorithm, SignError,
    SigningAlgorithm, SigningIdentity,
};
use thiserror::Error;
use x509_cert::{der::Decode, Certificate};
/// Failure while loading or initializing a PKCS#11 module.
#[derive(Debug, Error)]
pub enum Pkcs11AdapterError {
    /// The shared library could not be loaded or initialized.
    #[error("failed to load PKCS#11 module: {0}")]
    Module(String),
}
/// Certificate source backed by a PKCS#11 module such as SoftHSM or a smart-card driver.
#[derive(Debug)]
pub struct Pkcs11CertificateSource {
    module: Pkcs11,
    module_key: PathBuf,
    user_pin: Option<AuthPin>,
}
/// C_Initialize/C_Finalize state is global per shared library in the process,
/// so instances share one [`Pkcs11`] per canonical module path and the library
/// is finalized only when the last instance using it is dropped.
struct ModuleEntry {
    module: Pkcs11,
    owns_initialization: bool,
    references: usize,
}

fn modules() -> &'static Mutex<HashMap<PathBuf, ModuleEntry>> {
    static MODULES: OnceLock<Mutex<HashMap<PathBuf, ModuleEntry>>> = OnceLock::new();
    MODULES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn acquire_module(
    path: &Path,
    init_args: CInitializeArgs,
) -> Result<(PathBuf, Pkcs11), Pkcs11AdapterError> {
    let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut modules = modules().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(entry) = modules.get_mut(&key) {
        entry.references += 1;
        return Ok((key, entry.module.clone()));
    }
    let module =
        Pkcs11::new(path).map_err(|error| Pkcs11AdapterError::Module(error.to_string()))?;
    let owns_initialization = match module.initialize(init_args) {
        Ok(()) => true,
        Err(error) if module_is_already_initialized(&error) => false,
        Err(error) => return Err(Pkcs11AdapterError::Module(error.to_string())),
    };
    modules.insert(
        key.clone(),
        ModuleEntry {
            module: module.clone(),
            owns_initialization,
            references: 1,
        },
    );
    Ok((key, module))
}

fn release_module(key: &Path) {
    let mut modules = modules().lock().unwrap_or_else(PoisonError::into_inner);
    let Some(entry) = modules.get_mut(key) else {
        return;
    };
    entry.references -= 1;
    if entry.references > 0 {
        return;
    }
    if let Some(entry) = modules.remove(key) {
        if entry.owns_initialization {
            let _ = entry.module.finalize();
        }
    }
}
impl Pkcs11CertificateSource {
    /// Loads a PKCS#11 module and initializes it for operating-system threads.
    /// The optional PIN authenticates sessions; private-key bytes are never requested.
    pub fn load(
        module_path: impl AsRef<Path>,
        user_pin: Option<String>,
    ) -> Result<Self, Pkcs11AdapterError> {
        Self::load_with_init_args(
            module_path,
            CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK),
            user_pin,
        )
    }

    /// Like [`Self::load`], but lets the caller supply the module's
    /// `C_Initialize` argument directly — for adapters (such as
    /// `pdf-sign-nss`) whose module needs configuration this crate has no
    /// reason to know about, and which can only be built through
    /// `cryptoki`'s own `unsafe` API. This crate stays
    /// `#![forbid(unsafe_code)]`: building `init_args` is entirely the
    /// caller's doing.
    pub fn load_with_init_args(
        module_path: impl AsRef<Path>,
        init_args: CInitializeArgs,
        user_pin: Option<String>,
    ) -> Result<Self, Pkcs11AdapterError> {
        let (module_key, module) = acquire_module(module_path.as_ref(), init_args)?;
        Ok(Self {
            module,
            module_key,
            user_pin: user_pin.map(|pin| AuthPin::new(pin.into_boxed_str())),
        })
    }

    /// Session for signing: authentication failures must surface to the caller.
    fn authenticated_session(&self, slot: Slot) -> Result<Session, SignError> {
        let token = self.module.get_token_info(slot).map_err(sign_error)?;
        if token.user_pin_locked() {
            return Err(SignError::Backend {
                message: "token user PIN is locked".to_owned(),
            });
        }
        let session = self.module.open_ro_session(slot).map_err(sign_error)?;
        if token.login_required() {
            match session.login(UserType::User, self.user_pin.as_ref()) {
                Ok(()) | Err(CryptokiError::Pkcs11(RvError::UserAlreadyLoggedIn, _)) => {}
                Err(error) => return Err(sign_error(error)),
            }
        }
        Ok(session)
    }

    fn identities_in_slot(&self, slot: Slot) -> Result<Vec<SigningIdentity>, SignError> {
        let token = self.module.get_token_info(slot).map_err(sign_error)?;
        let session = self.module.open_ro_session(slot).map_err(sign_error)?;
        if token.login_required() && pin_attempt_is_safe(&token) {
            // Private-key objects are only visible after login; certificates
            // are public objects, so a failed login degrades the listing to
            // public enumeration instead of erroring — and it is never
            // retried once the token reports a failed attempt, so a listing
            // can never lock a token whose PIN this source does not hold.
            let _ = session.login(UserType::User, self.user_pin.as_ref());
        }
        let serial = token.serial_number().trim().to_owned();
        let records = fetch_certificates(&session)?;

        let mut identities = Vec::new();
        for record in &records {
            if record.id.is_empty() {
                continue;
            }
            let Some(key) = find_private_key(&session, &record.id)? else {
                continue;
            };
            let supported_algorithms = supported_algorithms_for_key(&session, key)?;
            if supported_algorithms.is_empty() {
                continue;
            }
            identities.push(SigningIdentity {
                id: format!(
                    "{}:{}",
                    encode_hex(serial.as_bytes()),
                    encode_hex(&record.id)
                ),
                display_name: String::from_utf8_lossy(&record.label).into_owned(),
                certificate_chain_der: certificate_chain(&record.der, &records),
                supported_algorithms,
            });
        }
        Ok(identities)
    }

    fn slot_with_serial(&self, serial: &str) -> Result<Option<Slot>, SignError> {
        for slot in self.module.get_slots_with_token().map_err(sign_error)? {
            let token = self.module.get_token_info(slot).map_err(sign_error)?;
            if token.serial_number().trim() == serial {
                return Ok(Some(slot));
            }
        }
        Ok(None)
    }
}
impl Drop for Pkcs11CertificateSource {
    fn drop(&mut self) {
        release_module(&self.module_key);
    }
}
impl CertificateSourcePort for Pkcs11CertificateSource {
    fn list_identities(&self) -> Vec<SigningIdentity> {
        let Ok(slots) = self.module.get_slots_with_token() else {
            return Vec::new();
        };
        slots
            .into_iter()
            .flat_map(|slot| self.identities_in_slot(slot).unwrap_or_default())
            .collect()
    }

    fn sign_digest_raw(
        &self,
        identity_id: &str,
        digest: &[u8],
        algorithm: SigningAlgorithm,
    ) -> Result<Vec<u8>, SignError> {
        let (serial, key_id) = parse_identity_id(identity_id)?;
        let slot =
            self.slot_with_serial(&serial)?
                .ok_or_else(|| SignError::IdentityUnavailable {
                    identity_id: identity_id.to_owned(),
                })?;
        let session = self.authenticated_session(slot)?;
        let key =
            find_private_key(&session, &key_id)?.ok_or_else(|| SignError::IdentityUnavailable {
                identity_id: identity_id.to_owned(),
            })?;

        if !supported_algorithms_for_key(&session, key)?.contains(&algorithm) {
            return Err(SignError::UnsupportedAlgorithm {
                identity_id: identity_id.to_owned(),
                algorithm,
            });
        }

        let signature = match algorithm {
            SigningAlgorithm::RsaPkcs1v15(digest_algorithm) => {
                let digest_info = rsa_digest_info(digest_algorithm, digest).ok_or_else(|| {
                    SignError::UnsupportedAlgorithm {
                        identity_id: identity_id.to_owned(),
                        algorithm,
                    }
                })?;
                session
                    .sign(&Mechanism::RsaPkcs, key, &digest_info)
                    .map_err(sign_error)?
            }
            SigningAlgorithm::Ecdsa(_) => {
                let raw = session
                    .sign(&Mechanism::Ecdsa, key, digest)
                    .map_err(sign_error)?;
                der_encode_ecdsa_signature(&raw).ok_or_else(|| SignError::Backend {
                    message: "token returned an invalid raw ECDSA signature".to_owned(),
                })?
            }
            _ => {
                return Err(SignError::UnsupportedAlgorithm {
                    identity_id: identity_id.to_owned(),
                    algorithm,
                })
            }
        };
        Ok(signature)
    }
}

/// A PIN presentation is only safe while the token reports a clean retry
/// counter: after one failure the flags stay set until a successful login,
/// so at most one unsolicited attempt ever reaches a token — its PIN can
/// never be locked by this source.
fn pin_attempt_is_safe(token: &TokenInfo) -> bool {
    !(token.user_pin_locked() || token.user_pin_final_try() || token.user_pin_count_low())
}

/// One X.509 certificate object read from a token, keyed by `CKA_ID`.
/// Records without an id cannot become identities but still belong to the
/// chain pool: CA certificates routinely carry no `CKA_ID`.
struct CertificateRecord {
    id: Vec<u8>,
    label: Vec<u8>,
    der: Vec<u8>,
}

fn fetch_certificates(session: &Session) -> Result<Vec<CertificateRecord>, SignError> {
    let handles = session
        .find_objects(&[
            Attribute::Class(ObjectClass::CERTIFICATE),
            Attribute::CertificateType(CertificateType::X_509),
        ])
        .map_err(sign_error)?;
    let mut records = Vec::with_capacity(handles.len());
    for handle in handles {
        let attributes = session
            .get_attributes(
                handle,
                &[
                    AttributeType::Id,
                    AttributeType::Label,
                    AttributeType::Value,
                ],
            )
            .map_err(sign_error)?;
        let mut record = CertificateRecord {
            id: Vec::new(),
            label: Vec::new(),
            der: Vec::new(),
        };
        for attribute in attributes {
            match attribute {
                Attribute::Id(value) => record.id = value,
                Attribute::Label(value) => record.label = value,
                Attribute::Value(value) => record.der = value,
                _ => {}
            }
        }
        if !record.der.is_empty() {
            records.push(record);
        }
    }
    Ok(records)
}

/// Orders the available token certificates leaf to root by following issuer
/// names, as [`SigningIdentity::certificate_chain_der`] requires. The walk
/// stops at a self-signed certificate, a missing issuer, or a cycle.
fn certificate_chain(leaf_der: &[u8], pool: &[CertificateRecord]) -> Vec<Vec<u8>> {
    let mut chain = vec![leaf_der.to_vec()];
    let Ok(mut current) = Certificate::from_der(leaf_der) else {
        return chain;
    };
    let parsed: Vec<(&CertificateRecord, Certificate)> = pool
        .iter()
        .filter_map(|record| {
            Certificate::from_der(&record.der)
                .ok()
                .map(|certificate| (record, certificate))
        })
        .collect();
    while current.tbs_certificate.issuer != current.tbs_certificate.subject {
        let Some((record, certificate)) = parsed.iter().find(|(record, candidate)| {
            candidate.tbs_certificate.subject == current.tbs_certificate.issuer
                && !chain.contains(&record.der)
        }) else {
            break;
        };
        chain.push(record.der.clone());
        current = certificate.clone();
    }
    chain
}

fn find_private_key(
    session: &Session,
    id: &[u8],
) -> Result<Option<cryptoki::object::ObjectHandle>, SignError> {
    session
        .find_objects(&[
            Attribute::Class(ObjectClass::PRIVATE_KEY),
            Attribute::Id(id.to_vec()),
        ])
        .map_err(sign_error)
        .map(unique_object)
}

fn unique_object<T>(objects: Vec<T>) -> Option<T> {
    let mut objects = objects.into_iter();
    let object = objects.next()?;
    objects.next().is_none().then_some(object)
}

fn module_is_already_initialized(error: &CryptokiError) -> bool {
    matches!(
        error,
        CryptokiError::Pkcs11(RvError::CryptokiAlreadyInitialized, _)
    )
}

fn supported_algorithms_for_key(
    session: &Session,
    key: cryptoki::object::ObjectHandle,
) -> Result<Vec<SigningAlgorithm>, SignError> {
    let attributes = session
        .get_attributes(key, &[AttributeType::KeyType])
        .map_err(sign_error)?;
    Ok(match attributes.into_iter().find_map(key_type) {
        Some(KeyType::RSA) => algorithms(SigningAlgorithm::RsaPkcs1v15),
        Some(KeyType::EC) => algorithms(SigningAlgorithm::Ecdsa),
        _ => Vec::new(),
    })
}

fn key_type(attribute: Attribute) -> Option<KeyType> {
    match attribute {
        Attribute::KeyType(key_type) if key_type == KeyType::RSA || key_type == KeyType::EC => {
            Some(key_type)
        }
        _ => None,
    }
}

fn sign_error(error: CryptokiError) -> SignError {
    match error {
        CryptokiError::Pkcs11(RvError::FunctionCanceled, _) => SignError::UserCancelled,
        error => SignError::Backend {
            message: error.to_string(),
        },
    }
}
fn algorithms(scheme: fn(DigestAlgorithm) -> SigningAlgorithm) -> Vec<SigningAlgorithm> {
    [
        DigestAlgorithm::Sha256,
        DigestAlgorithm::Sha384,
        DigestAlgorithm::Sha512,
    ]
    .into_iter()
    .map(scheme)
    .collect()
}
/// Identity ids are `hex(token serial):hex(CKA_ID)` — anchored to the token
/// serial number, not the slot, because slot ids change across re-insertion.
fn parse_identity_id(identity_id: &str) -> Result<(String, Vec<u8>), SignError> {
    let unavailable = || SignError::IdentityUnavailable {
        identity_id: identity_id.to_owned(),
    };
    let (serial_hex, key_hex) = identity_id.split_once(':').ok_or_else(unavailable)?;
    let serial = decode_hex(serial_hex)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or_else(unavailable)?;
    let key_id = decode_hex(key_hex)
        .filter(|key_id| !key_id.is_empty())
        .ok_or_else(unavailable)?;
    Ok((serial, key_id))
}
fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}
fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            std::str::from_utf8(chunk)
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use x509_cert::{
        der::{
            asn1::{BitString, UtcTime},
            Any, Encode,
        },
        name::Name,
        serial_number::SerialNumber,
        spki::{AlgorithmIdentifierOwned, ObjectIdentifier, SubjectPublicKeyInfoOwned},
        time::{Time, Validity},
        Certificate, TbsCertificate, Version,
    };

    #[test]
    fn unique_object_rejects_empty_and_ambiguous_associations() {
        assert!(unique_object(Vec::<u8>::new()).is_none());
        assert!(unique_object(vec![1, 2]).is_none());
    }

    #[test]
    fn parse_identity_id_rejects_an_empty_key_id() {
        assert!(parse_identity_id(&format!("{}:", encode_hex(b"SER1"))).is_err());
    }

    #[test]
    fn parse_identity_id_round_trips_serial_and_key_id() {
        let identity_id = format!("{}:{}", encode_hex(b"CARD 001"), encode_hex(&[0x0f, 0xa0]));

        let (serial, key_id) =
            parse_identity_id(&identity_id).expect("a well-formed identity id must parse");

        assert_eq!(serial, "CARD 001");
        assert_eq!(key_id, vec![0x0f, 0xa0]);
    }

    #[test]
    fn parse_identity_id_rejects_non_canonical_hex() {
        assert!(parse_identity_id(&format!("{}:+f", encode_hex(b"SER1"))).is_err());
        assert!(decode_hex("+f").is_none());
        assert_eq!(decode_hex("0f"), Some(vec![0x0f]));
    }

    #[test]
    fn initialized_module_is_not_owned_by_the_adapter() {
        let error = CryptokiError::Pkcs11(
            RvError::CryptokiAlreadyInitialized,
            cryptoki::context::Function::Initialize,
        );

        assert!(module_is_already_initialized(&error));
    }

    #[test]
    fn cancelled_pin_entry_maps_to_user_cancelled() {
        let error =
            CryptokiError::Pkcs11(RvError::FunctionCanceled, cryptoki::context::Function::Sign);

        assert_eq!(sign_error(error), SignError::UserCancelled);
    }

    #[test]
    fn ecdsa_signature_encodes_minimal_padded_integers() {
        let encoded = der_encode_ecdsa_signature(&[0x00, 0x80, 0x00, 0x01])
            .expect("a two-byte-per-half signature must encode");

        assert_eq!(
            encoded,
            vec![0x30, 0x07, 0x02, 0x02, 0x00, 0x80, 0x02, 0x01, 0x01]
        );
    }

    #[test]
    fn ecdsa_signature_encodes_p521_sized_signatures_with_long_form_length() {
        let raw = vec![0x80; 132];

        let encoded = der_encode_ecdsa_signature(&raw)
            .expect("a P-521 sized raw signature must encode with a long-form length");

        assert_eq!(&encoded[..2], &[0x30, 0x81]);
        let body_length = usize::from(encoded[2]);
        assert_eq!(encoded.len(), 3 + body_length);
        // Each half is a 66-byte magnitude with the high bit set: 0x00 pad + value.
        assert_eq!(&encoded[3..6], &[0x02, 0x43, 0x00]);
    }

    #[test]
    fn certificate_chain_orders_leaf_to_root_and_skips_unrelated_certificates() {
        let leaf = test_certificate("CN=Leaf", "CN=Intermediate");
        let intermediate = test_certificate("CN=Intermediate", "CN=Root");
        let root = test_certificate("CN=Root", "CN=Root");
        let unrelated = test_certificate("CN=Other", "CN=Other");
        let pool: Vec<CertificateRecord> = [&root, &unrelated, &intermediate, &leaf]
            .into_iter()
            .map(|der| CertificateRecord {
                id: Vec::new(),
                label: Vec::new(),
                der: der.clone(),
            })
            .collect();

        let chain = certificate_chain(&leaf, &pool);

        assert_eq!(chain, vec![leaf, intermediate, root]);
    }

    #[test]
    fn certificate_chain_stops_when_the_issuer_is_missing() {
        let leaf = test_certificate("CN=Leaf", "CN=Absent");

        assert_eq!(certificate_chain(&leaf, &[]), vec![leaf]);
    }

    fn test_certificate(subject: &str, issuer: &str) -> Vec<u8> {
        let algorithm = AlgorithmIdentifierOwned {
            oid: ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11"),
            parameters: Some(Any::null()),
        };
        let not_before = UtcTime::from_date_time(
            x509_cert::der::DateTime::new(2025, 1, 1, 0, 0, 0)
                .expect("test start time must be valid"),
        )
        .expect("test start time must convert");
        let not_after = UtcTime::from_date_time(
            x509_cert::der::DateTime::new(2035, 1, 1, 0, 0, 0)
                .expect("test end time must be valid"),
        )
        .expect("test end time must convert");
        let certificate = Certificate {
            tbs_certificate: TbsCertificate {
                version: Version::V3,
                serial_number: SerialNumber::new(&[1]).expect("test serial must be valid"),
                signature: algorithm.clone(),
                issuer: Name::from_str(issuer).expect("test issuer must parse"),
                validity: Validity {
                    not_before: Time::UtcTime(not_before),
                    not_after: Time::UtcTime(not_after),
                },
                subject: Name::from_str(subject).expect("test subject must parse"),
                subject_public_key_info: SubjectPublicKeyInfoOwned {
                    algorithm: AlgorithmIdentifierOwned {
                        oid: ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1"),
                        parameters: Some(Any::null()),
                    },
                    subject_public_key: BitString::from_bytes(&[1, 2, 3])
                        .expect("test public key must encode"),
                },
                issuer_unique_id: None,
                subject_unique_id: None,
                extensions: None,
            },
            signature_algorithm: algorithm,
            signature: BitString::from_bytes(&[4, 5, 6])
                .expect("test certificate signature must encode"),
        };

        certificate
            .to_der()
            .expect("test certificate must encode to DER")
    }
}
