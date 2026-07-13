//! Decrypt-on-open (T-025/T-026): opens a PDF, returning a decrypted
//! `LopdfDocument` plus the `SecurityContext` used, or a clean error for a
//! missing/wrong password (spec "Open Password-Protected PDF").

use std::path::Path;

use lopdf::{Dictionary, Document as LopdfRawDocument, LoadOptions};
use pdf_document::{
    Credential, EncryptionCredentials, Permissions, SecurityContext, SecurityHandler,
};

use crate::document::LopdfDocument;
use crate::error::ManipError;

/// Opens the PDF at `path`. If it is encrypted, `credential` must be `Some`
/// with either its user or owner password. The returned `SecurityContext`
/// records the authenticated role and the one known password. This is enough
/// for incremental saves; use [`open_document_with_passwords`] before an
/// encrypted full rewrite so both PDF password roles can be preserved.
///
/// Wrong passwords and missing-password-on-encrypted-document both return a
/// clean `Err`, never a panic and never a partial/garbled document (spec
/// "Open Password-Protected PDF" — wrong-password scenario).
///
/// Implementation note (lopdf 0.44 gotcha): `Document::load(path)` on an
/// *encrypted* file does NOT populate `objects` beyond the `/Encrypt`
/// dictionary itself — `load_encrypted_document` early-returns as soon as it
/// determines no password is available, before unpacking any raw object
/// bytes (see `reader.rs`). A later `doc.decrypt(password)` on that same
/// handle is therefore a no-op that still reports success, silently
/// producing a "decrypted" document with zero pages. The only way to get a
/// fully populated, decrypted `Document` is `load_with_options` with the
/// password supplied up front (as `gen-fixtures`' own tests already do), so
/// this function uses a cheap unauthenticated `load()` purely to read the
/// still-encrypted `/Encrypt` dictionary (handler/permissions) and probe
/// which credential kind matches, then performs the real, fully-decrypting
/// load via `load_with_options` for the document it actually returns.
pub fn open_document(
    path: &Path,
    credential: Option<&str>,
) -> Result<(LopdfDocument, Option<SecurityContext>), ManipError> {
    let probe = LopdfRawDocument::load(path)?;

    if !probe.is_encrypted() {
        return Ok((LopdfDocument(probe), None));
    }

    let password = credential.ok_or(ManipError::PasswordRequired)?;

    let encrypt_dict = probe
        .get_encrypted()
        .map_err(|_| ManipError::UnsupportedSecurityHandler)?
        .clone();
    let handler = security_handler_from_dict(&encrypt_dict)?;
    let permissions = permissions_from_dict(&encrypt_dict);

    let credential_kind = if probe.authenticate_owner_password(password).is_ok() {
        Credential::Owner
    } else if probe.authenticate_user_password(password).is_ok() {
        Credential::User
    } else {
        return Err(ManipError::WrongPassword);
    };

    let doc = load_decrypted(path, password)?;

    Ok((
        LopdfDocument(doc),
        Some(SecurityContext {
            handler,
            credential: credential_kind,
            credentials: match credential_kind {
                Credential::User => EncryptionCredentials::user(password),
                Credential::Owner => EncryptionCredentials::owner(password),
            },
            permissions,
        }),
    ))
}

/// Opens an encrypted PDF after independently verifying both its user and
/// owner passwords. The resulting context can safely take either writer path,
/// including a full rewrite that recreates the encryption dictionary.
///
/// An unencrypted document is opened normally and returns no security context.
/// Both passwords are checked before loading so a typo cannot become a silent
/// security-policy change at save time.
pub fn open_document_with_passwords(
    path: &Path,
    user_password: &str,
    owner_password: &str,
) -> Result<(LopdfDocument, Option<SecurityContext>), ManipError> {
    let probe = LopdfRawDocument::load(path)?;

    if !probe.is_encrypted() {
        return Ok((LopdfDocument(probe), None));
    }

    if probe.authenticate_user_password(user_password).is_err()
        || probe.authenticate_owner_password(owner_password).is_err()
    {
        return Err(ManipError::WrongPassword);
    }

    let encrypt_dict = probe
        .get_encrypted()
        .map_err(|_| ManipError::UnsupportedSecurityHandler)?
        .clone();
    let handler = security_handler_from_dict(&encrypt_dict)?;
    let permissions = permissions_from_dict(&encrypt_dict);
    let doc = load_decrypted(path, owner_password)?;

    Ok((
        LopdfDocument(doc),
        Some(SecurityContext {
            handler,
            credential: Credential::Owner,
            credentials: EncryptionCredentials::both(user_password, owner_password),
            permissions,
        }),
    ))
}

fn load_decrypted(path: &Path, password: &str) -> Result<LopdfRawDocument, ManipError> {
    LopdfRawDocument::load_with_options(path, LoadOptions::with_password(password)).map_err(|err| {
        match err {
            lopdf::Error::Decryption(_) | lopdf::Error::InvalidPassword => {
                ManipError::WrongPassword
            }
            other => ManipError::Lopdf(other),
        }
    })
}

