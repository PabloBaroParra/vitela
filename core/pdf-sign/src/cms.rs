//! Detached CMS `SignedData` construction for PDF signatures.

use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, DigestAlgorithmIdentifiers, EncapsulatedContentInfo, SignedAttributes,
    SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};
use der::asn1::{Any, ObjectIdentifier, OctetString, SetOfVec, UintRef};
use der::{Decode, Encode};
use sha2::{Digest, Sha256, Sha384, Sha512};
use spki::AlgorithmIdentifierOwned;
use x509_cert::attr::{Attribute, AttributeValue};
use x509_cert::Certificate;

use crate::{
    CertificateSourcePort, DigestAlgorithm, DocumentDigest, SignError, SigningAlgorithm,
    SigningIdentity,
};

const ID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
const ID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const ID_CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const ID_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const ID_SHA_256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const ID_SHA_384: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.2");
const ID_SHA_512: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.3");
const SHA_256_WITH_RSA_ENCRYPTION: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const SHA_384_WITH_RSA_ENCRYPTION: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.12");
const SHA_512_WITH_RSA_ENCRYPTION: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.13");
const ECDSA_WITH_SHA_256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const ECDSA_WITH_SHA_384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");
const ECDSA_WITH_SHA_512: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.4");
const RSA_ENCRYPTION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const ID_EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");

/// Builds a detached CMS `SignedData` value for a PDF byte-range digest.
///
/// The builder embeds the selected identity's complete certificate chain,
/// writes the document digest into the CMS `message-digest` signed attribute,
/// and asks the certificate source to sign the digest of the DER-encoded
/// signed attributes. No PDF bytes or private-key material enter the CMS.
#[derive(Clone, Copy)]
pub struct CmsSignedDataBuilder<'a> {
    source: &'a dyn CertificateSourcePort,
    identity: &'a SigningIdentity,
    document_digest: &'a DocumentDigest,
    algorithm: SigningAlgorithm,
}

impl<'a> CmsSignedDataBuilder<'a> {
    /// Creates a detached CMS builder.
    #[must_use]
    pub const fn new(
        source: &'a dyn CertificateSourcePort,
        identity: &'a SigningIdentity,
        document_digest: &'a DocumentDigest,
        algorithm: SigningAlgorithm,
    ) -> Self {
        Self {
            source,
            identity,
            document_digest,
            algorithm,
        }
    }

