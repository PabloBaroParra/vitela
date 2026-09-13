//! Whether an encrypted document could survive a full rewrite at all —
//! asked *before* the first edit, not at the save that would fail.
//!
//! Split from [`crate::security`], which owns the encoding side: that module
//! builds the encryption state a rewrite writes, this one answers whether one
//! could be built. The two questions have different callers — the FFI
//! boundary and the GTK4 shell ask this one while the user is still choosing
//! what to do, and the save encoder asks it again at the last moment — and
//! both read the same function so they can never disagree (checklist
//! "Seguridad y firmas" item 5, `docs/batch-pdf-assembly.md` section 5).

use pdf_document::{SecurityContext, SecurityHandler};

/// Why an encrypted document cannot have its encryption reproduced by a full
/// rewrite (checklist "Seguridad y firmas", `docs/batch-pdf-assembly.md`
/// section 5).
///
/// This is a property of how the document was *opened*, not of what the user
/// is trying to do, so it can be — and must be — answered before the first
/// edit is recorded rather than at the save that would fail. Any structural
/// page change forces the full-rewrite writer (see
/// [`crate::has_structural_page_changes`]), so reordering, inserting,
/// removing or importing a page into a blocked document produces work that
/// can never be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RewriteBlocker {
    /// Only one of the two PDF password roles is known. A PDF's owner
    /// password cannot be derived from its user password or vice versa, so
    /// re-encrypting would have to assign one password to both roles —
    /// silently changing the document's security policy.
    IncompleteCredentials,
    /// The document's security handler has no re-encryption implementation
    /// in this build (see [`crate::security::build_encryption_state`]'s handler match).
    UnsupportedHandler,
}

impl RewriteBlocker {
    /// The refusal text, in the caller's own words. `&'static str` by type:
    /// a reason is never formatted from the [`SecurityContext`] it describes,
    /// which is what keeps a password out of every message derived from one
    /// (checklist section 5, "credentials out of the domain model, logs and
    /// error messages").
    pub fn reason(self) -> &'static str {
        match self {
            RewriteBlocker::IncompleteCredentials => {
                "encrypted full rewrite requires user and owner passwords; reopen with \
                 open_document_with_passwords or use an incremental save"
            }
            RewriteBlocker::UnsupportedHandler => {
                "only RC4-128 and AES-128 re-encryption are implemented in this batch — no \
                 fixture in the corpus exercises AES-256 or any future SecurityHandler variant \
                 (see pdf-manip's SecurityHandler mapping notes)"
            }
        }
    }
}

/// Whether a document opened with `security` could be fully rewritten with
/// its encryption intact — `None` when it could.
///
/// `None` security is an unencrypted document, which every writer can
/// reproduce. Asked ahead of an edit by [`crate::save_document`]'s callers
/// (the FFI boundary and the GTK4 shell) and at save time by
/// [`crate::security::build_encryption_state`], so the two can never disagree about which
/// documents are editable.
///
/// This deliberately says nothing about `SaveIntent::StripProtection`, which
/// needs no encryption state at all: stripping is an explicit, separately
/// consented operation, and a caller taking that path is not asking this
/// question.
pub fn full_rewrite_blocker(security: Option<&SecurityContext>) -> Option<RewriteBlocker> {
    let security = security?;
    if security.credentials.complete().is_none() {
        return Some(RewriteBlocker::IncompleteCredentials);
    }
    match security.handler {
        SecurityHandler::Rc4_128 | SecurityHandler::Aes128 => None,
        SecurityHandler::Aes256 | _ => Some(RewriteBlocker::UnsupportedHandler),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Credential, EncryptionCredentials, Permissions, SecurityHandler};

    fn security(handler: SecurityHandler) -> SecurityContext {
        SecurityContext {
            handler,
            credential: Credential::User,
            credentials: EncryptionCredentials::both("secret-user-pw", "secret-owner-pw"),
            permissions: Permissions(0xFFFF_FFFC_u32),
        }
    }

    #[test]
    fn an_unencrypted_document_can_always_be_rewritten() {
        assert_eq!(full_rewrite_blocker(None), None);
    }

    #[test]
    fn a_context_with_both_passwords_can_be_rewritten() {
        assert_eq!(
            full_rewrite_blocker(Some(&security(SecurityHandler::Rc4_128))),
            None
        );
    }

    #[test]
    fn a_context_with_only_one_password_blocks_a_rewrite() {
        let mut one_password = security(SecurityHandler::Rc4_128);
        one_password.credentials = EncryptionCredentials::user("user-only");

        assert_eq!(
            full_rewrite_blocker(Some(&one_password)),
            Some(RewriteBlocker::IncompleteCredentials)
        );
    }

    #[test]
    fn an_unimplemented_handler_blocks_a_rewrite_even_with_both_passwords() {
        assert_eq!(
            full_rewrite_blocker(Some(&security(SecurityHandler::Aes256))),
            Some(RewriteBlocker::UnsupportedHandler)
        );
    }
}
