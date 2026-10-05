//! Signing: a digital signature added to the open document, across the
//! UniFFI boundary.
//!
//! The FFI twin of the Linux shell's `.pfx` flow (`apps/linux-gtk/src/app/
//! sign`, `write::sign`). That shell links `pdf_sign` and
//! `pdf_sign_pfx` directly; a shell on the other side of this boundary
//! cannot hold a `CertificateSourcePort`, so the unlocked certificate
//! crosses as an opaque [`SigningCertificate`] and only its identities'
//! names come back out. Private keys stay in their source adapters.
//!
//! ## What is signed
//!
//! [`sign_to_bytes`] signs the document as it would be saved now (see
//! `DocumentState::signing_input`) and returns the signed bytes; the open
//! session is untouched. The signature is invisible and sits on the first
//! page, exactly as `pdf_sign::sign_document` places it for the Linux shell
//! (batch decision 4 in `docs/batch-digital-signature.md`).
//!
//! PKCS#12 and PKCS#11 sources retain keys in their adapters. Native system
//! stores provide public identities and a digest-signing callback; no private
//! key is exported to Rust or to another shell.

use std::collections::HashSet;
use std::sync::Arc;

use pdf_sign::{CertificateSourcePort, SignError};
use pdf_sign_pfx::PfxCertificateSource;

use crate::document::DocumentHandle;
use crate::error::FfiError;

/// One identity a [`SigningCertificate`] can sign as — what a shell lists
/// for the user to pick from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FfiSigningIdentity {
    /// Passed back to [`sign_to_bytes`] to sign as this identity.
    pub id: String,
    /// The label to show: the certificate's subject.
    pub display_name: String,
}

/// A signing source, retaining its adapter for as long as the shell keeps it.
/// Opaque on purpose: the shell reads which
/// identities it holds and hands it back to [`sign_to_bytes`], nothing else.
#[derive(uniffi::Object)]
pub struct SigningCertificate {
    source: Box<dyn CertificateSourcePort>,
}

impl std::fmt::Debug for SigningCertificate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningCertificate").finish_non_exhaustive()
    }
}

impl SigningCertificate {
    pub(crate) fn from_source(source: Box<dyn CertificateSourcePort>) -> Arc<Self> {
        Arc::new(Self { source })
    }
}

#[uniffi::export]
impl SigningCertificate {
    /// Every identity in the source that can sign — a certificate paired with
    /// its private key. Empty for a file that holds only certificates.
    pub fn identities(&self) -> Vec<FfiSigningIdentity> {
        self.source
            .list_identities()
            .into_iter()
            .map(|identity| FfiSigningIdentity {
                id: identity.id,
                display_name: identity.display_name,
            })
            .collect()
    }
}

/// Unlocks the `.pfx`/`.p12` file in `bytes` with `password`.
///
/// # Errors
///
/// [`FfiError::WrongPassword`] when the file does not open with `password`
/// — PKCS#12 cannot tell a wrong password from a damaged or non-PKCS#12 file
/// (both fail the same integrity check), so a shell should word its prompt
/// to cover both.
#[uniffi::export]
pub fn open_signing_certificate(
    bytes: Vec<u8>,
    password: String,
) -> Result<Arc<SigningCertificate>, FfiError> {
    PfxCertificateSource::from_pkcs12(&bytes, &password)
        .map(|source| SigningCertificate::from_source(Box::new(source)))
        .map_err(|_| FfiError::WrongPassword)
}

/// Why `handle` cannot be signed, as a lower-case clause, or `None` when it
/// can. Cheap: it reads the protection and the page count.
///
/// A signature is a new `/FT /Sig` field and its widget, structurally the
/// same change as placing a form field — so the gate is the form-authoring
/// one (batch decision 5), the annotate and modify-contents bits together.
#[uniffi::export]
pub fn signing_refusal(handle: &DocumentHandle) -> Option<String> {
    if !handle.form_field_editing_allowed() {
        return Some("this document does not permit adding a signature".to_string());
    }
    if handle.page_count() == 0 {
        return Some("the document has no pages to sign".to_string());
    }
    None
}

/// Signs the document as `identity_id` from `certificate` and returns the
/// signed PDF. The open session is untouched: a shell writes the bytes and
/// reopens them, as after protecting a copy.
///
/// # Errors
///
/// - [`FfiError::UnsupportedOperation`] when [`signing_refusal`] refuses, or
///   when the identity cannot sign (see [`sign_error`] for the clauses).
/// - [`FfiError::SignaturesWouldBeInvalidated`] when the pending edits could
///   only be saved by breaking a signature the file already carries.
#[uniffi::export]
pub fn sign_to_bytes(
    handle: &DocumentHandle,
    certificate: Arc<SigningCertificate>,
    identity_id: String,
) -> Result<Vec<u8>, FfiError> {
    if let Some(detail) = signing_refusal(handle) {
        return Err(FfiError::UnsupportedOperation { detail });
    }
    let (bytes, password, field_name) = {
        let state = handle.lock();
        let (bytes, password) = state.signing_input()?;
        (bytes, password, next_signature_field_name(state.base()))
    };
    pdf_sign::sign_document(
        bytes,
        password.as_deref(),
        1,
        field_name,
        certificate.source.as_ref(),
        &identity_id,
    )
    .map_err(sign_error)
}

