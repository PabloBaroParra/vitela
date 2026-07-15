//! Integration test (T-078, TDD): the fixture generator must produce a
//! known-good signed-PDF corpus (rcgen self-signed identities, test-only)
//! where every fixture:
//! - remains a loadable PDF containing the expected number of
//!   `adbe.pkcs7.detached` signature dictionaries,
//! - carries a `/ByteRange` that covers the complete signed revision except
//!   the `/Contents` gap,
//! - embeds a CMS `SignedData` whose `message-digest` signed attribute
//!   matches the digest recomputed over the byte ranges, and whose signature
//!   cryptographically verifies against the embedded self-signed certificate,
//! - for the two-signature fixture: the first signature stays verifiable
//!   after the second one is appended (spec.md acceptance criterion).

use std::path::PathBuf;

use cms::{cert::CertificateChoices, content_info::ContentInfo, signed_data::SignedData};
use der::{
    asn1::{ObjectIdentifier, OctetString},
    Decode, Encode, Header, SliceReader,
};
use gen_fixtures::signed::{generate_signed_corpus, SignedAlgorithm, SIGNED_CORPUS};
use lopdf::{Document, Object};
use p256::ecdsa::{Signature as P256Signature, VerifyingKey as P256VerifyingKey};
use rsa::{
    pkcs1v15::{Signature as RsaSignature, VerifyingKey as RsaVerifyingKey},
    pkcs8::DecodePublicKey,
    RsaPublicKey,
};
use sha2::{Digest, Sha256, Sha384};
use signature::hazmat::PrehashVerifier;
use x509_cert::Certificate;

const ID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const ID_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const SHA_256_WITH_RSA_ENCRYPTION: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const ECDSA_WITH_SHA_256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");

fn unique_temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "gen-fixtures-signed-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// One parsed signature: its four `/ByteRange` integers and the DER `CMS`
/// value extracted from `/Contents` (zero padding stripped via the DER
/// header's own length).
struct ParsedSignature {
    byte_range: [u64; 4],
    cms_der: Vec<u8>,
}

fn parsed_signatures(bytes: &[u8]) -> Vec<ParsedSignature> {
    let doc = Document::load_mem(bytes).expect("fixture must remain a loadable PDF");
    let mut signatures = Vec::new();
    for object in doc.objects.values() {
        let Object::Dictionary(dictionary) = object else {
            continue;
        };
        let is_signature = matches!(
            dictionary.get(b"SubFilter"),
            Ok(Object::Name(name)) if name == b"adbe.pkcs7.detached"
        );
        if !is_signature {
            continue;
        }
        let Ok(Object::Array(range)) = dictionary.get(b"ByteRange") else {
            panic!("signature dictionary must carry /ByteRange");
        };
        let byte_range: Vec<u64> = range
            .iter()
            .map(|value| match value {
                Object::Integer(int) => u64::try_from(*int).expect("byte range must be positive"),
                other => panic!("unexpected /ByteRange element: {other:?}"),
            })
            .collect();
        let Ok(Object::String(contents, _)) = dictionary.get(b"Contents") else {
            panic!("signature dictionary must carry /Contents");
        };
        let mut reader = SliceReader::new(contents).expect("contents must fit a DER reader");
        let header = Header::decode(&mut reader).expect("contents must start with a DER header");
        let der_len = usize::try_from(
            (header.encoded_len().expect("DER header length") + header.length)
                .expect("DER value length"),
        )
        .expect("DER length fits usize");
        signatures.push(ParsedSignature {
            byte_range: byte_range
                .try_into()
                .expect("/ByteRange must hold 4 integers"),
            cms_der: contents[..der_len].to_vec(),
        });
    }
    signatures.sort_by_key(|signature| signature.byte_range[2] + signature.byte_range[3]);
    signatures
}

fn ranged_bytes(bytes: &[u8], range: [u64; 4]) -> Vec<u8> {
    let [start_a, len_a, start_b, len_b] = range.map(|value| value as usize);
    let mut covered = Vec::with_capacity(len_a + len_b);
    covered.extend_from_slice(&bytes[start_a..start_a + len_a]);
    covered.extend_from_slice(&bytes[start_b..start_b + len_b]);
    covered
}

fn digest_for(algorithm: SignedAlgorithm, data: &[u8]) -> Vec<u8> {
    match algorithm {
        SignedAlgorithm::Rsa2048Sha256 | SignedAlgorithm::P256Sha256 => {
            Sha256::digest(data).to_vec()
        }
        SignedAlgorithm::P256Sha384 => Sha384::digest(data).to_vec(),
    }
}

