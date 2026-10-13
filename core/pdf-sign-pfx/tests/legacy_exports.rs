//! `.p12` files as OpenSSL 1.x and LibreSSL export them (see
//! `fixtures/generate.sh`): the encryption schemes those defaults and
//! options produce either import and sign, or are refused with a reason the
//! person who chose the file can act on — never an empty identity list.

use p256::ecdsa::{Signature as P256Signature, VerifyingKey as P256VerifyingKey};
use pdf_sign::{CertificateSourcePort, DigestAlgorithm, DocumentDigest, SigningAlgorithm};
use pdf_sign_pfx::{PfxAdapterError, PfxCertificateSource};
use rsa::{
    pkcs1v15::{Signature as RsaSignature, VerifyingKey as RsaVerifyingKey},
    pkcs8::DecodePublicKey,
    RsaPublicKey,
};
use sha2::Sha256;
use signature::hazmat::PrehashVerifier;
use x509_cert::{
    der::{Decode, Encode},
    Certificate,
};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// Opens `name`, expects exactly one identity, and checks a SHA-256 digest
/// it signs verifies against that identity's own certificate.
fn signs_with_its_certificate(name: &str, algorithm: SigningAlgorithm) {
    let source = PfxCertificateSource::from_pkcs12(&fixture(name), "password")
        .unwrap_or_else(|error| panic!("{name} should import: {error}"));
    let identities = source.list_identities();
    assert_eq!(identities.len(), 1, "{name} holds one signing identity");
    let identity = &identities[0];
    let digest = DocumentDigest::new(DigestAlgorithm::Sha256, vec![0x3C; 32])
        .expect("correct-length SHA-256 digest should wrap");

    let signature = source
        .sign_digest(&identity.id, &digest, algorithm)
        .unwrap_or_else(|error| panic!("{name} should sign: {error}"));

    let leaf = Certificate::from_der(&identity.certificate_chain_der[0])
        .expect("the leaf certificate should parse");
    let spki = leaf
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .expect("the leaf's public key should encode");
    match algorithm {
        SigningAlgorithm::RsaPkcs1v15(_) => {
            let key = RsaPublicKey::from_public_key_der(&spki).expect("an RSA certificate");
            let signature = RsaSignature::try_from(signature.as_slice()).expect("RSA signature");
            RsaVerifyingKey::<Sha256>::new(key)
                .verify_prehash(digest.as_bytes(), &signature)
                .expect("the signature should verify with the certificate");
        }
        SigningAlgorithm::Ecdsa(_) => {
            let key = P256VerifyingKey::from_public_key_der(&spki).expect("a P-256 certificate");
            let signature = P256Signature::from_der(&signature).expect("DER ECDSA signature");
            key.verify_prehash(digest.as_bytes(), &signature)
                .expect("the signature should verify with the certificate");
        }
        _ => unreachable!("only RSA and ECDSA fixtures"),
    }
}

fn refusal(name: &str) -> String {
    match PfxCertificateSource::from_pkcs12(&fixture(name), "password") {
        Err(PfxAdapterError::Unusable(reason)) => reason,
        Err(other) => panic!("{name} should be refused as unusable, not {other:?}"),
        Ok(source) => panic!("{name} should be refused, not open with {source:?}"),
    }
}

const RSA_SHA256: SigningAlgorithm = SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256);
const ECDSA_SHA256: SigningAlgorithm = SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256);

#[test]
fn the_rc2_40_certificate_bag_of_the_old_defaults_is_read() {
    signs_with_its_certificate("rsa-rc2-40.p12", RSA_SHA256);
    signs_with_its_certificate("p256-rc2-40.p12", ECDSA_SHA256);
}

#[test]
fn pbes2_with_the_default_hmac_sha1_prf_is_read_with_aes_and_3des() {
    signs_with_its_certificate("rsa-pbes2-aes-256.p12", RSA_SHA256);
    signs_with_its_certificate("rsa-pbes2-3des.p12", RSA_SHA256);
}

#[test]
fn a_wrong_password_is_still_a_wrong_password() {
    let error = PfxCertificateSource::from_pkcs12(&fixture("rsa-rc2-40.p12"), "not it")
        .expect_err("the MAC should not verify");

    assert!(matches!(error, PfxAdapterError::Pkcs12(_)));
}

#[test]
fn an_unsupported_encryption_scheme_says_how_to_export_again() {
    let reason = refusal("rsa-rc4-128.p12");

    assert!(
        reason.contains("encryption scheme that is not supported"),
        "{reason}"
    );
    assert!(reason.contains("AES-256 or 3DES"), "{reason}");
}

#[test]
fn a_spelled_out_curve_says_to_name_it() {
    let reason = refusal("p256-explicit-curve.p12");

    assert!(reason.contains("instead of naming the curve"), "{reason}");
}

#[test]
fn a_key_that_is_neither_rsa_nor_p256_is_named_as_such() {
    let reason = refusal("p384.p12");

    assert!(reason.contains("not an RSA or P-256 key"), "{reason}");
}
