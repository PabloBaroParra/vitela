//! Encrypted-save behavior (T-034, T-035): incremental saves retain lopdf's
//! encryption state, while full rewrites require both PDF passwords
//! before re-encrypting. Explicit strip protection never touches `EditLog`.
//! See spec.md "Encrypted Document Save Behavior".
//!
//! The **incremental** writer needs none of this module's functions: lopdf's
//! `IncrementalDocument::save_internal` already re-encrypts every appended
//! object automatically whenever `prev_documents.encryption_state` is
//! `Some(..)` (true whenever the base document was opened via
//! `load_with_options(path, LoadOptions::with_password(pw))`, which
//! `pdf_manip::open_document` already does) — see that function's doc
//! comment in the vendored lopdf source for the re-encrypt-per-object loop.
//! This module exists for the **full-rewrite** writer, which starts from a
//! freshly reconciled `lopdf::Document` and must explicitly re-apply
//! encryption before serializing.
//!
//! [`SaveIntent::StripProtection`] can only be honored by the full-rewrite
//! writer: an incremental append cannot retroactively decrypt bytes already
//! written in a prior encrypted revision, so `crate::strategy` forces
//! full-rewrite whenever this intent is requested, regardless of whether any
//! structural page change is present.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::{EncryptionState, EncryptionVersion, Permissions as LopdfPermissions};
use pdf_document::{SecurityContext, SecurityHandler};

use crate::rewrite::{full_rewrite_blocker, RewriteBlocker};

use crate::error::SaveError;

/// Caller's intent for how a save should treat an existing `SecurityContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SaveIntent {
    /// Re-encrypt with the same handler/credentials used to open (spec
    /// default — never a silent strip).
    #[default]
    Default,
    /// Explicit, user-consented removal of protection. Callers MUST record
    /// `AuditEvent::StripProtectionConsent` on `Document.audit_log`
    /// themselves *before* calling save with this intent — pdf-save's role
    /// is only to honor the intent on the encoding side; it never touches
    /// `EditLog` or the audit log itself (spec "Strip is not undoable").
    StripProtection,
}

/// Builds a lopdf `EncryptionState` matching `security`'s handler, for
/// re-encrypting a freshly-rewritten `lopdf::Document` (full-rewrite path).
///
/// A PDF's original owner password cannot be derived from a user password (or
/// vice versa). Refuse a full rewrite unless both passwords were
/// supplied at open, rather than silently assigning one password to both roles.
/// The refusable conditions live in [`full_rewrite_blocker`], which callers
/// consult before editing; this function reads the same answer so an edit
/// allowed up front cannot be refused down here.
pub fn build_encryption_state(
    document: &lopdf::Document,
    security: &SecurityContext,
) -> Result<EncryptionState, SaveError> {
    if let Some(blocker) = full_rewrite_blocker(Some(security)) {
        return Err(SaveError::InvalidSaveRequest(blocker.reason()));
    }
    let (user_password, owner_password) = security
        .credentials
        .complete()
        .expect("full_rewrite_blocker reported no blocker, so both passwords are present");
    let permissions = LopdfPermissions::from_bits_retain(u64::from(security.permissions.0));

    let version = match security.handler {
        SecurityHandler::Rc4_128 => EncryptionVersion::V2 {
            document,
            owner_password,
            user_password,
            key_length: 128,
            permissions,
        },
        SecurityHandler::Aes128 => {
            let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
            EncryptionVersion::V4 {
                document,
                encrypt_metadata: true,
                crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password,
                user_password,
                permissions,
            }
        }
        // Unreachable in practice — `full_rewrite_blocker` above already
        // refused every handler this match cannot build. Kept, and answering
        // with the same words, because `SecurityHandler` is `#[non_exhaustive]`:
        // a variant added upstream must fail here identically rather than
        // stop the crate compiling.
        SecurityHandler::Aes256 | _ => {
            return Err(SaveError::InvalidSaveRequest(
                RewriteBlocker::UnsupportedHandler.reason(),
            ));
        }
    };

    EncryptionState::try_from(version).map_err(SaveError::from)
}

