use pdf_sign_pkcs11::Pkcs11CertificateSource;

#[test]
fn loading_a_missing_module_returns_an_error_without_exposing_a_private_key() {
    let error = Pkcs11CertificateSource::load("missing-pkcs11-module.so", None)
        .expect_err("a missing PKCS#11 module must fail during adapter construction");

    assert!(error.to_string().contains("PKCS#11 module"));
}

#[test]
fn loading_a_missing_module_with_a_pin_returns_an_error_without_attempting_signing() {
    let error = Pkcs11CertificateSource::load("missing-pkcs11-module.so", Some("1234".into()))
        .expect_err("a missing PKCS#11 module must fail before any token operation");

    assert!(error.to_string().contains("PKCS#11 module"));
}
