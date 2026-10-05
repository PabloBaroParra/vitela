//! Failure contracts of the source boundary. Real foreign RSA/ECDSA callbacks
//! are exercised by Windows SigningSmoke against generated C# bindings.
use pdf_ffi::{
    open_platform_signing_source, open_token_signing_source, FfiError, FfiPlatformSigningIdentity,
    FfiSigningAlgorithm, PlatformSigningSource,
};

struct UnavailableStore;
impl PlatformSigningSource for UnavailableStore {
    fn identities(&self) -> Result<Vec<FfiPlatformSigningIdentity>, FfiError> {
        Err(FfiError::UnsupportedOperation {
            detail: "store unavailable".into(),
        })
    }
    fn sign_digest(
        &self,
        _: String,
        _: Vec<u8>,
        _: FfiSigningAlgorithm,
    ) -> Result<Vec<u8>, FfiError> {
        panic!("discovery failure must never attempt signing")
    }
}

#[test]
fn native_store_discovery_errors_cross_the_boundary_without_creating_a_source() {
    let error = open_platform_signing_source(Box::new(UnavailableStore)).unwrap_err();
    assert!(
        matches!(error, FfiError::UnsupportedOperation { detail } if detail == "store unavailable")
    );
}

#[test]
fn missing_token_module_is_refused_without_disclosing_the_pin() {
    let error = open_token_signing_source(
        "missing-vitela-token-module".into(),
        Some("private-test-pin".into()),
    )
    .unwrap_err();
    assert!(matches!(&error, FfiError::UnsupportedOperation { .. }));
    assert!(!error.to_string().contains("private-test-pin"));
}
