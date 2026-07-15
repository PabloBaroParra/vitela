//! Generates the signed-PDF test corpus (T-078): known-good fixtures signed
//! with rcgen self-signed identities, strictly for test use.
//!
//! Every fixture travels through the REAL production pipeline — the base
//! document is reloaded like a user file, the signature field is appended
//! through pdf-save's incremental hook, the byte-range digest is produced by
//! `pdf_sign::digest_byte_ranges`, the CMS `SignedData` is built by
//! `CmsSignedDataBuilder`, and signing happens through the production
//! `PfxCertificateSource` adapter. Nothing here reimplements signing.

use std::io;
use std::path::{Path, PathBuf};

use lopdf::{Dictionary, Object};
use p12_keystore::{
    Certificate as P12Certificate, KeyStore, KeyStoreEntry, PrivateKey, PrivateKeyChain,
};
use pdf_save::ObjectSink;
use pdf_sign::{
    append_signature_bytes, digest_byte_ranges, prepare_signature_bytes, CertificateSourcePort,
    CmsSignedDataBuilder, DigestAlgorithm, SignatureFieldBuilder, SigningAlgorithm,
    DEFAULT_SIGNATURE_CAPACITY,
};
use pdf_sign_pfx::PfxCertificateSource;
use rand::rngs::OsRng;
use rcgen::{CertificateParams, DnType, KeyPair, PKCS_ECDSA_P256_SHA256};
use rsa::pkcs8::EncodePrivateKey;
use rsa::RsaPrivateKey;

/// Password protecting every generated test PFX identity. Test-only.
pub const FIXTURE_PFX_PASSWORD: &str = "fixture-pass";

/// Key and digest combination used to sign a corpus fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignedAlgorithm {
    /// RSA-2048 with RSASSA-PKCS1-v1_5 and SHA-256.
    Rsa2048Sha256,
    /// ECDSA P-256 with SHA-256.
    P256Sha256,
    /// ECDSA P-256 with SHA-384.
    P256Sha384,
}

impl SignedAlgorithm {
    fn digest(self) -> DigestAlgorithm {
        match self {
            Self::Rsa2048Sha256 | Self::P256Sha256 => DigestAlgorithm::Sha256,
            Self::P256Sha384 => DigestAlgorithm::Sha384,
        }
    }

    fn signing(self) -> SigningAlgorithm {
        match self {
            Self::Rsa2048Sha256 => SigningAlgorithm::RsaPkcs1v15(DigestAlgorithm::Sha256),
            Self::P256Sha256 => SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha256),
            Self::P256Sha384 => SigningAlgorithm::Ecdsa(DigestAlgorithm::Sha384),
        }
    }
}

/// Describes one signed fixture: output file name, signing algorithm, and
/// how many sequential signatures the file carries.
#[derive(Debug, Clone, Copy)]
pub struct SignedFixtureSpec {
    pub file_name: &'static str,
    pub algorithm: SignedAlgorithm,
    pub signatures: usize,
}

/// The signed-PDF corpus: single signatures across the supported key/digest
/// matrix plus one doubly-signed fixture proving the second signature
/// preserves the first (spec.md acceptance criterion).
pub const SIGNED_CORPUS: &[SignedFixtureSpec] = &[
    SignedFixtureSpec {
        file_name: "rsa2048_sha256.pdf",
        algorithm: SignedAlgorithm::Rsa2048Sha256,
        signatures: 1,
    },
    SignedFixtureSpec {
        file_name: "p256_sha256.pdf",
        algorithm: SignedAlgorithm::P256Sha256,
        signatures: 1,
    },
    SignedFixtureSpec {
        file_name: "p256_sha384.pdf",
        algorithm: SignedAlgorithm::P256Sha384,
        signatures: 1,
    },
    SignedFixtureSpec {
        file_name: "two_signatures_rsa2048_sha256.pdf",
        algorithm: SignedAlgorithm::Rsa2048Sha256,
        signatures: 2,
    },
];

fn other(error: impl ToString) -> io::Error {
    io::Error::other(error.to_string())
}

