use pdf_sign::{
    der_encode_ecdsa_signature, der_integer, der_length, rsa_digest_info, DigestAlgorithm,
};

#[test]
fn rsa_digest_info_wraps_sha256_digest_with_its_der_algorithm_identifier() {
    let digest = vec![0xA5; 32];

    let encoded = rsa_digest_info(DigestAlgorithm::Sha256, &digest)
        .expect("SHA-256 digest should have a PKCS#1 DigestInfo encoding");

    assert_eq!(
        encoded,
        [
            vec![
                0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x01, 0x05, 0x00, 0x04, 0x20,
            ],
            digest,
        ]
        .concat()
    );
}

#[test]
fn der_helpers_encode_padded_ecdsa_components_and_long_form_lengths() {
    assert_eq!(der_length(0x12c), Some(vec![0x82, 0x01, 0x2c]));
    assert_eq!(
        der_integer(&[0x00, 0x80]),
        Some(vec![0x02, 0x02, 0x00, 0x80])
    );
    assert_eq!(
        der_encode_ecdsa_signature(&[0x00, 0x80, 0x00, 0x01]),
        Some(vec![0x30, 0x07, 0x02, 0x02, 0x00, 0x80, 0x02, 0x01, 0x01])
    );
}
