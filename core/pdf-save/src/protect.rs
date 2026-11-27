//! Protect (T-249): applying new AES-128 password protection to a snapshot
//! of a document and returning the protected bytes.
//!
//! Moved here from `pdf-ffi` so every boundary — UniFFI today, wasm next —
//! applies the same password rules and builds the same security context
//! instead of each carrying its own copy.
//!
//! The live model is never mutated: the security context is set on a
//! **clone**, so a caller may cancel or fail while writing the destination
//! without changing the open document's security state.

use std::fmt;

use pdf_document::{
    Credential, Document, EncryptionCredentials, Permissions, SecurityContext, SecurityHandler,
};

use crate::error::SaveError;
use crate::security::SaveIntent;
use crate::strategy::{self, SaveSource, SignatureAcknowledgement};

/// The permission mask used when a shell applies password protection without
/// offering a restrictions editor. The passwords control access; every
/// document permission remains granted.
pub const GRANTED_PROTECTION_PERMISSIONS: u32 = 0xFFFF_FFFC;

/// Why [`protected_bytes`] produced no file.
#[derive(Debug)]
#[non_exhaustive]
pub enum ProtectError {
    /// One of the two password roles was left empty.
    MissingPassword,
    /// Both password roles were given the same value. That would let everyone
    /// who can open the file also bypass its permission policy, which is not
    /// an equivalent simplification.
    SamePasswords,
    /// The protected save itself failed — including a refusal to break a
    /// signature the caller has not acknowledged.
    Save(SaveError),
}

impl fmt::Display for ProtectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPassword => {
                f.write_str("both open and permissions passwords are required")
            }
            Self::SamePasswords => {
                f.write_str("the open and permissions passwords must be different")
            }
            Self::Save(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ProtectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Save(error) => Some(error),
            Self::MissingPassword | Self::SamePasswords => None,
        }
    }
}

impl From<SaveError> for ProtectError {
    fn from(error: SaveError) -> Self {
        Self::Save(error)
    }
}

/// Applies new AES-128 protection to a snapshot of `source.document` and
/// returns the protected PDF bytes.
///
/// Both PDF password roles are required and must differ — checked before
/// anything is cloned or saved.
///
/// # Errors
///
/// [`ProtectError::MissingPassword`], [`ProtectError::SamePasswords`], or
/// [`ProtectError::Save`] wrapping whatever the save refused with.
pub fn protected_bytes(
    source: SaveSource<'_>,
    open_password: &str,
    permissions_password: &str,
    signatures: SignatureAcknowledgement,
) -> Result<Vec<u8>, ProtectError> {
    if open_password.is_empty() || permissions_password.is_empty() {
        return Err(ProtectError::MissingPassword);
    }
    if open_password == permissions_password {
        return Err(ProtectError::SamePasswords);
    }

    let document = protected_clone(source.document, open_password, permissions_password);
    strategy::save_document(source.input_for(&document, SaveIntent::ApplyProtection, signatures))
        .map_err(Into::into)
}

/// Whether applying new password protection to `source.document` would
/// invalidate a signature — asked with placeholder passwords, since the
/// answer depends on the save's shape and not on which passwords it uses.
pub fn protection_will_invalidate_signatures(source: SaveSource<'_>) -> Result<bool, SaveError> {
    let document = protected_clone(source.document, "probe-open", "probe-permissions");
    strategy::will_invalidate_signatures(source.input_for(
        &document,
        SaveIntent::ApplyProtection,
        SignatureAcknowledgement::Unacknowledged,
    ))
}

/// A clone of `document` carrying the AES-128 security context a protected
/// save writes.
fn protected_clone(
    document: &Document,
    open_password: &str,
    permissions_password: &str,
) -> Document {
    let mut document = document.clone();
    document.security = Some(SecurityContext {
        handler: SecurityHandler::Aes128,
        credential: Credential::Owner,
        credentials: EncryptionCredentials::both(open_password, permissions_password),
        permissions: Permissions(GRANTED_PROTECTION_PERMISSIONS),
    });
    document
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{document_from_lopdf, ImportedSources};
    use pdf_manip::LopdfDocument;

    struct Fixture {
        document: Document,
        base: LopdfDocument,
        bytes: Vec<u8>,
    }

    impl Fixture {
        fn source(&self) -> SaveSource<'_> {
            SaveSource {
                document: &self.document,
                base: &self.base,
                original_bytes: Some(&self.bytes),
                imported_sources: ImportedSources::none(),
            }
        }
    }

    fn fixture() -> Fixture {
        let mut doc = gen_fixtures::build_base_document("Protected by pdf-save");
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save fixture");
        let (base, security) =
            pdf_manip::open_document_from_bytes(&bytes, None).expect("open fixture");
        let document = document_from_lopdf(&base, security).expect("model");
        Fixture {
            document,
            base,
            bytes,
        }
    }

    #[test]
    fn protected_bytes_require_the_new_open_password() {
        let fixture = fixture();

        let protected = protected_bytes(
            fixture.source(),
            "open-pw",
            "permissions-pw",
            SignatureAcknowledgement::Unacknowledged,
        )
        .expect("protection should be applied");

        assert!(pdf_manip::open_document_from_bytes(&protected, None).is_err());
        let (_, security) = pdf_manip::open_document_from_bytes(&protected, Some("open-pw"))
            .expect("the open password opens it");
        let security = security.expect("the output is encrypted");
        assert_eq!(security.handler, SecurityHandler::Aes128);
    }

    #[test]
    fn protecting_leaves_the_source_model_unprotected() {
        let fixture = fixture();

        protected_bytes(
            fixture.source(),
            "open-pw",
            "permissions-pw",
            SignatureAcknowledgement::Unacknowledged,
        )
        .expect("protection should be applied");

        assert!(fixture.document.security.is_none());
    }

    #[test]
    fn an_empty_password_role_is_refused() {
        let fixture = fixture();

        for (open, permissions) in [("", "permissions-pw"), ("open-pw", ""), ("", "")] {
            let result = protected_bytes(
                fixture.source(),
                open,
                permissions,
                SignatureAcknowledgement::Unacknowledged,
            );
            assert!(matches!(result, Err(ProtectError::MissingPassword)));
        }
    }

    #[test]
    fn equal_password_roles_are_refused() {
        let fixture = fixture();

        let result = protected_bytes(
            fixture.source(),
            "same-pw",
            "same-pw",
            SignatureAcknowledgement::Unacknowledged,
        );

        assert!(matches!(result, Err(ProtectError::SamePasswords)));
    }

    /// The FFI boundary forwards these strings to shells verbatim, so they
    /// are part of its contract.
    #[test]
    fn refusals_keep_their_boundary_messages() {
        assert_eq!(
            ProtectError::MissingPassword.to_string(),
            "both open and permissions passwords are required"
        );
        assert_eq!(
            ProtectError::SamePasswords.to_string(),
            "the open and permissions passwords must be different"
        );
    }

    #[test]
    fn an_unsigned_document_reports_no_signature_to_invalidate() {
        let fixture = fixture();

        assert!(!protection_will_invalidate_signatures(fixture.source()).unwrap());
    }
}