    /// Builds and DER-encodes a CMS `ContentInfo` containing `SignedData`.
    ///
    /// The encapsulated content is omitted, as required for a detached PDF
    /// signature. The signer is identified by the leaf certificate's issuer
    /// and serial number.
    ///
    /// # Errors
    ///
    /// Returns [`SignError`] when the digest and signature algorithms do not
    /// match, the identity did not advertise the requested algorithm, its
    /// certificate chain is absent or malformed, CMS encoding fails, or the
    /// certificate source cannot produce a signature.
    pub fn build(self) -> Result<Vec<u8>, SignError> {
        self.validate_algorithm()?;
        let certificates = self.parse_certificates()?;
        let leaf = certificates
            .first()
            .ok_or_else(|| SignError::MissingCertificateChain {
                identity_id: self.identity.id.clone(),
            })?;
        self.validate_leaf_public_key_algorithm(leaf)?;
        let digest_algorithm = digest_algorithm_identifier(self.document_digest.algorithm());
        let signed_attributes = signed_attributes(self.document_digest)?;
        let signed_attributes_der = encode_der(&signed_attributes)?;
        let signed_attributes_digest =
            digest_bytes(&signed_attributes_der, self.document_digest.algorithm())?;
        let signature = self.source.sign_digest(
            &self.identity.id,
            &signed_attributes_digest,
            self.algorithm,
        )?;
        if signature.is_empty() {
            return Err(SignError::EmptySignature);
        }
        if matches!(self.algorithm, SigningAlgorithm::Ecdsa(_)) {
            validate_ecdsa_signature(&signature)?;
        }

        let signer_info = SignerInfo {
            version: CmsVersion::V1,
            sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
                issuer: leaf.tbs_certificate.issuer.clone(),
                serial_number: leaf.tbs_certificate.serial_number.clone(),
            }),
            digest_alg: digest_algorithm.clone(),
            signed_attrs: Some(signed_attributes),
            signature_algorithm: signature_algorithm_identifier(self.algorithm),
            signature: OctetString::new(signature).map_err(cms_encoding)?,
            unsigned_attrs: None,
        };
        let signed_data = SignedData {
            version: CmsVersion::V1,
            digest_algorithms: DigestAlgorithmIdentifiers::try_from(vec![digest_algorithm])
                .map_err(cms_encoding)?,
            encap_content_info: EncapsulatedContentInfo {
                econtent_type: ID_DATA,
                econtent: None,
            },
            certificates: Some(certificate_set(certificates)?),
            crls: None,
            signer_infos: SignerInfos::try_from(vec![signer_info]).map_err(cms_encoding)?,
        };
        let signed_data_der = encode_der(&signed_data)?;
        let content_info = ContentInfo {
            content_type: ID_SIGNED_DATA,
            content: Any::from_der(&signed_data_der).map_err(cms_encoding)?,
        };

        encode_der(&content_info)
    }

    fn validate_algorithm(self) -> Result<(), SignError> {
        let digest = self.document_digest.algorithm();
        if self.algorithm.digest_algorithm() != digest {
            return Err(SignError::DigestAlgorithmMismatch {
                digest,
                signing: self.algorithm,
            });
        }
        if !self.identity.supported_algorithms.contains(&self.algorithm) {
            return Err(SignError::UnsupportedAlgorithm {
                identity_id: self.identity.id.clone(),
                algorithm: self.algorithm,
            });
        }
        Ok(())
    }

    fn parse_certificates(self) -> Result<Vec<Certificate>, SignError> {
        self.identity
            .certificate_chain_der
            .iter()
            .enumerate()
            .map(|(index, certificate)| {
                Certificate::from_der(certificate).map_err(|error| SignError::InvalidCertificate {
                    index,
                    message: error.to_string(),
                })
            })
            .collect()
    }

    fn validate_leaf_public_key_algorithm(self, leaf: &Certificate) -> Result<(), SignError> {
        let public_key_algorithm = leaf.tbs_certificate.subject_public_key_info.algorithm.oid;
        let compatible = match self.algorithm {
            SigningAlgorithm::RsaPkcs1v15(_) => public_key_algorithm == RSA_ENCRYPTION,
            SigningAlgorithm::Ecdsa(_) => public_key_algorithm == ID_EC_PUBLIC_KEY,
        };

        if compatible {
            Ok(())
        } else {
            Err(SignError::IncompatibleCertificateAlgorithm {
                identity_id: self.identity.id.clone(),
                public_key_algorithm: public_key_algorithm.to_string(),
                signing: self.algorithm,
            })
        }
    }
}

fn validate_ecdsa_signature(signature: &[u8]) -> Result<(), SignError> {
    let components =
        <[UintRef<'_>; 2]>::from_der(signature).map_err(|_| SignError::InvalidEcdsaSignature)?;

    if components
        .iter()
        .any(|component| component.as_bytes().is_empty() || component.as_bytes() == [0])
    {
        return Err(SignError::InvalidEcdsaSignature);
    }

    Ok(())
}

fn signed_attributes(document_digest: &DocumentDigest) -> Result<SignedAttributes, SignError> {
    let content_type = Attribute {
        oid: ID_CONTENT_TYPE,
        values: SetOfVec::try_from(vec![Any::encode_from(&ID_DATA).map_err(cms_encoding)?])
            .map_err(cms_encoding)?,
    };
    let digest_value = OctetString::new(document_digest.as_bytes()).map_err(cms_encoding)?;
    let message_digest = Attribute {
        oid: ID_MESSAGE_DIGEST,
        values: SetOfVec::<AttributeValue>::try_from(vec![
            Any::encode_from(&digest_value).map_err(cms_encoding)?
        ])
        .map_err(cms_encoding)?,
    };

    SignedAttributes::try_from(vec![content_type, message_digest]).map_err(cms_encoding)
}

fn certificate_set(certificates: Vec<Certificate>) -> Result<CertificateSet, SignError> {
    CertificateSet::try_from(
        certificates
            .into_iter()
            .map(CertificateChoices::Certificate)
            .collect::<Vec<_>>(),
    )
    .map_err(cms_encoding)
}

fn digest_algorithm_identifier(algorithm: DigestAlgorithm) -> AlgorithmIdentifierOwned {
    let oid = match algorithm {
        DigestAlgorithm::Sha256 => ID_SHA_256,
        DigestAlgorithm::Sha384 => ID_SHA_384,
        DigestAlgorithm::Sha512 => ID_SHA_512,
    };
    AlgorithmIdentifierOwned {
        oid,
        parameters: None,
    }
}

fn signature_algorithm_identifier(algorithm: SigningAlgorithm) -> AlgorithmIdentifierOwned {
    let (oid, parameters) = match algorithm {
        SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256) => {
            (SHA_256_WITH_RSA_ENCRYPTION, Some(Any::null()))
        }
        SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha384) => {
            (SHA_384_WITH_RSA_ENCRYPTION, Some(Any::null()))
        }
        SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha512) => {
            (SHA_512_WITH_RSA_ENCRYPTION, Some(Any::null()))
        }
        SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256) => (ECDSA_WITH_SHA_256, None),
        SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha384) => (ECDSA_WITH_SHA_384, None),
        SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha512) => (ECDSA_WITH_SHA_512, None),
    };
    AlgorithmIdentifierOwned { oid, parameters }
}

