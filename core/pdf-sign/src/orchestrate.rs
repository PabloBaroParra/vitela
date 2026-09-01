//! Production signing pipeline (T-178, `docs/batch-digital-signature.md`):
//! opens a document, appends an unsigned signature widget, digests it, asks a
//! [`CertificateSourcePort`] to sign that digest, and writes the result back.
//!
//! This is the same five-step dance
//! `tests/fixtures/gen-fixtures/src/signed.rs`'s `append_signed_revision`
//! already proves works — that function stays as the fixture corpus's own
//! generator, this is its production twin: typed errors instead of
//! `io::Error`, a caller-chosen page instead of a hardcoded `1`, and a
//! signing algorithm picked from the identity's own capabilities (batch
//! decision 7) instead of a hardcoded one.

use lopdf::{Dictionary, Object};
use pdf_save::ObjectSink;

use crate::{
    append_signature_bytes, digest_byte_ranges, prepare_signature_bytes, CertificateSourcePort,
    CmsSignedDataBuilder, SignError, SignatureFieldBuilder,
};

/// Signs `bytes` with the identity `identity_id` names on `source`, adding a
/// new (invisible — batch decision 4) signature field to page `page_number`
/// (one-based, matching `lopdf`'s own page-numbering convention).
///
/// `document_password` opens an encrypted document the same way
/// `pdf_manip::open_document_from_bytes` always has — `None` for an
/// unencrypted one.
///
/// The signing algorithm is the first entry in the identity's
/// `supported_algorithms` — nothing here asks the caller to choose one by
/// hand (decision 7: a user without cryptography background has no basis to
/// pick between RSA and ECDSA).
///
/// # Errors
///
/// [`SignError::PageNotFound`] if `page_number` does not exist,
/// [`SignError::IdentityUnavailable`] if `identity_id` is not currently
/// listed by `source`, [`SignError::NoSupportedAlgorithm`] if the identity
/// advertises no algorithm at all, [`SignError::DocumentOpen`]/
/// [`SignError::IncrementalWrite`] for the `pdf-manip`/`pdf-save` steps, or
/// any other `SignError` from digesting, building the CMS value, or
/// inserting it (see those functions' own docs).
pub fn sign_document(
    bytes: Vec<u8>,
    document_password: Option<&str>,
    page_number: u32,
    field_name: impl Into<String>,
    source: &dyn CertificateSourcePort,
    identity_id: &str,
) -> Result<Vec<u8>, SignError> {
    let identity = source
        .list_identities()
        .into_iter()
        .find(|identity| identity.id == identity_id)
        .ok_or_else(|| SignError::IdentityUnavailable {
            identity_id: identity_id.to_owned(),
        })?;
    let signing_algorithm = identity
        .supported_algorithms
        .first()
        .copied()
        .ok_or_else(|| SignError::NoSupportedAlgorithm {
            identity_id: identity_id.to_owned(),
        })?;

    let (base, _security) = pdf_manip::open_document_from_bytes(&bytes, document_password)
        .map_err(|error| SignError::DocumentOpen {
            message: error.to_string(),
        })?;
    let page_object_id = *base
        .as_lopdf()
        .get_pages()
        .get(&page_number)
        .ok_or(SignError::PageNotFound { page_number })?;
    let catalog_id = base
        .as_lopdf()
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| SignError::DocumentOpen {
            message: "document trailer has no /Root reference".to_owned(),
        })?;

    let placeholder = SignatureFieldBuilder::new(field_name, page_object_id, [0.0; 4]).build()?;
    let contents_capacity = placeholder.contents_capacity;
    let signature_dictionary = placeholder.signature_dictionary;
    let field_dictionary = placeholder.field_dictionary;

    let with_field = pdf_save::append_incremental_update(bytes, base, move |writer| {
        let signature_id = writer.add_object(Object::Dictionary(signature_dictionary));
        let mut field = field_dictionary;
        field.set("V", Object::Reference(signature_id));
        let field_id = writer.add_object(Object::Dictionary(field));

        // A document that already carries a signature must extend /Annots,
        // never replace it — the array may already be inline or an indirect
        // object depending on how the earlier signature (or this shell's own
        // annotations) wrote it.
        let annotations_entry = writer
            .page_dict_mut(page_object_id)?
            .get(b"Annots")
            .cloned();
        match annotations_entry {
            Ok(Object::Reference(annotations_id)) => {
                writer.opt_clone_object_to_new_document(annotations_id)?;
                writer
                    .new_document
                    .get_object_mut(annotations_id)
                    .and_then(Object::as_array_mut)?
                    .push(Object::Reference(field_id));
            }
            Ok(Object::Array(mut annotations)) => {
                annotations.push(Object::Reference(field_id));
                writer
                    .page_dict_mut(page_object_id)?
                    .set("Annots", annotations);
            }
            _ => {
                writer
                    .page_dict_mut(page_object_id)?
                    .set("Annots", vec![Object::Reference(field_id)]);
            }
        }

        // Compliant readers discover signature fields through /AcroForm
        // /Fields (PDF 32000-1 §12.7.2) as well as the page's /Annots;
        // register the field there too, with SigFlags = SignaturesExist |
        // AppendOnly (3), same as every signed fixture in the test corpus.
        writer.opt_clone_object_to_new_document(catalog_id)?;
        let acro_form_entry = writer
            .new_document
            .get_object(catalog_id)
            .and_then(Object::as_dict)?
            .get(b"AcroForm")
            .cloned();
        let mut acro_form = match acro_form_entry {
            Ok(Object::Dictionary(existing)) => existing,
            _ => Dictionary::new(),
        };
        let mut fields = match acro_form.get(b"Fields") {
            Ok(Object::Array(existing)) => existing.clone(),
            _ => Vec::new(),
        };
        fields.push(Object::Reference(field_id));
        acro_form.set("Fields", fields);
        acro_form.set("SigFlags", 3);
        writer
            .new_document
            .get_object_mut(catalog_id)
            .and_then(Object::as_dict_mut)?
            .set("AcroForm", Object::Dictionary(acro_form));
        Ok(())
    })
    .map_err(|error| SignError::IncrementalWrite {
        message: error.to_string(),
    })?;

    let prepared = prepare_signature_bytes(with_field, contents_capacity)?;
    let digest = digest_byte_ranges(
        &prepared.bytes,
        prepared.byte_range,
        signing_algorithm.digest_algorithm(),
    )?;
    let cms = CmsSignedDataBuilder::new(source, &identity, &digest, signing_algorithm).build()?;
    append_signature_bytes(prepared.bytes, prepared.byte_range, &cms)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use der::asn1::{Any, BitString, UtcTime};
    use der::{DateTime, Encode};
    use spki::{AlgorithmIdentifierOwned, ObjectIdentifier, SubjectPublicKeyInfoOwned};
    use x509_cert::certificate::{TbsCertificate, Version};
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::time::{Time, Validity};
    use x509_cert::Certificate as X509Certificate;

    use crate::{DigestAlgorithm, SigningAlgorithm, SigningIdentity};

    use super::*;

    const RSA_ENCRYPTION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
    const SHA_256_WITH_RSA_ENCRYPTION: ObjectIdentifier =
        ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
    /// Arbitrary bytes standing in for a real platform signature.
    /// `CmsSignedDataBuilder` never verifies the signature cryptographically
    /// (that is the reader's job, not the writer's) — it only checks the
    /// certificate's public-key-algorithm OID matches the requested signing
    /// algorithm family and that the bytes aren't empty, exactly what
    /// `cms.rs`'s own `RecordingCertificateSource`/`test_certificate` test
    /// helpers already rely on. `sign_document`'s five-step *plumbing* is
    /// what these tests exercise, not real cryptography — that is already
    /// covered by `pdf-sign-pfx`'s and `pdf-sign-pkcs11`'s own test suites
    /// against their real adapters.
    const FAKE_SIGNATURE: &[u8] = b"orchestrate-test-signature";

    /// A syntactically well-formed but unsigned (no real CA, no real key)
    /// RSA leaf certificate DER — same shape `cms.rs`'s own
    /// `test_certificate` helper builds, duplicated here rather than shared
    /// because it is `cfg(test)`-only in both places and small enough that a
    /// shared home would be more ceremony than the duplication it removes.
    fn fake_rsa_certificate_der() -> Vec<u8> {
        let name = Name::from_str("CN=pdf-sign orchestrate test").expect("test name must parse");
        let certificate_algorithm = AlgorithmIdentifierOwned {
            oid: SHA_256_WITH_RSA_ENCRYPTION,
            parameters: Some(Any::null()),
        };
        let public_key_algorithm = AlgorithmIdentifierOwned {
            oid: RSA_ENCRYPTION,
            parameters: Some(Any::null()),
        };
        let not_before = DateTime::new(2025, 1, 1, 0, 0, 0)
            .and_then(UtcTime::from_date_time)
            .expect("test start time must be valid");
        let not_after = DateTime::new(2035, 1, 1, 0, 0, 0)
            .and_then(UtcTime::from_date_time)
            .expect("test end time must be valid");

        let certificate = X509Certificate {
            tbs_certificate: TbsCertificate {
                version: Version::V3,
                serial_number: SerialNumber::new(&[1]).expect("test serial must be valid"),
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
                        .expect("test public key must encode"),
                },
                issuer_unique_id: None,
                subject_unique_id: None,
                extensions: None,
            },
            signature_algorithm: certificate_algorithm,
            signature: BitString::from_bytes(&[4, 5, 6])
                .expect("test certificate signature must encode"),
        };

        certificate.to_der().expect("test certificate must encode")
    }

    struct FakeCertificateSource {
        identity: SigningIdentity,
    }

    impl FakeCertificateSource {
        fn new() -> Self {
            Self {
                identity: SigningIdentity {
                    id: "orchestrate-test-identity".to_owned(),
                    display_name: "Orchestrate test signer".to_owned(),
                    certificate_chain_der: vec![fake_rsa_certificate_der()],
                    supported_algorithms: vec![SigningAlgorithm::RsaPkcs1v15(
                        DigestAlgorithm::Sha256,
                    )],
                },
            }
        }
    }

    impl CertificateSourcePort for FakeCertificateSource {
        fn list_identities(&self) -> Vec<SigningIdentity> {
            vec![self.identity.clone()]
        }

        fn sign_digest_raw(
            &self,
            identity_id: &str,
            _digest: &[u8],
            _algorithm: SigningAlgorithm,
        ) -> Result<Vec<u8>, SignError> {
            if identity_id != self.identity.id {
                return Err(SignError::IdentityUnavailable {
                    identity_id: identity_id.to_owned(),
                });
            }
            Ok(FAKE_SIGNATURE.to_vec())
        }
    }

    /// `create_blank_document` alone has zero pages (`Kids: []`) — this adds
    /// the one page these tests need to sign.
    fn blank_document_bytes() -> Vec<u8> {
        let empty = pdf_manip::create_blank_document(
            pdf_document::PageSize::A4,
            pdf_document::Orientation::Portrait,
        );
        let one_page = pdf_manip::insert_blank_page(
            &empty,
            0,
            pdf_document::PageSize::A4,
            pdf_document::Orientation::Portrait,
        )
        .expect("test document must accept a blank page");
        let mut document = one_page.into_lopdf();
        let mut bytes = Vec::new();
        document
            .save_to(&mut bytes)
            .expect("blank test document must serialize");
        bytes
    }

    #[test]
    fn signs_a_document_with_no_prior_acroform() {
        let source = FakeCertificateSource::new();
        let bytes = blank_document_bytes();

        let signed = sign_document(bytes, None, 1, "Signature_1", &source, &source.identity.id)
            .expect("signing a blank document must succeed");

        let (document, _) =
            pdf_manip::open_document_from_bytes(&signed, None).expect("signed bytes must reopen");
        let root_id = document
            .as_lopdf()
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .expect("signed document must keep its /Root reference");
        let acro_form = document
            .as_lopdf()
            .get_object(root_id)
            .and_then(Object::as_dict)
            .and_then(|catalog| catalog.get(b"AcroForm"))
            .and_then(Object::as_dict)
            .expect("signing must add an /AcroForm dictionary");
        let fields = acro_form
            .get(b"Fields")
            .and_then(Object::as_array)
            .expect("/AcroForm must carry a /Fields array");
        assert_eq!(fields.len(), 1, "exactly one signature field must be added");
    }

    #[test]
    fn signing_twice_keeps_both_fields() {
        let source = FakeCertificateSource::new();
        let bytes = blank_document_bytes();

        let once = sign_document(bytes, None, 1, "Signature_1", &source, &source.identity.id)
            .expect("first signature must succeed");
        let twice = sign_document(once, None, 1, "Signature_2", &source, &source.identity.id)
            .expect("second signature must succeed");

        let (document, _) = pdf_manip::open_document_from_bytes(&twice, None)
            .expect("twice-signed bytes must reopen");
        let root_id = document
            .as_lopdf()
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .expect("twice-signed document must keep its /Root reference");
        let fields = document
            .as_lopdf()
            .get_object(root_id)
            .and_then(Object::as_dict)
            .and_then(|catalog| catalog.get(b"AcroForm"))
            .and_then(Object::as_dict)
            .and_then(|acro_form| acro_form.get(b"Fields"))
            .and_then(Object::as_array)
            .expect("twice-signed document must keep an /AcroForm /Fields array");
        assert_eq!(
            fields.len(),
            2,
            "a second signature must add to /Fields, not replace the first entry"
        );
    }

    #[test]
    fn rejects_a_page_number_that_does_not_exist() {
        let source = FakeCertificateSource::new();
        let bytes = blank_document_bytes();

        let error = sign_document(bytes, None, 2, "Signature_1", &source, &source.identity.id)
            .expect_err("a blank one-page document has no page 2");

        assert_eq!(error, SignError::PageNotFound { page_number: 2 });
    }

    #[test]
    fn rejects_an_identity_id_the_source_does_not_list() {
        let source = FakeCertificateSource::new();
        let bytes = blank_document_bytes();

        let error = sign_document(
            bytes,
            None,
            1,
            "Signature_1",
            &source,
            "not-a-real-identity",
        )
        .expect_err("an unknown identity id must be refused before touching the document");

        assert_eq!(
            error,
            SignError::IdentityUnavailable {
                identity_id: "not-a-real-identity".to_owned()
            }
        );
    }
}