/// Bytes-based equivalent of [`open_document`] — the **canonical**
/// cross-platform entrypoint (spec delta "FileAccessPort"): Android (Storage
/// Access Framework) and iOS (security-scoped bookmarks) never hand a shell a
/// plain filesystem path, only a byte stream. `open_document`'s path-based
/// contract remains as a convenience solely for the GTK4 shell, which bypasses
/// the FFI boundary and can read its own files with `std::fs` directly.
///
/// Same gotcha as the path-based loader (see `open_document`'s doc comment):
/// a cheap unauthenticated `load_mem` probes the still-encrypted `/Encrypt`
/// dictionary and which credential kind matches, then the real, fully
/// decrypting load goes through `load_mem_with_options` with the password
/// supplied up front.
pub fn open_document_from_bytes(
    bytes: &[u8],
    credential: Option<&str>,
) -> Result<(LopdfDocument, Option<SecurityContext>), ManipError> {
    let probe = LopdfRawDocument::load_mem(bytes)?;

    if !probe.is_encrypted() {
        return Ok((LopdfDocument(probe), None));
    }

    let password = credential.ok_or(ManipError::PasswordRequired)?;

    let encrypt_dict = probe
        .get_encrypted()
        .map_err(|_| ManipError::UnsupportedSecurityHandler)?
        .clone();
    let handler = security_handler_from_dict(&encrypt_dict)?;
    let permissions = permissions_from_dict(&encrypt_dict);

    let credential_kind = if probe.authenticate_owner_password(password).is_ok() {
        Credential::Owner
    } else if probe.authenticate_user_password(password).is_ok() {
        Credential::User
    } else {
        return Err(ManipError::WrongPassword);
    };

    let doc = load_decrypted_from_bytes(bytes, password)?;

    Ok((
        LopdfDocument(doc),
        Some(SecurityContext {
            handler,
            credential: credential_kind,
            credentials: match credential_kind {
                Credential::User => EncryptionCredentials::user(password),
                Credential::Owner => EncryptionCredentials::owner(password),
            },
            permissions,
        }),
    ))
}

/// Bytes-based equivalent of [`open_document_with_passwords`] — see that
/// function's doc comment; used before an encrypted full rewrite so both PDF
/// password roles can be preserved when the caller only has a byte buffer
/// (Android/iOS), not a path.
pub fn open_document_with_passwords_from_bytes(
    bytes: &[u8],
    user_password: &str,
    owner_password: &str,
) -> Result<(LopdfDocument, Option<SecurityContext>), ManipError> {
    let probe = LopdfRawDocument::load_mem(bytes)?;

    if !probe.is_encrypted() {
        return Ok((LopdfDocument(probe), None));
    }

    if probe.authenticate_user_password(user_password).is_err()
        || probe.authenticate_owner_password(owner_password).is_err()
    {
        return Err(ManipError::WrongPassword);
    }

    let encrypt_dict = probe
        .get_encrypted()
        .map_err(|_| ManipError::UnsupportedSecurityHandler)?
        .clone();
    let handler = security_handler_from_dict(&encrypt_dict)?;
    let permissions = permissions_from_dict(&encrypt_dict);
    let doc = load_decrypted_from_bytes(bytes, owner_password)?;

    Ok((
        LopdfDocument(doc),
        Some(SecurityContext {
            handler,
            credential: Credential::Owner,
            credentials: EncryptionCredentials::both(user_password, owner_password),
            permissions,
        }),
    ))
}

fn load_decrypted_from_bytes(bytes: &[u8], password: &str) -> Result<LopdfRawDocument, ManipError> {
    LopdfRawDocument::load_mem_with_options(bytes, LoadOptions::with_password(password)).map_err(
        |err| match err {
            lopdf::Error::Decryption(_) | lopdf::Error::InvalidPassword => {
                ManipError::WrongPassword
            }
            other => ManipError::Lopdf(other),
        },
    )
}

/// Maps the `/Encrypt` dictionary's `/V` (and, for `/V 4`, its crypt
/// filter's `/CFM`) to `pdf_document::SecurityHandler`. Only the two
/// handlers covered by Batch 0's encrypted-PDF corpus are recognized — RC4
/// (`/V` 1 or 2) and AES-128 (`/V 4`, `StdCF`/`AESV2`); anything else is
/// `UnsupportedSecurityHandler` until a future batch extends this (AES-256,
/// `/V 5`, is mapped for forward-compatibility with `SecurityHandler`'s
/// `#[non_exhaustive]` variant, though no fixture exercises it yet).
fn security_handler_from_dict(encrypt: &Dictionary) -> Result<SecurityHandler, ManipError> {
    let version = encrypt
        .get(b"V")
        .and_then(|o| o.as_i64())
        .map_err(|_| ManipError::UnsupportedSecurityHandler)?;

    match version {
        1 | 2 => Ok(SecurityHandler::Rc4_128),
        4 => {
            let cfm = encrypt
                .get(b"CF")
                .and_then(|o| o.as_dict())
                .and_then(|cf| cf.get(b"StdCF"))
                .and_then(|o| o.as_dict())
                .and_then(|std_cf| std_cf.get(b"CFM"))
                .and_then(|o| o.as_name())
                .map_err(|_| ManipError::UnsupportedSecurityHandler)?;

            match cfm {
                b"AESV2" => Ok(SecurityHandler::Aes128),
                b"V2" => Ok(SecurityHandler::Rc4_128),
                _ => Err(ManipError::UnsupportedSecurityHandler),
            }
        }
        5 => Ok(SecurityHandler::Aes256),
        _ => Err(ManipError::UnsupportedSecurityHandler),
    }
}

/// Reads the raw `/P` permission bitmask into `pdf_document::Permissions`
/// (bit semantics are owned by this crate per `security.rs`'s doc comment;
/// `pdf_document` stores the raw value only).
fn permissions_from_dict(encrypt: &Dictionary) -> Permissions {
    let raw = encrypt.get(b"P").and_then(|o| o.as_i64()).unwrap_or(0);
    Permissions((raw as i32) as u32)
}
