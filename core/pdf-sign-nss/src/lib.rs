//! NSS shared certificate database implementation of
//! [`pdf_sign::CertificateSourcePort`] — the closest thing Linux has to
//! Windows' system certificate store: `~/.pki/nssdb`, the database Chrome and
//! most Firefox profiles read software (non-smart-card) certificates from.
//!
//! NSS's own PKCS#11 module (`libsoftokn3.so`) only knows which profile to
//! read once it is told, via a `configdir=...` string passed through
//! `C_Initialize`'s `pReserved` argument — a raw C string, not a typed
//! parameter. `pdf-sign-pkcs11` stays `#![forbid(unsafe_code)]` because it is
//! the boundary private-key bytes never cross; building that one pointer is
//! this crate's entire reason to exist, and its only `unsafe` code.
#![deny(unsafe_code)]

use std::ffi::{c_void, CString};
use std::path::Path;
use std::ptr::NonNull;

use pdf_sign_pkcs11::{
    CInitializeArgs, CInitializeFlags, Pkcs11AdapterError, Pkcs11CertificateSource,
};
use thiserror::Error;

/// Failure while loading the NSS shared certificate database.
#[derive(Debug, Error)]
pub enum NssAdapterError {
    /// `profile_dir` cannot be used inside NSS's single-quoted `configdir=`
    /// argument — either it is not valid UTF-8, or it contains a `'` that
    /// would terminate the quoted value early and corrupt the argument NSS
    /// parses.
    #[error("the certificate database path {0:?} cannot be passed to NSS")]
    InvalidProfilePath(std::path::PathBuf),
    /// The underlying PKCS#11 module failed to load or initialize.
    #[error(transparent)]
    Pkcs11(#[from] Pkcs11AdapterError),
}

/// Loads `module_path` (NSS's `libsoftokn3.so`) against the certificate
/// database at `profile_dir` (typically `~/.pki/nssdb`), returning a
/// [`Pkcs11CertificateSource`] — from here on it is an ordinary PKCS#11
/// source, listing and signing with identities exactly like a smart card
/// would. `user_pin` authenticates the database the same way a token PIN
/// does; it is commonly `None`/empty, since most `~/.pki/nssdb` databases
/// have no password set.
pub fn load(
    module_path: impl AsRef<Path>,
    profile_dir: impl AsRef<Path>,
    user_pin: Option<String>,
) -> Result<Pkcs11CertificateSource, NssAdapterError> {
    let init_string = build_init_string(profile_dir.as_ref())?;
    let init_args = nss_init_args(&init_string);
    // `init_string` outlives this call: PKCS#11's `pReserved` need only stay
    // valid for the duration of `C_Initialize` itself, which happens inside
    // `load_with_init_args`, before `init_string` is dropped at the end of
    // this function.
    Ok(Pkcs11CertificateSource::load_with_init_args(
        module_path,
        init_args,
        user_pin,
    )?)
}

/// Builds NSS's `configdir='sql:...'` init string. `flags=readOnly` because
/// this adapter only ever reads certificates and asks the token to sign —
/// never writes to a database a running Firefox or Chrome may also hold
/// open.
fn build_init_string(profile_dir: &Path) -> Result<CString, NssAdapterError> {
    let invalid = || NssAdapterError::InvalidProfilePath(profile_dir.to_path_buf());
    let profile_str = profile_dir.to_str().ok_or_else(invalid)?;
    if profile_str.contains('\'') {
        return Err(invalid());
    }
    CString::new(format!(
        "configdir='sql:{profile_str}' certPrefix='' keyPrefix='' secmod='secmod.db' flags=readOnly"
    ))
    .map_err(|_| invalid())
}

/// The crate's one exception to `#![deny(unsafe_code)]`: NSS's `configdir`
/// argument only reaches `C_Initialize` through `cryptoki`'s own `unsafe`
/// pointer-carrying constructor.
#[allow(unsafe_code)]
fn nss_init_args(init_string: &CString) -> CInitializeArgs {
    // SAFETY: `ptr` is `CString::as_ptr`, which the standard library
    // guarantees is non-null and NUL-terminated for as long as `init_string`
    // lives — which is the caller's (`load`'s) whole function body, spanning
    // the `C_Initialize` call this argument is built for. PKCS#11 requires
    // `pReserved` to remain valid only for that one call, so no dangling
    // pointer can be observed afterward.
    let ptr = init_string.as_ptr() as *mut c_void;
    let non_null = NonNull::new(ptr).expect("CString::as_ptr is never null");
    unsafe { CInitializeArgs::new_with_reserved(CInitializeFlags::OS_LOCKING_OK, non_null) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_init_string_embeds_the_profile_path_as_a_shared_sql_database() {
        let init_string = build_init_string(Path::new("/home/alice/.pki/nssdb"))
            .expect("a plain path must build a valid init string");

        assert_eq!(
            init_string.to_str().unwrap(),
            "configdir='sql:/home/alice/.pki/nssdb' certPrefix='' keyPrefix='' \
             secmod='secmod.db' flags=readOnly"
        );
    }

    #[test]
    fn build_init_string_rejects_a_path_containing_a_single_quote() {
        let error = build_init_string(Path::new("/home/o'brien/.pki/nssdb"))
            .expect_err("an embedded quote would terminate NSS's quoted value early");

        assert!(matches!(error, NssAdapterError::InvalidProfilePath(_)));
    }
}
