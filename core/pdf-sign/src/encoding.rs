//! Shared DER encodings required by certificate-source adapters.

use crate::DigestAlgorithm;

/// Encodes the ASN.1 DER length octets for a value no larger than 65535 bytes.
#[must_use]
pub fn der_length(length: usize) -> Option<Vec<u8>> {
    match length {
        0..=0x7f => Some(vec![length as u8]),
        0x80..=0xff => Some(vec![0x81, length as u8]),
        0x100..=0xffff => Some(vec![0x82, (length >> 8) as u8, (length & 0xff) as u8]),
        _ => None,
    }
}

/// Encodes a non-negative integer as a minimal ASN.1 DER `INTEGER`.
#[must_use]
pub fn der_integer(value: &[u8]) -> Option<Vec<u8>> {
    let magnitude = &value[value
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(value.len())..];
    let pad = usize::from(magnitude.first().is_none_or(|byte| byte & 0x80 != 0));
    let mut integer = vec![0x02];
    integer.extend(der_length(magnitude.len() + pad)?);
    if pad == 1 {
        integer.push(0x00);
    }
    integer.extend_from_slice(magnitude);
    Some(integer)
}

/// Converts a fixed-width `r || s` ECDSA signature into DER `ECDSA-Sig-Value`.
#[must_use]
pub fn der_encode_ecdsa_signature(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.is_empty() || !raw.len().is_multiple_of(2) {
        return None;
    }
    let half = raw.len() / 2;
    let body = [der_integer(&raw[..half])?, der_integer(&raw[half..])?].concat();
    let mut sequence = vec![0x30];
    sequence.extend(der_length(body.len())?);
    sequence.extend(body);
    Some(sequence)
}

/// Builds the PKCS#1 v1.5 `DigestInfo` value required by raw RSA signers.
#[must_use]
pub fn rsa_digest_info(algorithm: DigestAlgorithm, digest: &[u8]) -> Option<Vec<u8>> {
    let prefix = match algorithm {
        DigestAlgorithm::Sha256 => &[
            0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x01, 0x05, 0x00, 0x04, 0x20,
        ][..],
        DigestAlgorithm::Sha384 => &[
            0x30, 0x41, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x02, 0x05, 0x00, 0x04, 0x30,
        ][..],
        DigestAlgorithm::Sha512 => &[
            0x30, 0x51, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x03, 0x05, 0x00, 0x04, 0x40,
        ][..],
    };
    Some([prefix, digest].concat())
}
