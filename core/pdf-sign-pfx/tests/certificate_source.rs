use der::{
    asn1::{Any, BitString, UtcTime},
    DateTime, Decode, Encode,
};
use p12_keystore::{Certificate, KeyStore, KeyStoreEntry, PrivateKey, PrivateKeyChain};
use p256::ecdsa::{
    Signature as P256Signature, SigningKey as P256SigningKey, VerifyingKey as P256VerifyingKey,
};
use pdf_sign::{CertificateSourcePort, DigestAlgorithm, DocumentDigest, SigningAlgorithm};
use pdf_sign_pfx::PfxCertificateSource;
use rsa::rand_core::OsRng;
use rsa::{
    pkcs1v15::{Signature, VerifyingKey},
    pkcs8::{EncodePrivateKey, EncodePublicKey},
    RsaPrivateKey,
};
use sha2::Sha256;
use signature::hazmat::PrehashVerifier;
use x509_cert::{
    certificate::{TbsCertificate, Version},
    name::Name,
    serial_number::SerialNumber,
    spki::{AlgorithmIdentifierOwned, ObjectIdentifier, SubjectPublicKeyInfoOwned},
    time::{Time, Validity},
    Certificate as X509Certificate,
};

#[test]
fn malformed_pfx_is_rejected_before_an_identity_or_private_key_is_exposed() {
    let error = PfxCertificateSource::from_pkcs12(b"not a PKCS#12 file", "password")
        .expect_err("malformed PKCS#12 data must fail during adapter construction");

    assert!(error.to_string().contains("PKCS#12"));
}

#[test]
fn missing_pfx_file_is_reported_without_listing_identities() {
    let error = PfxCertificateSource::from_file("missing-certificate.pfx", "password")
        .expect_err("a missing PFX file must fail before key import");

    assert!(error.to_string().contains("PFX file"));
}

#[test]
fn pfx_identity_lists_its_certificate_and_signs_a_sha256_digest_in_process() {
    let key = RsaPrivateKey::new(&mut OsRng, 2048).expect("test key generation should succeed");
    let pfx = pfx_with_identity(
        key.to_pkcs8_der()
            .expect("test RSA key should encode as PKCS#8")
            .as_bytes(),
        key.to_public_key()
            .to_public_key_der()
            .expect("test public key should encode as SPKI")
            .as_bytes(),
        "Local test signer",
        "password",
    );
    let source = PfxCertificateSource::from_pkcs12(&pfx, "password")
        .expect("a generated PKCS#12 container should import");
    let identity = source
        .list_identities()
        .pop()
        .expect("the PFX private-key entry should be listed");
    let digest = DocumentDigest::new(DigestAlgorithm::Sha256, vec![0xA5; 32])
        .expect("correct-length SHA-256 digest should wrap");

    let signature = source
        .sign_digest(
            &identity.id,
            &digest,
            SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256),
        )
        .expect("the imported RSA key should sign in process");
    let signature = Signature::try_from(signature.as_slice())
        .expect("RSA signature bytes should have the modulus width");

    assert_eq!(identity.display_name, "Local test signer");
    assert_eq!(identity.certificate_chain_der.len(), 1);
    VerifyingKey::<Sha256>::new(key.to_public_key())
        .verify_prehash(digest.as_bytes(), &signature)
        .expect("the returned signature should verify with the imported key");
}

#[test]
fn pfx_identity_signs_a_sha512_digest_with_p256_as_der_ecdsa() {
    let key = P256SigningKey::random(&mut OsRng);
    let pfx = pfx_with_identity(
        key.to_pkcs8_der()
            .expect("test P-256 key should encode as PKCS#8")
            .as_bytes(),
        key.verifying_key()
            .to_public_key_der()
            .expect("test P-256 public key should encode as SPKI")
            .as_bytes(),
        "P-256 signer",
        "password",
    );
    let source = PfxCertificateSource::from_pkcs12(&pfx, "password")
        .expect("a generated PKCS#12 container should import");
    let identity = source
        .list_identities()
        .pop()
        .expect("the PFX private-key entry should be listed");
    let digest = DocumentDigest::new(DigestAlgorithm::Sha512, vec![0x5A; 64])
        .expect("correct-length SHA-512 digest should wrap");

    let signature = source
        .sign_digest(
            &identity.id,
            &digest,
            SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha512),
        )
        .expect("the imported P-256 key should sign in process");
    let signature = P256Signature::from_der(&signature)
        .expect("P-256 adapter signatures must use DER ECDSA-Sig-Value");

    assert!(identity
        .supported_algorithms
        .contains(&SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha512)));
    P256VerifyingKey::from(&key)
        .verify_prehash(digest.as_bytes(), &signature)
        .expect("the returned signature should verify with the imported key");
}

#[test]
fn pfx_with_mismatched_rsa_key_and_leaf_certificate_is_not_an_identity() {
    let key = RsaPrivateKey::new(&mut OsRng, 2048).expect("test key generation should succeed");
    let certificate_key = RsaPrivateKey::new(&mut OsRng, 2048)
        .expect("test certificate key generation should succeed");
    let pfx = pfx_with_identity(
        key.to_pkcs8_der()
            .expect("test RSA key should encode")
            .as_bytes(),
        certificate_key
            .to_public_key()
            .to_public_key_der()
            .expect("test public key should encode")
            .as_bytes(),
        "Mismatched RSA signer",
        "password",
    );

    let source = PfxCertificateSource::from_pkcs12(&pfx, "password")
        .expect("a structurally valid PFX container should parse");

    assert!(source.list_identities().is_empty());
}