/// Builds a PKCS#12 container holding one rcgen self-signed identity for
/// `algorithm`, protected with [`FIXTURE_PFX_PASSWORD`].
fn self_signed_pfx(algorithm: SignedAlgorithm, common_name: &str) -> io::Result<Vec<u8>> {
    let key_pair = match algorithm {
        SignedAlgorithm::Rsa2048Sha256 => {
            let key = RsaPrivateKey::new(&mut OsRng, 2048).map_err(other)?;
            let pkcs8 = key.to_pkcs8_der().map_err(other)?;
            KeyPair::try_from(pkcs8.as_bytes()).map_err(other)?
        }
        SignedAlgorithm::P256Sha256 | SignedAlgorithm::P256Sha384 => {
            KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(other)?
        }
    };

    let mut params = CertificateParams::new(Vec::<String>::new()).map_err(other)?;
    params
        .distinguished_name
        .push(DnType::CommonName, common_name);
    let certificate = params.self_signed(&key_pair).map_err(other)?;

    let private_key = PrivateKey::from_der(&key_pair.serialize_der()).map_err(other)?;
    let certificate = P12Certificate::from_der(certificate.der()).map_err(other)?;
    let mut store = KeyStore::new();
    store.add_entry(
        common_name,
        KeyStoreEntry::PrivateKeyChain(PrivateKeyChain::new(
            "fixture-key-id",
            private_key,
            [certificate],
        )),
    );
    store.writer(FIXTURE_PFX_PASSWORD).write().map_err(other)
}

/// Appends one signed revision to `bytes` through the production pipeline:
/// incremental signature field, byte-range digest, CMS build, and signature
/// insertion.
fn append_signed_revision(
    bytes: Vec<u8>,
    source: &PfxCertificateSource,
    algorithm: SignedAlgorithm,
    field_name: &str,
) -> io::Result<Vec<u8>> {
    let (base, _security) = pdf_manip::open_document_from_bytes(&bytes, None).map_err(other)?;
    let page_object_id = *base
        .as_lopdf()
        .get_pages()
        .get(&1)
        .ok_or_else(|| other("fixture base document must contain one page"))?;
    let catalog_id = base
        .as_lopdf()
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(other)?;

    let placeholder = SignatureFieldBuilder::new(field_name, page_object_id, [0.0; 4])
        .build()
        .map_err(other)?;
    let signature_dictionary = placeholder.signature_dictionary;
    let field_dictionary = placeholder.field_dictionary;

    let with_field = pdf_save::append_incremental_update(bytes, base, move |writer| {
        let signature_id = writer.add_object(Object::Dictionary(signature_dictionary));
        let mut field = field_dictionary;
        field.set("V", Object::Reference(signature_id));
        let field_id = writer.add_object(Object::Dictionary(field));

        // A second signature must extend /Annots, never replace the first
        // signature's widget; the array may be inline or an indirect object.
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
        // /Fields (PDF 32000-1 §12.7.2); register the field there too, with
        // SigFlags = SignaturesExist | AppendOnly.
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
    .map_err(other)?;

    let prepared =
        prepare_signature_bytes(with_field, DEFAULT_SIGNATURE_CAPACITY).map_err(other)?;
    let digest = digest_byte_ranges(&prepared.bytes, prepared.byte_range, algorithm.digest())
        .map_err(other)?;

    let identities = source.list_identities();
    let identity = identities
        .first()
        .ok_or_else(|| other("fixture PFX must expose exactly one identity"))?;
    let cms = CmsSignedDataBuilder::new(source, identity, &digest, algorithm.signing())
        .build()
        .map_err(other)?;

    append_signature_bytes(prepared.bytes, prepared.byte_range, &cms).map_err(other)
}

/// Generates the signed corpus into `out_dir`, creating it if needed.
/// Returns the written paths in [`SIGNED_CORPUS`] order.
pub fn generate_signed_corpus(out_dir: &Path) -> io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out_dir)?;

    let mut written = Vec::with_capacity(SIGNED_CORPUS.len());
    for spec in SIGNED_CORPUS {
        let pfx = self_signed_pfx(spec.algorithm, "Vitela fixture signer")?;
        let source =
            PfxCertificateSource::from_pkcs12(&pfx, FIXTURE_PFX_PASSWORD).map_err(other)?;

        let doc = crate::build_base_document(&format!("Signed fixture: {}", spec.file_name));
        let mut bytes = Vec::new();
        let mut doc = doc;
        doc.save_to(&mut bytes).map_err(other)?;

        for index in 0..spec.signatures {
            bytes = append_signed_revision(
                bytes,
                &source,
                spec.algorithm,
                &format!("Signature_{}", index + 1),
            )?;
        }

        let out_path = out_dir.join(spec.file_name);
        std::fs::write(&out_path, &bytes)?;
        written.push(out_path);
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_file_names_are_unique() {
        let mut names: Vec<&str> = SIGNED_CORPUS.iter().map(|spec| spec.file_name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SIGNED_CORPUS.len());
    }

    #[test]
    fn every_algorithm_maps_digest_and_signing_consistently() {
        for spec in SIGNED_CORPUS {
            assert_eq!(
                spec.algorithm.signing().digest_algorithm(),
                spec.algorithm.digest()
            );
        }
    }
}