/// Extracts (signed_attrs_der, message_digest, signature, leaf_certificate)
/// from a detached CMS `SignedData`.
fn cms_parts(cms_der: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>, Certificate) {
    let content_info =
        ContentInfo::from_der(cms_der).expect("/Contents must hold a DER ContentInfo");
    assert_eq!(content_info.content_type, ID_SIGNED_DATA);
    let signed_data: SignedData = content_info
        .content
        .decode_as()
        .expect("ContentInfo must wrap SignedData");
    assert!(
        signed_data.encap_content_info.econtent.is_none(),
        "PDF signatures must be detached"
    );

    let signer_info = signed_data
        .signer_infos
        .0
        .iter()
        .next()
        .expect("SignedData must carry one SignerInfo");
    let signed_attrs = signer_info
        .signed_attrs
        .as_ref()
        .expect("PDF CMS signatures must carry signed attributes");
    let message_digest = signed_attrs
        .iter()
        .find(|attribute| attribute.oid == ID_MESSAGE_DIGEST)
        .and_then(|attribute| attribute.values.iter().next())
        .map(|value| {
            value
                .decode_as::<OctetString>()
                .expect("message-digest must be an OCTET STRING")
                .as_bytes()
                .to_vec()
        })
        .expect("SignedData must carry a message-digest attribute");
    let signed_attrs_der = signed_attrs
        .to_der()
        .expect("signed attributes must re-encode as SET OF");

    let certificates = signed_data
        .certificates
        .as_ref()
        .expect("SignedData must embed the signer certificate");
    let leaf = certificates
        .0
        .iter()
        .find_map(|choice| match choice {
            CertificateChoices::Certificate(certificate) => Some(certificate.clone()),
            _ => None,
        })
        .expect("certificate set must contain an X.509 certificate");

    (
        signed_attrs_der,
        message_digest,
        signer_info.signature.as_bytes().to_vec(),
        leaf,
    )
}

/// Asserts the signature fields are discoverable the way compliant readers
/// discover them (PDF 32000-1 §12.7.2): through `/AcroForm /Fields` and the
/// page's `/Annots`, not by scanning every indirect object.
fn assert_fields_registered(bytes: &[u8], expected: usize) {
    let doc = Document::load_mem(bytes).expect("fixture must remain a loadable PDF");
    let resolve_array = |value: &Object| -> Vec<Object> {
        match value {
            Object::Array(array) => array.clone(),
            Object::Reference(id) => doc
                .get_object(*id)
                .and_then(Object::as_array)
                .expect("referenced array must resolve")
                .clone(),
            other => panic!("expected an array or reference, found {other:?}"),
        }
    };

    let catalog = doc.catalog().expect("fixture must expose its catalog");
    let acro_form = match catalog
        .get(b"AcroForm")
        .expect("catalog must carry /AcroForm")
    {
        Object::Dictionary(dictionary) => dictionary.clone(),
        Object::Reference(id) => doc
            .get_object(*id)
            .and_then(Object::as_dict)
            .expect("referenced /AcroForm must resolve")
            .clone(),
        other => panic!("unexpected /AcroForm value: {other:?}"),
    };
    assert_eq!(
        acro_form.get(b"SigFlags").and_then(Object::as_i64).ok(),
        Some(3),
        "/AcroForm must set SigFlags = SignaturesExist | AppendOnly"
    );
    let fields = resolve_array(
        acro_form
            .get(b"Fields")
            .expect("/AcroForm must carry /Fields"),
    );
    assert_eq!(
        fields.len(),
        expected,
        "/AcroForm /Fields must list every signature field"
    );
    for field in &fields {
        let Object::Reference(field_id) = field else {
            panic!("/Fields entries must be references");
        };
        let field = doc
            .get_object(*field_id)
            .and_then(Object::as_dict)
            .expect("field reference must resolve");
        assert_eq!(
            field.get(b"FT").and_then(Object::as_name).ok(),
            Some(b"Sig".as_slice()),
            "every /Fields entry must be a signature field"
        );
    }

    let page_id = *doc
        .get_pages()
        .get(&1)
        .expect("fixture must keep its single page");
    let page = doc
        .get_object(page_id)
        .and_then(Object::as_dict)
        .expect("page must resolve");
    let annotations = resolve_array(page.get(b"Annots").expect("page must carry /Annots"));
    assert_eq!(
        annotations.len(),
        expected,
        "the page's /Annots must accumulate one widget per signature"
    );
}

/// Asserts the embedded certificate is genuinely self-signed: its own X.509
/// signature verifies against its own public key.
fn verify_certificate_self_signature(certificate: &Certificate) {
    let tbs_der = certificate
        .tbs_certificate
        .to_der()
        .expect("TBS certificate must re-encode");
    let raw_signature = certificate
        .signature
        .as_bytes()
        .expect("certificate signature must be byte-aligned");
    let spki_der = certificate
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .expect("leaf SPKI must re-encode");

    match certificate.signature_algorithm.oid {
        SHA_256_WITH_RSA_ENCRYPTION => {
            let public_key = RsaPublicKey::from_public_key_der(&spki_der)
                .expect("self-signed RSA certificate must carry an RSA SPKI");
            let verifying_key = RsaVerifyingKey::<Sha256>::new(public_key);
            let signature =
                RsaSignature::try_from(raw_signature).expect("certificate signature must parse");
            verifying_key
                .verify_prehash(&Sha256::digest(&tbs_der), &signature)
                .expect("certificate must be validly self-signed (RSA)");
        }
        ECDSA_WITH_SHA_256 => {
            let verifying_key = P256VerifyingKey::from_public_key_der(&spki_der)
                .expect("self-signed ECDSA certificate must carry a P-256 SPKI");
            let signature =
                P256Signature::from_der(raw_signature).expect("certificate signature must be DER");
            verifying_key
                .verify_prehash(&Sha256::digest(&tbs_der), &signature)
                .expect("certificate must be validly self-signed (ECDSA)");
        }
        other => panic!("unexpected certificate signature algorithm: {other}"),
    }
}

