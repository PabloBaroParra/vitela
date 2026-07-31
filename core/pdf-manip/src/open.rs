//! Decrypt-on-open (T-025/T-026): opens a PDF, returning a decrypted
//! `LopdfDocument` plus the `SecurityContext` used, or a clean error for a
//! missing/wrong password (spec "Open Password-Protected PDF").
//!
//! [`read_security_context`] exposes the security half on its own, for a
//! caller that holds the document open through another backend and needs the
//! permissions but not the model — see [`crate::text_extraction_is_allowed`].

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
    let security = security_from_encrypt_dict(&probe, password)?;
    let doc = load_decrypted(path, password)?;

    Ok((LopdfDocument(doc), Some(security)))
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

    let (handler, permissions) = encryption_settings(&probe)?;
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

/// Reads a document's [`SecurityContext`] without decrypting it — the probe
/// half of [`open_document`], on its own.
///
/// This exists for a caller that already has the document open through
/// another backend and needs nothing from lopdf but the security policy: the
/// GTK4 shell renders through pdfium and never builds an lopdf model, yet
/// still has to honour the text-extraction permission (see
/// [`crate::text_extraction_is_allowed`]). It skips the second, fully
/// decrypting load that [`open_document`] performs, so an encrypted document
/// costs only the cheap unauthenticated probe.
///
/// Unlike [`open_document`], a `None` credential is not an error: an
/// encrypted document is probed with the empty user password, which is
/// exactly how the common "opens without any prompt, but forbids copying"
/// document authenticates. A document that genuinely needs a password still
/// reports [`ManipError::WrongPassword`] — pass the same password the
/// document was opened with.
pub fn read_security_context(
    path: &Path,
    credential: Option<&str>,
) -> Result<Option<SecurityContext>, ManipError> {
    let probe = LopdfRawDocument::load(path)?;
    security_from_probe(&probe, credential)
}

/// Bytes-based equivalent of [`read_security_context`] — for a shell holding
/// a byte buffer (Android/iOS, or the GTK4 shell's compiled-in samples)
/// rather than a path.
pub fn read_security_context_from_bytes(
    bytes: &[u8],
    credential: Option<&str>,
) -> Result<Option<SecurityContext>, ManipError> {
    let probe = LopdfRawDocument::load_mem(bytes)?;
    security_from_probe(&probe, credential)
}

/// Resolves a loaded probe's security state, whichever of lopdf's two shapes
/// it arrived in.
///
/// A probe of an encrypted document normally still carries its `/Encrypt`
/// dictionary, because the unauthenticated load stops before decrypting
/// anything. There is one exception: when the document's **user password is
/// empty**, that load already succeeds, so lopdf decrypts the whole document
/// in place, records the derived state in `Document::encryption_state`, and
/// *removes* `/Encrypt` from the trailer (see `reader.rs`'s
/// `load_encrypted_document`). Such a document then looks unencrypted to
/// `is_encrypted()` while still carrying real permissions — and "opens
/// without a prompt, but forbids copying" is the single most common shape of
/// restricted PDF in the wild, so the gate reads the recovered state instead
/// of waving it through.
fn security_from_probe(
    probe: &LopdfRawDocument,
    credential: Option<&str>,
) -> Result<Option<SecurityContext>, ManipError> {
    if probe.is_encrypted() {
        return security_from_encrypt_dict(probe, credential.unwrap_or("")).map(Some);
    }

    probe
        .encryption_state
        .as_ref()
        .map(security_from_unlocked_state)
        .transpose()
}

/// Builds the context for a document lopdf already unlocked with the empty
/// user password (see [`security_from_probe`]): its `/Encrypt` dictionary is
/// gone, but the decoded state carries the same handler and `/P` bits.
///
/// The credential is `User` because that is what an open with no password
/// supplied *is* — the same access level pdfium grants such a document. An
/// owner-password holder who wants the owner's bypass has to present it, in
/// which case the document reaches [`security_from_encrypt_dict`] instead.
fn security_from_unlocked_state(
    state: &lopdf::EncryptionState,
) -> Result<SecurityContext, ManipError> {
    let crypt_filter_method = state
        .crypt_filters()
        .get(state.default_string_filter())
        .map(|filter| filter.method().to_vec());

    Ok(SecurityContext {
        handler: security_handler(state.version(), crypt_filter_method.as_deref())?,
        credential: Credential::User,
        credentials: EncryptionCredentials::user(""),
        permissions: Permissions(state.permissions().bits() as u32),
    })
}

/// Reads an already-loaded, still-encrypted probe's `/Encrypt` dictionary and
/// resolves which credential role `password` authenticates.
///
/// Every open path and [`read_security_context`] share this, so the mapping
/// from a raw `/Encrypt` dictionary to a `SecurityContext` exists exactly
/// once. The caller must have established that `probe` is encrypted.
fn security_from_encrypt_dict(
    probe: &LopdfRawDocument,
    password: &str,
) -> Result<SecurityContext, ManipError> {
    let (handler, permissions) = encryption_settings(probe)?;

    let credential = if probe.authenticate_owner_password(password).is_ok() {
        Credential::Owner
    } else if probe.authenticate_user_password(password).is_ok() {
        Credential::User
    } else {
        return Err(ManipError::WrongPassword);
    };

    Ok(SecurityContext {
        handler,
        credential,
        credentials: match credential {
            Credential::User => EncryptionCredentials::user(password),
            Credential::Owner => EncryptionCredentials::owner(password),
        },
        permissions,
    })
}

/// Reads the handler and raw permission bitmask out of an encrypted probe's
/// `/Encrypt` dictionary.
fn encryption_settings(
    probe: &LopdfRawDocument,
) -> Result<(SecurityHandler, Permissions), ManipError> {
    let encrypt_dict = probe
        .get_encrypted()
        .map_err(|_| ManipError::UnsupportedSecurityHandler)?;
    Ok((
        security_handler_from_dict(encrypt_dict)?,
        permissions_from_dict(encrypt_dict),
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
    let security = security_from_encrypt_dict(&probe, password)?;
    let doc = load_decrypted_from_bytes(bytes, password)?;

    Ok((LopdfDocument(doc), Some(security)))
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

    let (handler, permissions) = encryption_settings(&probe)?;
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

    let crypt_filter_method = encrypt
        .get(b"CF")
        .and_then(|o| o.as_dict())
        .and_then(|cf| cf.get(b"StdCF"))
        .and_then(|o| o.as_dict())
        .and_then(|std_cf| std_cf.get(b"CFM"))
        .and_then(|o| o.as_name())
        .ok();

    security_handler(version, crypt_filter_method)
}

/// Maps an `/Encrypt` `/V` (plus, for `/V 4`, its crypt filter's `/CFM`) to a
/// [`SecurityHandler`]. Shared by the dictionary reader above and the
/// already-unlocked path, which recovers the same two facts from lopdf's
/// decoded `EncryptionState`.
fn security_handler(
    version: i64,
    crypt_filter_method: Option<&[u8]>,
) -> Result<SecurityHandler, ManipError> {
    match version {
        1 | 2 => Ok(SecurityHandler::Rc4_128),
        4 => match crypt_filter_method {
            Some(b"AESV2") => Ok(SecurityHandler::Aes128),
            Some(b"V2") => Ok(SecurityHandler::Rc4_128),
            _ => Err(ManipError::UnsupportedSecurityHandler),
        },
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
