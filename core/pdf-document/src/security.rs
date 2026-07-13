//! Security context: captures the encryption handler and credentials used to
//! open a protected document (T-011).
//!
//! Pure data — no crypto is performed here. `pdf-manip`/`pdf-save` (Batch 4/6)
//! use this to decrypt on open and preserve its encryption policy on save, per
//! spec "Encrypted Document Save Behavior".

use std::fmt;

/// Standard PDF security handler used to encrypt the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SecurityHandler {
    Rc4_128,
    Aes128,
    Aes256,
}

/// The role of the password that authenticated the open operation.
///
/// Kept distinct because owner-password opens typically grant full
/// permissions regardless of the `Permissions` bitmask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credential {
    User,
    Owner,
}

/// Passwords available to safely reproduce a document's encryption settings.
///
/// PDF user and owner passwords are independent. A caller that supplies only
/// one can open and incrementally save a document, but cannot safely perform a
/// full rewrite because the other password cannot be recovered from the file.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct EncryptionCredentials {
    pub user_password: Option<String>,
    pub owner_password: Option<String>,
}

impl fmt::Debug for EncryptionCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncryptionCredentials")
            .field(
                "user_password",
                &self.user_password.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "owner_password",
                &self.owner_password.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl EncryptionCredentials {
    pub fn user(password: impl Into<String>) -> Self {
        Self {
            user_password: Some(password.into()),
            owner_password: None,
        }
    }

    pub fn owner(password: impl Into<String>) -> Self {
        Self {
            user_password: None,
            owner_password: Some(password.into()),
        }
    }

    pub fn both(user_password: impl Into<String>, owner_password: impl Into<String>) -> Self {
        Self {
            user_password: Some(user_password.into()),
            owner_password: Some(owner_password.into()),
        }
    }

    pub fn complete(&self) -> Option<(&str, &str)> {
        Some((
            self.user_password.as_deref()?,
            self.owner_password.as_deref()?,
        ))
    }
}

/// Permission bitmask as defined by the PDF spec's encryption dictionary
/// `/P` entry. Bit semantics are owned by the encryption layer (`pdf-manip`);
/// this crate stores the raw value only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Permissions(pub u32);

/// Captured on open for an encrypted document.
///
/// Incremental saves retain lopdf's original encryption state. A full rewrite
/// uses `credentials` only when both passwords are present; it otherwise fails
/// rather than silently changing the document's security semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityContext {
    pub handler: SecurityHandler,
    pub credential: Credential,
    pub credentials: EncryptionCredentials,
    pub permissions: Permissions,
}