#[test]
fn pfx_with_mismatched_p256_key_and_leaf_certificate_is_not_an_identity() {
    let key = P256SigningKey::random(&mut OsRng);
    let certificate_key = P256SigningKey::random(&mut OsRng);
    let pfx = pfx_with_identity(
        key.to_pkcs8_der()
            .expect("test P-256 key should encode")
            .as_bytes(),
        certificate_key
            .verifying_key()
            .to_public_key_der()
            .expect("test P-256 public key should encode")
            .as_bytes(),
        "Mismatched P-256 signer",
        "password",
    );

    let source = PfxCertificateSource::from_pkcs12(&pfx, "password")
        .expect("a structurally valid PFX container should parse");

    assert!(source.list_identities().is_empty());
}

#[test]
fn pfx_with_two_certificate_issuer_cycle_imports_without_hanging() {
    let key = RsaPrivateKey::new(&mut OsRng, 2048).expect("test key generation should succeed");
    let public_key = key
        .to_public_key()
        .to_public_key_der()
        .expect("test public key should encode");
    let pfx = pfx_with_certificate_chain(
        key.to_pkcs8_der()
            .expect("test RSA key should encode")
            .as_bytes(),
        [
            test_certificate(public_key.as_bytes(), "CN=Cycle A", "CN=Cycle B"),
            test_certificate(public_key.as_bytes(), "CN=Cycle B", "CN=Cycle A"),
        ],
        "Cyclic certificate chain",
        "password",
    );

    let source = PfxCertificateSource::from_pkcs12(&pfx, "password")
        .expect("a cyclic issuer chain should terminate during import");

    assert_eq!(source.list_identities().len(), 1);
}

fn pfx_with_identity(
    private_key_der: &[u8],
    public_key_spki_der: &[u8],
    alias: &str,
    password: &str,
) -> Vec<u8> {
    let private_key = PrivateKey::from_der(private_key_der)
        .expect("PKCS#8 test key should import into the PFX writer");
    let certificate = Certificate::from_der(&test_certificate(
        public_key_spki_der,
        "CN=Local test signer",
        "CN=Local test signer",
    ))
    .expect("test certificate should import into the PFX writer");
    let mut store = KeyStore::new();
    store.add_entry(
        alias,
        KeyStoreEntry::PrivateKeyChain(PrivateKeyChain::new(
            "test-key-id",
            private_key,
            [certificate],
        )),
    );

    store
        .writer(password)
        .write()
        .expect("test keychain should serialize as PKCS#12")
}

fn pfx_with_certificate_chain(
    private_key_der: &[u8],
    certificates_der: impl IntoIterator<Item = Vec<u8>>,
    alias: &str,
    password: &str,
) -> Vec<u8> {
    let private_key = PrivateKey::from_der(private_key_der)
        .expect("PKCS#8 test key should import into the PFX writer");
    let certificates = certificates_der
        .into_iter()
        .map(|der| Certificate::from_der(&der).expect("test certificate should import"))
        .collect::<Vec<_>>();
    let mut store = KeyStore::new();
    store.add_entry(
        alias,
        KeyStoreEntry::PrivateKeyChain(PrivateKeyChain::new(
            "test-key-id",
            private_key,
            certificates,
        )),
    );

    store
        .writer(password)
        .write()
        .expect("test keychain should serialize as PKCS#12")
}

fn test_certificate(public_key_spki_der: &[u8], subject: &str, issuer: &str) -> Vec<u8> {
    let algorithm = AlgorithmIdentifierOwned {
        oid: ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11"),
        parameters: Some(Any::null()),
    };
    let public_key = SubjectPublicKeyInfoOwned::from_der(public_key_spki_der)
        .expect("test public key SPKI should decode");
    let validity = Validity {
        not_before: Time::UtcTime(test_time(2025)),
        not_after: Time::UtcTime(test_time(2035)),
    };
    let subject: Name = subject.parse().expect("test subject name should parse");
    let issuer: Name = issuer.parse().expect("test issuer name should parse");
    X509Certificate {
        tbs_certificate: TbsCertificate {
            version: Version::V3,
            serial_number: SerialNumber::new(&[1]).expect("test serial should be valid"),
            signature: algorithm.clone(),
            issuer,
            validity,
            subject,
            subject_public_key_info: public_key,
            issuer_unique_id: None,
            subject_unique_id: None,
            extensions: None,
        },
        signature_algorithm: algorithm,
        signature: BitString::from_bytes(&[1]).expect("test signature should encode"),
    }
    .to_der()
    .expect("test certificate should encode")
}

fn test_time(year: u16) -> UtcTime {
    DateTime::new(year, 1, 1, 0, 0, 0)
        .and_then(UtcTime::from_date_time)
        .expect("test time should be valid")
}
