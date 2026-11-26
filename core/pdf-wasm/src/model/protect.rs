//! Protecting a copy of an open document with two passwords (B26 T-251).
//!
//! Every rule is the core's (`pdf_save::protect`, moved there in T-249 so
//! the web and `pdf-ffi` share it): both password roles are required and
//! must differ, the protection is AES-128 with every permission granted
//! (`GRANTED_PROTECTION_PERMISSIONS`), and it is written on a **clone**, so
//! the open document's own security never changes. The signature question
//! is the core's protection-specific one (`protection_will_invalidate_signatures`),
//! asked over the same `SaveSource` the protected save is built from —
//! pending edits included.
//!
//! The one gate the core leaves to its callers is the shells': Windows and
//! Android ask the document's modify-contents permission
//! (`content_editing_is_allowed`) before offering Protect, because writing a
//! new encryption dictionary is a change to the document. It lives here, in
//! Rust, so no TypeScript copy of it exists.
//!
//! Protecting is not an edit: it records no undo step and leaves the session
//! as it found it.

use pdf_save::SignatureAcknowledgement;

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// Why a document whose permissions withhold modification cannot be
/// protected — the Windows and Android sentence, as the core's lower-case
/// clause.
pub const PROTECTION_FORBIDDEN: &str = "this document does not permit protection changes";