fn digest_bytes(bytes: &[u8], algorithm: DigestAlgorithm) -> Result<DocumentDigest, SignError> {
    let digest = match algorithm {
        DigestAlgorithm::Sha256 => Sha256::digest(bytes).to_vec(),
        DigestAlgorithm::Sha384 => Sha384::digest(bytes).to_vec(),
        DigestAlgorithm::Sha512 => Sha512::digest(bytes).to_vec(),
    };
    DocumentDigest::new(algorithm, digest)
}

fn encode_der<T: Encode>(value: &T) -> Result<Vec<u8>, SignError> {
    value.to_der().map_err(cms_encoding)
}

fn cms_encoding(error: der::Error) -> SignError {
    SignError::CmsEncoding {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Mutex;

    use der::asn1::{BitString, UtcTime};
    use der::DateTime;
    use spki::SubjectPublicKeyInfoOwned;
    use x509_cert::certificate::{TbsCertificate, Version};
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::time::{Time, Validity};

    use super::*;

    const TEST_RSA_SIGNATURE: &[u8] = b"platform-signature";
    const TEST_ECDSA_SIGNATURE: &[u8] = &[0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01];

    #[derive(Default)]
    struct RecordingCertificateSource {
        calls: Mutex<Vec<(Vec<u8>, SigningAlgorithm)>>,
        signature: Option<Vec<u8>>,
    }

    impl CertificateSourcePort for RecordingCertificateSource {
        fn list_identities(&self) -> Vec<SigningIdentity> {
            Vec::new()
        }

        fn sign_digest_raw(
            &self,
            _identity_id: &str,
            digest: &[u8],
            algorithm: SigningAlgorithm,
        ) -> Result<Vec<u8>, SignError> {
            self.calls
                .lock()
                .expect("recording lock should be available")
                .push((digest.to_vec(), algorithm));
            Ok(self.signature.clone().unwrap_or_else(|| match algorithm {
                SigningAlgorithm::RsaPkcs1v15(_) => TEST_RSA_SIGNATURE.to_vec(),
                SigningAlgorithm::Ecdsa(_) => TEST_ECDSA_SIGNATURE.to_vec(),
            }))
        }
    }

    fn identity(algorithm: SigningAlgorithm) -> SigningIdentity {
        SigningIdentity {
            id: "test-key".to_owned(),
            display_name: "Test signer".to_owned(),
            certificate_chain_der: vec![
                test_certificate(1, algorithm),
                test_certificate(2, algorithm),
            ],
            supported_algorithms: vec![algorithm],
        }
    }

    fn test_certificate(serial: u8, signing_algorithm: SigningAlgorithm) -> Vec<u8> {
        let name = Name::from_str("CN=Test signer").expect("test name should parse");
        let certificate_algorithm = AlgorithmIdentifierOwned {
            oid: SHA_256_WITH_RSA_ENCRYPTION,
            parameters: Some(Any::null()),
        };
        let public_key_algorithm = match signing_algorithm {
            SigningAlgorithm::RsaPkcs1v15(_) => AlgorithmIdentifierOwned {
                oid: RSA_ENCRYPTION,
                parameters: Some(Any::null()),
            },
            SigningAlgorithm::Ecdsa(_) => AlgorithmIdentifierOwned {
                oid: ID_EC_PUBLIC_KEY,
                parameters: Some(
                    Any::encode_from(&ObjectIdentifier::new_unwrap("1.2.840.10045.3.1.7"))
                        .expect("test EC curve identifier should encode"),
                ),
            },
        };
        let not_before = DateTime::new(2025, 1, 1, 0, 0, 0)
            .and_then(UtcTime::from_date_time)
            .expect("test start time should be valid");
        let not_after = DateTime::new(2035, 1, 1, 0, 0, 0)
            .and_then(UtcTime::from_date_time)
            .expect("test end time should be valid");
        let certificate = Certificate {
            tbs_certificate: TbsCertificate {
                version: Version::V3,
                serial_number: SerialNumber::new(&[serial])
                    .expect("test serial number should be valid"),
                signature: certificate_algorithm.clone(),
                issuer: name.clone(),
                validity: Validity {
                    not_before: Time::UtcTime(not_before),
                    not_after: Time::UtcTime(not_after),
                },
                subject: name,
                subject_public_key_info: SubjectPublicKeyInfoOwned {
                    algorithm: public_key_algorithm,
                    subject_public_key: BitString::from_bytes(&[1, 2, 3])
                        .expect("test public key should encode"),
                },
                issuer_unique_id: None,
                subject_unique_id: None,
                extensions: None,
            },
            signature_algorithm: certificate_algorithm,
            signature: BitString::from_bytes(&[4, 5, 6])
                .expect("test certificate signature should encode"),
        };

        certificate
            .to_der()
            .expect("test certificate should encode")
    }

    fn document_digest(algorithm: DigestAlgorithm) -> DocumentDigest {
        DocumentDigest::new(algorithm, vec![0xA5; algorithm.digest_len()])
            .expect("test digest should wrap")
    }

    fn decode_signed_data(cms_der: &[u8]) -> SignedData {
        let content_info =
            ContentInfo::from_der(cms_der).expect("CMS ContentInfo should decode from DER");
        assert_eq!(content_info.content_type, ID_SIGNED_DATA);
        content_info
            .content
            .decode_as::<SignedData>()
            .expect("SignedData content should decode")
    }

    fn signed_attribute(signer: &SignerInfo, oid: ObjectIdentifier) -> &Attribute {
        signer
            .signed_attrs
            .as_ref()
            .expect("signed attributes should be present")
            .iter()
            .find(|attribute| attribute.oid == oid)
            .expect("required signed attribute should be present")
    }

    #[test]
    fn builder_creates_detached_signed_data_with_digest_and_certificate_chain() {
        let source = RecordingCertificateSource::default();
        let algorithm = SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256);
        let identity = identity(algorithm);
        let document_digest = document_digest(DigestAlgorithm::Sha256);

        let cms_der = CmsSignedDataBuilder::new(&source, &identity, &document_digest, algorithm)
            .build()
            .expect("valid inputs should build CMS SignedData");
        let signed_data = decode_signed_data(&cms_der);
        let signer = signed_data
            .signer_infos
            .0
            .iter()
            .next()
            .expect("one signer should be present");
        let message_digest = signed_attribute(signer, ID_MESSAGE_DIGEST)
            .values
            .iter()
            .next()
            .expect("message-digest should have one value")
            .decode_as::<OctetString>()
            .expect("message-digest should be an octet string");
        let content_type = signed_attribute(signer, ID_CONTENT_TYPE)
            .values
            .iter()
            .next()
            .expect("content-type should have one value")
            .decode_as::<ObjectIdentifier>()
            .expect("content-type should be an object identifier");

        assert_eq!(
            (
                signed_data.encap_content_info.econtent_type,
                signed_data.encap_content_info.econtent,
                signed_data
                    .certificates
                    .as_ref()
                    .expect("certificate chain should be present")
                    .0
                    .len(),
                signer.signature.as_bytes(),
                message_digest.as_bytes(),
                content_type,
            ),
            (
                ID_DATA,
                None,
                identity.certificate_chain_der.len(),
                TEST_RSA_SIGNATURE,
                document_digest.as_bytes(),
                ID_DATA,
            )
        );
    }

    #[test]
    fn builder_signs_sha256_digest_of_der_encoded_signed_attributes() {
        let source = RecordingCertificateSource::default();
        let algorithm = SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256);
        let identity = identity(algorithm);
        let document_digest = document_digest(DigestAlgorithm::Sha256);

        let cms_der = CmsSignedDataBuilder::new(&source, &identity, &document_digest, algorithm)
            .build()
            .expect("valid inputs should build CMS SignedData");
        let signed_data = decode_signed_data(&cms_der);
        let signed_attributes = signed_data
            .signer_infos
            .0
            .iter()
            .next()
            .and_then(|signer| signer.signed_attrs.as_ref())
            .expect("signed attributes should be present");
        let expected_digest = Sha256::digest(
            signed_attributes
                .to_der()
                .expect("signed attributes should encode"),
        );
        let calls = source
            .calls
            .lock()
            .expect("recording lock should be available");

        assert_eq!(calls.as_slice(), &[(expected_digest.to_vec(), algorithm)]);
    }

    #[test]
    fn builder_uses_leaf_issuer_and_serial_as_signer_identifier() {
        let source = RecordingCertificateSource::default();
        let algorithm = SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha384);
        let identity = identity(algorithm);
        let document_digest = document_digest(DigestAlgorithm::Sha384);
        let leaf = Certificate::from_der(&identity.certificate_chain_der[0])
            .expect("test leaf certificate should decode");

        let cms_der = CmsSignedDataBuilder::new(&source, &identity, &document_digest, algorithm)
            .build()
            .expect("valid inputs should build CMS SignedData");
        let signed_data = decode_signed_data(&cms_der);
        let signer = signed_data
            .signer_infos
            .0
            .iter()
            .next()
            .expect("one signer should be present");

        assert_eq!(
            signer.sid,
            SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
                issuer: leaf.tbs_certificate.issuer,
                serial_number: leaf.tbs_certificate.serial_number,
            })
        );
    }

    #[test]
    fn algorithm_identifiers_follow_cms_sha2_conventions() {
        let digest_oids = [
            DigestAlgorithm::Sha256,
            DigestAlgorithm::Sha384,
            DigestAlgorithm::Sha512,
        ]
        .map(|algorithm| digest_algorithm_identifier(algorithm).oid);
        let rsa = [
            DigestAlgorithm::Sha256,
            DigestAlgorithm::Sha384,
            DigestAlgorithm::Sha512,
        ]
        .map(|digest| signature_algorithm_identifier(SigningAlgorithm::RsaPkcs1v15(digest)));
        let ecdsa = [
            DigestAlgorithm::Sha256,
            DigestAlgorithm::Sha384,
            DigestAlgorithm::Sha512,
        ]
        .map(|digest| signature_algorithm_identifier(SigningAlgorithm::Ecdsa(digest)));

        assert_eq!(
            (digest_oids, rsa, ecdsa),
            (
                [ID_SHA_256, ID_SHA_384, ID_SHA_512],
                [
                    AlgorithmIdentifierOwned {
                        oid: SHA_256_WITH_RSA_ENCRYPTION,
                        parameters: Some(Any::null()),
                    },
                    AlgorithmIdentifierOwned {
                        oid: SHA_384_WITH_RSA_ENCRYPTION,
                        parameters: Some(Any::null()),
                    },
                    AlgorithmIdentifierOwned {
                        oid: SHA_512_WITH_RSA_ENCRYPTION,
                        parameters: Some(Any::null()),
                    },
                ],
                [
                    AlgorithmIdentifierOwned {
                        oid: ECDSA_WITH_SHA_256,
                        parameters: None,
                    },
                    AlgorithmIdentifierOwned {
                        oid: ECDSA_WITH_SHA_384,
                        parameters: None,
                    },
                    AlgorithmIdentifierOwned {
                        oid: ECDSA_WITH_SHA_512,
                        parameters: None,
                    },
                ],
            )
        );
    }

    #[test]
    fn builder_rejects_missing_certificate_chain_before_signing() {
        let source = RecordingCertificateSource::default();
        let algorithm = SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256);
        let mut identity = identity(algorithm);
        identity.certificate_chain_der.clear();
        let document_digest = document_digest(DigestAlgorithm::Sha256);

        let error = CmsSignedDataBuilder::new(&source, &identity, &document_digest, algorithm)
            .build()
            .expect_err("an identity without a certificate should fail");

        assert_eq!(
            error,
            SignError::MissingCertificateChain {
                identity_id: identity.id
            }
        );
    }

    #[test]
    fn builder_reports_malformed_certificate_position() {
        let source = RecordingCertificateSource::default();
        let algorithm = SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256);
        let mut identity = identity(algorithm);
        identity.certificate_chain_der[1] = vec![0x30, 0x00];
        let document_digest = document_digest(DigestAlgorithm::Sha256);

        let error = CmsSignedDataBuilder::new(&source, &identity, &document_digest, algorithm)
            .build()
            .expect_err("a malformed intermediate certificate should fail");

        assert!(matches!(
            error,
            SignError::InvalidCertificate { index: 1, .. }
        ));
    }

    #[test]
    fn builder_rejects_unadvertised_algorithm_before_signing() {
        let source = RecordingCertificateSource::default();
        let advertised = SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256);
        let requested = SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256);
        let identity = identity(advertised);
        let document_digest = document_digest(DigestAlgorithm::Sha256);

        let error = CmsSignedDataBuilder::new(&source, &identity, &document_digest, requested)
            .build()
            .expect_err("an unadvertised signature algorithm should fail");

        assert_eq!(
            error,
            SignError::UnsupportedAlgorithm {
                identity_id: identity.id,
                algorithm: requested,
            }
        );
    }

    #[test]
    fn builder_rejects_empty_platform_signature() {
        let source = RecordingCertificateSource {
            signature: Some(Vec::new()),
            ..RecordingCertificateSource::default()
        };
        let algorithm = SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256);
        let identity = identity(algorithm);
        let document_digest = document_digest(DigestAlgorithm::Sha256);

        let error = CmsSignedDataBuilder::new(&source, &identity, &document_digest, algorithm)
            .build()
            .expect_err("an empty platform signature should fail");

        assert_eq!(error, SignError::EmptySignature);
    }

    #[test]
    fn builder_rejects_non_der_ecdsa_signature() {
        let source = RecordingCertificateSource {
            signature: Some(b"raw-ecdsa-signature".to_vec()),
            ..RecordingCertificateSource::default()
        };
        let algorithm = SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256);
        let identity = identity(algorithm);
        let document_digest = document_digest(DigestAlgorithm::Sha256);

        let error = CmsSignedDataBuilder::new(&source, &identity, &document_digest, algorithm)
            .build()
            .expect_err("raw ECDSA bytes must not be embedded in CMS");

        assert_eq!(error, SignError::InvalidEcdsaSignature);
    }

    #[test]
    fn builder_accepts_der_ecdsa_signature() {
        let source = RecordingCertificateSource::default();
        let algorithm = SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256);
        let identity = identity(algorithm);
        let document_digest = document_digest(DigestAlgorithm::Sha256);

        let cms_der = CmsSignedDataBuilder::new(&source, &identity, &document_digest, algorithm)
            .build()
            .expect("DER ECDSA-Sig-Value should be accepted");
        let signed_data = decode_signed_data(&cms_der);
        let signature = &signed_data
            .signer_infos
            .0
            .iter()
            .next()
            .expect("one signer should be present")
            .signature;

        assert_eq!(signature.as_bytes(), TEST_ECDSA_SIGNATURE);
    }

    #[test]
    fn validate_ecdsa_signature_rejects_zero_components() {
        for signature in [
            &[0x30, 0x06, 0x02, 0x01, 0x00, 0x02, 0x01, 0x01][..],
            &[0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x00][..],
        ] {
            assert_eq!(
                validate_ecdsa_signature(signature),
                Err(SignError::InvalidEcdsaSignature)
            );
        }
    }

    #[test]
    fn builder_rejects_ecdsa_for_rsa_leaf_certificate() {
        let source = RecordingCertificateSource::default();
        let algorithm = SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256);
        let mut identity = identity(algorithm);
        identity.certificate_chain_der[0] =
            test_certificate(1, SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256));
        let document_digest = document_digest(DigestAlgorithm::Sha256);

        let error = CmsSignedDataBuilder::new(&source, &identity, &document_digest, algorithm)
            .build()
            .expect_err("ECDSA must not use an RSA leaf certificate");
        let calls = source
            .calls
            .lock()
            .expect("recording lock should be available")
            .len();

        assert_eq!(
            (error, calls),
            (
                SignError::IncompatibleCertificateAlgorithm {
                    identity_id: identity.id,
                    public_key_algorithm: RSA_ENCRYPTION.to_string(),
                    signing: algorithm,
                },
                0,
            )
        );
    }
}