/// `Signature_1`, `Signature_2`, … — the first such name no field in `base`
/// already has, so signing a signed document does not repeat a `/T` in
/// `/AcroForm /Fields`. Starts counting past the signature fields there are,
/// as the Linux shell does, and then skips any name that is still taken.
fn next_signature_field_name(base: &pdf_manip::LopdfDocument) -> String {
    let document = base.as_lopdf();
    let mut signatures = 0;
    let mut taken = HashSet::new();
    for dictionary in document.objects.values().filter_map(|o| o.as_dict().ok()) {
        if let Ok(name) = dictionary.get(b"T").and_then(|t| t.as_str()) {
            taken.insert(String::from_utf8_lossy(name).into_owned());
        }
        if dictionary.get(b"FT").and_then(|ft| ft.as_name()).ok() == Some(b"Sig".as_slice()) {
            signatures += 1;
        }
    }
    (signatures + 1..)
        .map(|n| format!("Signature_{n}"))
        .find(|name| !taken.contains(name))
        .expect("an unbounded range always finds an unused name")
}

/// Reopens signed output with the source session's credentials, without
/// returning passwords to the shell. Retains both roles when both were known.
///
/// # Errors
/// Returns the same opening errors as [`crate::open_from_bytes`].
#[uniffi::export]
pub fn reopen_signed_document(
    handle: &DocumentHandle,
    bytes: Vec<u8>,
) -> Result<Arc<DocumentHandle>, FfiError> {
    let (password, credentials) = {
        let state = handle.lock();
        (
            state.render_password().map(str::to_owned),
            state
                .document()
                .security
                .as_ref()
                .map(|s| s.credentials.clone()),
        )
    };
    if let Some((user, owner)) = credentials.as_ref().and_then(|c| c.complete()) {
        crate::open_with_passwords_from_bytes(bytes, user.to_owned(), owner.to_owned())
    } else {
        crate::open_from_bytes(bytes, password)
    }
}

/// A signing failure as the boundary's error: the clause a reader can act
/// on where there is one, and the core's own words otherwise.
fn sign_error(error: SignError) -> FfiError {
    let detail = match error {
        SignError::IdentityUnavailable { .. } => {
            "that identity is not in the certificate file".to_string()
        }
        SignError::NoSupportedAlgorithm { .. }
        | SignError::UnsupportedAlgorithm { .. }
        | SignError::IncompatibleCertificateAlgorithm { .. } => {
            "this certificate's key type cannot be used to sign".to_string()
        }
        SignError::MissingCertificateChain { .. } | SignError::InvalidCertificate { .. } => {
            "the certificate in this file could not be read".to_string()
        }
        other => format!("the document could not be signed ({other})"),
    };
    FfiError::UnsupportedOperation { detail }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_fields(names: &[(&str, bool)]) -> pdf_manip::LopdfDocument {
        let mut document = lopdf::Document::with_version("1.7");
        for (name, is_signature) in names {
            let mut field = lopdf::Dictionary::new();
            field.set("T", lopdf::Object::string_literal(*name));
            if *is_signature {
                field.set("FT", lopdf::Object::Name(b"Sig".to_vec()));
            }
            document.add_object(field);
        }
        pdf_manip::LopdfDocument::from_lopdf(document)
    }

    #[test]
    fn the_first_signature_is_signature_1() {
        assert_eq!(next_signature_field_name(&with_fields(&[])), "Signature_1");
    }

    #[test]
    fn a_second_signature_counts_past_the_first() {
        let base = with_fields(&[("Signature_1", true)]);
        assert_eq!(next_signature_field_name(&base), "Signature_2");
    }

    #[test]
    fn a_name_another_field_already_has_is_skipped() {
        let base = with_fields(&[("Approval", true), ("Signature_2", false)]);
        assert_eq!(next_signature_field_name(&base), "Signature_3");
    }

    #[test]
    fn an_unknown_identity_names_the_certificate_file() {
        let error = sign_error(SignError::IdentityUnavailable {
            identity_id: "x".to_string(),
        });
        assert!(
            matches!(error, FfiError::UnsupportedOperation { detail } if detail.contains("certificate file"))
        );
    }
}