impl DocumentModel {
    /// Why this document cannot be protected, or `None` when it can. Cheap:
    /// it reads the permissions, it saves nothing.
    pub fn protection_refusal(&self) -> Option<&'static str> {
        (!pdf_manip::content_editing_is_allowed(self.document.security.as_ref()))
            .then_some(PROTECTION_FORBIDDEN)
    }

    /// Whether the protected copy breaks a signature the file carries: when
    /// `true`, `protected_bytes` refuses with `SignaturesWouldBeInvalidated`
    /// unless told the user agreed.
    pub fn protection_will_invalidate_signatures(&self) -> Result<bool, EngineError> {
        Ok(self.with_save_source(pdf_save::protection_will_invalidate_signatures)?)
    }

    /// The document with every edit applied, written under new AES-128
    /// protection: `open_password` to open it, `permissions_password` to
    /// lift its permissions. The session is untouched.
    pub fn protected_bytes(
        &self,
        open_password: &str,
        permissions_password: &str,
        invalidate_signatures: bool,
    ) -> Result<Vec<u8>, EngineError> {
        if let Some(refusal) = self.protection_refusal() {
            return Err(EngineError::new(ErrorKind::EditNotPermitted, refusal));
        }
        let signatures = if invalidate_signatures {
            SignatureAcknowledgement::ProceedAndInvalidate
        } else {
            SignatureAcknowledgement::Unacknowledged
        };
        Ok(self.with_save_source(|source| {
            pdf_save::protected_bytes(source, open_password, permissions_password, signatures)
        })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{pdfs, PageEdit};
    use pdf_document::SecurityHandler;
    use pdf_manip::OpenCredentials;

    const OPEN: &str = "open-pw";
    const PERMISSIONS: &str = "permissions-pw";

    fn open(bytes: &[u8]) -> DocumentModel {
        DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
    }

    fn reopen(bytes: &[u8], password: &str) -> Result<DocumentModel, EngineError> {
        DocumentModel::open(bytes, &OpenCredentials::Password(password.into()))
    }

    #[test]
    fn the_copy_needs_the_open_password_and_the_permissions_password_opens_it_too() {
        let model = open(&pdfs::text_document(&["one", "two"]));
        assert_eq!(model.protection_refusal(), None);
        assert!(!model.protection_will_invalidate_signatures().unwrap());

        let bytes = model.protected_bytes(OPEN, PERMISSIONS, false).unwrap();

        let err = DocumentModel::open(&bytes, &OpenCredentials::NoPassword)
            .err()
            .unwrap();
        assert_eq!(err.kind, ErrorKind::PasswordRequired);
        assert_eq!(
            reopen(&bytes, "nope").err().unwrap().kind,
            ErrorKind::WrongPassword
        );
        let by_open = reopen(&bytes, OPEN).expect("the open password opens it");
        assert_eq!(by_open.page_count(), 2);
        let security = by_open.document.security.as_ref().expect("encrypted");
        assert_eq!(security.handler, SecurityHandler::Aes128);
        // Every permission is granted: the passwords are the access control.
        assert!(by_open.text_extraction_allowed());
        assert!(by_open.document_assembly_allowed());

        let by_permissions =
            reopen(&bytes, PERMISSIONS).expect("the permissions password opens it");
        assert_eq!(by_permissions.page_count(), 2);
        // Both together are the reopen the native shells do.
        DocumentModel::open(
            &bytes,
            &OpenCredentials::UserAndOwner {
                user: OPEN.into(),
                owner: PERMISSIONS.into(),
            },
        )
        .expect("both passwords open it");
    }

    #[test]
    fn the_session_stays_unprotected_with_its_edits_and_history() {
        let mut model = open(&pdfs::text_document(&["one", "two", "three"]));
        model
            .apply_edit(PageEdit::Remove { index: 0, count: 1 })
            .unwrap();

        let bytes = model.protected_bytes(OPEN, PERMISSIONS, false).unwrap();
        assert_eq!(reopen(&bytes, OPEN).unwrap().page_count(), 2);
        assert!(model.document.security.is_none());
        assert!(model.can_undo());
        assert!(!model.can_redo());
        assert!(model.undo());
        assert_eq!(model.page_count(), 3);
    }

    #[test]
    fn missing_or_equal_passwords_are_refused_in_the_cores_words() {
        let model = open(&pdfs::text_document(&["one"]));
        for (open_pw, permissions_pw) in [("", PERMISSIONS), (OPEN, ""), ("", "")] {
            let err = model
                .protected_bytes(open_pw, permissions_pw, false)
                .unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidPasswords);
            assert_eq!(
                err.message,
                "both open and permissions passwords are required"
            );
        }
        let err = model.protected_bytes(OPEN, OPEN, false).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidPasswords);
        assert_eq!(
            err.message,
            "the open and permissions passwords must be different"
        );
    }

    #[test]
    fn a_signed_document_needs_the_users_yes() {
        let model = open(pdfs::SIGNED);
        assert_eq!(model.protection_refusal(), None);
        assert!(model.protection_will_invalidate_signatures().unwrap());

        let err = model.protected_bytes(OPEN, PERMISSIONS, false).unwrap_err();
        assert_eq!(err.kind, ErrorKind::SignaturesWouldBeInvalidated);

        let bytes = model
            .protected_bytes(OPEN, PERMISSIONS, true)
            .expect("acknowledged");
        assert_eq!(reopen(&bytes, OPEN).unwrap().page_count(), 1);
    }

    #[test]
    fn a_document_that_withholds_modification_is_refused_ahead() {
        let model = reopen(&pdfs::restricted_document(), pdfs::USER_PASSWORD).unwrap();
        assert_eq!(model.protection_refusal(), Some(PROTECTION_FORBIDDEN));

        let err = model.protected_bytes(OPEN, PERMISSIONS, false).unwrap_err();
        assert_eq!(err.kind, ErrorKind::EditNotPermitted);
        assert_eq!(err.message, PROTECTION_FORBIDDEN);
    }

    #[test]
    fn its_owner_may_change_the_passwords_of_a_protected_document() {
        let model = DocumentModel::open(
            &pdfs::restricted_document(),
            &OpenCredentials::UserAndOwner {
                user: pdfs::USER_PASSWORD.into(),
                owner: pdfs::OWNER_PASSWORD.into(),
            },
        )
        .unwrap();
        assert_eq!(model.protection_refusal(), None);

        let bytes = model.protected_bytes(OPEN, PERMISSIONS, false).unwrap();
        assert_eq!(
            reopen(&bytes, pdfs::USER_PASSWORD).err().unwrap().kind,
            ErrorKind::WrongPassword
        );
        let reopened = reopen(&bytes, OPEN).unwrap();
        // The new protection grants what the old one withheld.
        assert!(reopened.text_extraction_allowed());
    }

    #[test]
    fn a_document_granting_modification_is_reprotected_from_its_open_password() {
        // The Windows and Android gate: the modify-contents permission, which
        // this document grants to whoever opens it.
        let model = reopen(&pdfs::locked_document(&["one"]), pdfs::USER_PASSWORD).unwrap();
        assert_eq!(model.protection_refusal(), None);

        let bytes = model.protected_bytes(OPEN, PERMISSIONS, false).unwrap();
        assert_eq!(reopen(&bytes, OPEN).unwrap().page_count(), 1);
    }
}