fn verify_signature(
    algorithm: SignedAlgorithm,
    certificate: &Certificate,
    signed_attrs_der: &[u8],
    signature: &[u8],
) {
    let spki_der = certificate
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .expect("leaf SPKI must re-encode");
    match algorithm {
        SignedAlgorithm::Rsa2048Sha256 => {
            let public_key = RsaPublicKey::from_public_key_der(&spki_der)
                .expect("leaf SPKI must hold an RSA key");
            let verifying_key = RsaVerifyingKey::<Sha256>::new(public_key);
            let signature = RsaSignature::try_from(signature).expect("RSA signature must parse");
            verifying_key
                .verify_prehash(&Sha256::digest(signed_attrs_der), &signature)
                .expect("RSA signature over signed attributes must verify");
        }
        SignedAlgorithm::P256Sha256 | SignedAlgorithm::P256Sha384 => {
            let verifying_key = P256VerifyingKey::from_public_key_der(&spki_der)
                .expect("leaf SPKI must hold a P-256 key");
            let signature =
                P256Signature::from_der(signature).expect("ECDSA signature must be DER");
            verifying_key
                .verify_prehash(&digest_for(algorithm, signed_attrs_der), &signature)
                .expect("ECDSA signature over signed attributes must verify");
        }
    }
}

#[test]
fn corpus_covers_rsa_ecdsa_and_a_second_signature() {
    let algorithms: Vec<SignedAlgorithm> =
        SIGNED_CORPUS.iter().map(|spec| spec.algorithm).collect();
    assert!(algorithms.contains(&SignedAlgorithm::Rsa2048Sha256));
    assert!(algorithms.contains(&SignedAlgorithm::P256Sha256));
    assert!(algorithms.contains(&SignedAlgorithm::P256Sha384));
    assert!(
        SIGNED_CORPUS.iter().any(|spec| spec.signatures == 2),
        "corpus must include a two-signature fixture"
    );
}

#[test]
fn every_fixture_carries_verifiable_known_good_signatures() {
    let out_dir = unique_temp_dir("verify");
    let written = generate_signed_corpus(&out_dir).expect("generate_signed_corpus should succeed");
    assert_eq!(
        written.len(),
        SIGNED_CORPUS.len(),
        "one file per corpus spec"
    );

    for spec in SIGNED_CORPUS {
        let bytes = std::fs::read(out_dir.join(spec.file_name)).expect("fixture file must exist");
        let signatures = parsed_signatures(&bytes);
        assert_eq!(
            signatures.len(),
            spec.signatures,
            "{}: expected {} signature dictionaries",
            spec.file_name,
            spec.signatures
        );

        let last = signatures.last().expect("at least one signature");
        assert_eq!(
            last.byte_range[2] + last.byte_range[3],
            bytes.len() as u64,
            "{}: the last signature's ranges must end at the file end",
            spec.file_name
        );

        assert_fields_registered(&bytes, spec.signatures);

        for signature in &signatures {
            assert_eq!(signature.byte_range[0], 0, "ranges must start at offset 0");
            let (signed_attrs_der, message_digest, raw_signature, certificate) =
                cms_parts(&signature.cms_der);
            let recomputed =
                digest_for(spec.algorithm, &ranged_bytes(&bytes, signature.byte_range));
            assert_eq!(
                message_digest, recomputed,
                "{}: message-digest attribute must match the byte-range digest",
                spec.file_name
            );
            verify_certificate_self_signature(&certificate);
            verify_signature(
                spec.algorithm,
                &certificate,
                &signed_attrs_der,
                &raw_signature,
            );
        }
    }

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn second_signature_preserves_the_first_revision_ranges() {
    let out_dir = unique_temp_dir("two-sigs");
    generate_signed_corpus(&out_dir).expect("generate_signed_corpus should succeed");

    let spec = SIGNED_CORPUS
        .iter()
        .find(|spec| spec.signatures == 2)
        .expect("corpus must include a two-signature fixture");
    let bytes = std::fs::read(out_dir.join(spec.file_name)).expect("fixture file must exist");
    let signatures = parsed_signatures(&bytes);
    assert_eq!(signatures.len(), 2);

    let first_end = signatures[0].byte_range[2] + signatures[0].byte_range[3];
    assert!(
        first_end < bytes.len() as u64,
        "the first signature must cover only its own (earlier) revision"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}