/// Applies (or skips) encryption on `document` per `intent`/`security`,
/// ahead of a full-rewrite serialize.
///
/// - `StripProtection` never encrypts, regardless of `security`.
/// - `Default` with `security: Some(..)` re-encrypts only with both
///   passwords (see [`build_encryption_state`]).
/// - `Default` with `security: None` is a no-op — the document was never
///   encrypted, so there is nothing to re-apply.
pub fn apply_encryption_for_full_rewrite(
    document: &mut lopdf::Document,
    security: Option<&SecurityContext>,
    intent: SaveIntent,
) -> Result<(), SaveError> {
    if intent == SaveIntent::StripProtection {
        return Ok(());
    }
    let Some(security) = security else {
        return Ok(());
    };

    let state = build_encryption_state(document, security)?;
    document.encrypt(&state)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object};
    use pdf_document::{
        Credential, EncryptionCredentials, Permissions as DocPermissions, SecurityHandler,
    };

    fn base_doc() -> lopdf::Document {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Vec::<Object>::new(),
                "Count" => 0,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        // The standard security handler's key derivation needs a file /ID.
        let file_id = Object::string_literal("test-fixture-id");
        doc.trailer.set("ID", vec![file_id.clone(), file_id]);
        doc
    }

    fn security(handler: SecurityHandler) -> SecurityContext {
        SecurityContext {
            handler,
            credential: Credential::User,
            credentials: EncryptionCredentials::both("secret-user-pw", "secret-owner-pw"),
            permissions: DocPermissions(0xFFFF_FFFC_u32),
        }
    }

    #[test]
    fn strip_protection_never_encrypts_even_with_security_context() {
        let mut doc = base_doc();
        apply_encryption_for_full_rewrite(
            &mut doc,
            Some(&security(SecurityHandler::Aes128)),
            SaveIntent::StripProtection,
        )
        .expect("strip should succeed");
        assert!(!doc.is_encrypted());
    }

    #[test]
    fn default_intent_with_no_security_context_is_a_noop() {
        let mut doc = base_doc();
        apply_encryption_for_full_rewrite(&mut doc, None, SaveIntent::Default)
            .expect("no-op should succeed");
        assert!(!doc.is_encrypted());
    }

    #[test]
    fn default_intent_reencrypts_with_rc4() {
        let mut doc = base_doc();
        apply_encryption_for_full_rewrite(
            &mut doc,
            Some(&security(SecurityHandler::Rc4_128)),
            SaveIntent::Default,
        )
        .expect("re-encrypt should succeed");
        assert!(doc.is_encrypted());
    }

    #[test]
    fn default_intent_reencrypts_with_aes128_and_round_trips() {
        let mut doc = base_doc();
        apply_encryption_for_full_rewrite(
            &mut doc,
            Some(&security(SecurityHandler::Aes128)),
            SaveIntent::Default,
        )
        .expect("re-encrypt should succeed");
        assert!(doc.is_encrypted());

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save should succeed");

        let mut reloaded = lopdf::Document::load_mem(&bytes).expect("reload should succeed");
        assert!(reloaded.is_encrypted());
        reloaded
            .decrypt("secret-user-pw")
            .expect("re-encrypted doc must open with the same credential");
    }

    #[test]
    fn full_rewrite_rejects_a_context_with_only_one_password() {
        let doc = base_doc();
        let security = SecurityContext {
            handler: SecurityHandler::Aes128,
            credential: Credential::User,
            credentials: EncryptionCredentials::user("user-only"),
            permissions: DocPermissions(0xFFFF_FFFC_u32),
        };

        let result = build_encryption_state(&doc, &security);
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    #[test]
    fn aes256_is_rejected_as_not_yet_implemented() {
        let doc = base_doc();
        let result = build_encryption_state(&doc, &security(SecurityHandler::Aes256));
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    /// The predicate a caller asks *before* editing and the encoder that runs
    /// at save time must never disagree: an edit allowed by the first and
    /// refused by the second is exactly the silent trap this check exists to
    /// remove. Both read [`full_rewrite_blocker`], and this pins that they
    /// still report the same words.
    #[test]
    fn the_encoder_refuses_precisely_what_the_blocker_reports() {
        let doc = base_doc();
        let mut one_password = security(SecurityHandler::Rc4_128);
        one_password.credentials = EncryptionCredentials::user("user-only");

        for context in [one_password, security(SecurityHandler::Aes256)] {
            let blocker = full_rewrite_blocker(Some(&context)).expect("context must be blocked");
            let Err(error) = build_encryption_state(&doc, &context) else {
                panic!("a blocked context must not produce an encryption state");
            };

            assert!(
                matches!(error, SaveError::InvalidSaveRequest(reason) if reason == blocker.reason()),
                "encoder and blocker disagree for {context:?}: {error}"
            );
        }
    }

    /// Checklist §5 item 4: a credential must not travel into a message a
    /// user, a log or a bug report could see. The reasons are `&'static str`
    /// by type, so this pins the one thing that could still go wrong — a
    /// future reason built by formatting the context that carries them.
    #[test]
    fn a_blocked_rewrite_never_names_the_password_it_is_missing() {
        let doc = base_doc();
        let mut one_password = security(SecurityHandler::Rc4_128);
        one_password.credentials = EncryptionCredentials::user("hunter2-user-password");

        let Err(error) = build_encryption_state(&doc, &one_password) else {
            panic!("one password must not produce an encryption state");
        };

        assert!(!error.to_string().contains("hunter2"));
    }
}
