//! Digest calculation over PDF byte ranges, bound to its algorithm.

use sha2::{Digest, Sha256, Sha384, Sha512};

use crate::{ByteRange, DigestAlgorithm, SignError};

/// Digest bytes bound to the [`DigestAlgorithm`] that produced them.
///
/// Carrying the algorithm makes a "hashed with one algorithm, signed claiming
/// another" mismatch detectable even between algorithms that share an output
/// length. [`CertificateSourcePort::sign_digest`] only accepts this type, so
/// every digest reaching an adapter has a validated length and a known origin.
///
/// [`CertificateSourcePort::sign_digest`]: crate::CertificateSourcePort::sign_digest
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentDigest {
    algorithm: DigestAlgorithm,
    bytes: Vec<u8>,
}

impl DocumentDigest {
    /// Wraps digest bytes computed outside [`digest_byte_ranges`], such as
    /// the digest of DER-encoded CMS signed attributes.
    ///
    /// # Errors
    ///
    /// Returns [`SignError::InvalidDigestLength`] when `bytes` does not have
    /// the output length of `algorithm`.
    pub fn new(algorithm: DigestAlgorithm, bytes: Vec<u8>) -> Result<Self, SignError> {
        let expected = algorithm.digest_len();
        if bytes.len() != expected {
            return Err(SignError::InvalidDigestLength {
                algorithm,
                expected,
                actual: bytes.len(),
            });
        }
        Ok(Self { algorithm, bytes })
    }

    /// Returns the algorithm that produced these digest bytes.
    #[must_use]
    pub const fn algorithm(&self) -> DigestAlgorithm {
        self.algorithm
    }

    /// Returns the digest bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Computes a digest over the two regions described by a PDF `/ByteRange`.
///
/// Bytes in the gap between the regions are excluded from the digest. For a
/// prepared signature this gap is the complete hexadecimal `/Contents` token.
///
/// # Errors
///
/// Returns [`SignError::InvalidByteRange`] when either covered region falls
/// outside `bytes`.
pub fn digest_byte_ranges(
    bytes: &[u8],
    byte_range: ByteRange,
    algorithm: DigestAlgorithm,
) -> Result<DocumentDigest, SignError> {
    let [first_offset, first_length, second_offset, second_length] = byte_range.values();
    let invalid_range = || SignError::InvalidByteRange {
        byte_range,
        document_length: bytes.len(),
    };
    let first = covered_slice(bytes, first_offset, first_length).ok_or_else(invalid_range)?;
    let second = covered_slice(bytes, second_offset, second_length).ok_or_else(invalid_range)?;

    let digest = match algorithm {
        DigestAlgorithm::Sha256 => digest_regions::<Sha256>(first, second),
        DigestAlgorithm::Sha384 => digest_regions::<Sha384>(first, second),
        DigestAlgorithm::Sha512 => digest_regions::<Sha512>(first, second),
    };
    Ok(DocumentDigest {
        algorithm,
        bytes: digest,
    })
}

fn covered_slice(bytes: &[u8], offset: u64, length: u64) -> Option<&[u8]> {
    let end = offset.checked_add(length)?;
    if end > bytes.len() as u64 {
        return None;
    }
    bytes.get(offset as usize..end as usize)
}

fn digest_regions<D: Digest>(first: &[u8], second: &[u8]) -> Vec<u8> {
    let mut digest = D::new();
    digest.update(first);
    digest.update(second);
    digest.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prepare_signature_bytes, signature::serialized_placeholder};

    fn prepared_signature() -> crate::PreparedSignature {
        prepare_signature_bytes(serialized_placeholder(8), 8)
            .expect("test placeholder should be prepared")
    }

    fn covered_bytes(prepared: &crate::PreparedSignature) -> Vec<u8> {
        let [first_offset, first_length, second_offset, second_length] =
            prepared.byte_range.values().map(|value| value as usize);
        let mut bytes = prepared.bytes[first_offset..first_offset + first_length].to_vec();
        bytes.extend_from_slice(&prepared.bytes[second_offset..second_offset + second_length]);
        bytes
    }

    #[test]
    fn digest_byte_ranges_matches_sha256_of_covered_bytes() {
        let prepared = prepared_signature();
        let expected = Sha256::digest(covered_bytes(&prepared)).to_vec();

        let actual = digest_byte_ranges(
            &prepared.bytes,
            prepared.byte_range,
            DigestAlgorithm::Sha256,
        )
        .expect("valid byte ranges should hash");

        assert_eq!(actual.as_bytes(), expected);
    }

    #[test]
    fn digest_byte_ranges_matches_sha384_of_covered_bytes() {
        let prepared = prepared_signature();
        let expected = Sha384::digest(covered_bytes(&prepared)).to_vec();

        let actual = digest_byte_ranges(
            &prepared.bytes,
            prepared.byte_range,
            DigestAlgorithm::Sha384,
        )
        .expect("valid byte ranges should hash");

        assert_eq!(actual.as_bytes(), expected);
    }

    #[test]
    fn digest_byte_ranges_matches_sha512_of_covered_bytes() {
        let prepared = prepared_signature();
        let expected = Sha512::digest(covered_bytes(&prepared)).to_vec();

        let actual = digest_byte_ranges(
            &prepared.bytes,
            prepared.byte_range,
            DigestAlgorithm::Sha512,
        )
        .expect("valid byte ranges should hash");

        assert_eq!(actual.as_bytes(), expected);
    }

    #[test]
    fn digest_byte_ranges_binds_the_requested_algorithm() {
        let prepared = prepared_signature();

        let digest = digest_byte_ranges(
            &prepared.bytes,
            prepared.byte_range,
            DigestAlgorithm::Sha384,
        )
        .expect("valid byte ranges should hash");

        assert_eq!(digest.algorithm(), DigestAlgorithm::Sha384);
    }

    #[test]
    fn digest_byte_ranges_ignores_contents_gap() {
        let prepared = prepared_signature();
        let original = digest_byte_ranges(
            &prepared.bytes,
            prepared.byte_range,
            DigestAlgorithm::Sha256,
        )
        .expect("valid byte ranges should hash");
        let mut changed = prepared.bytes.clone();
        changed[prepared.contents_hex_range.start] = b'F';

        let changed = digest_byte_ranges(&changed, prepared.byte_range, DigestAlgorithm::Sha256)
            .expect("changing excluded contents should still hash");

        assert_eq!(changed, original);
    }

    #[test]
    fn digest_byte_ranges_rejects_region_past_document_end() {
        let prepared = prepared_signature();
        let truncated = &prepared.bytes[..prepared.bytes.len() - 1];

        let error = digest_byte_ranges(truncated, prepared.byte_range, DigestAlgorithm::Sha256)
            .expect_err("out-of-bounds byte range should fail");

        assert_eq!(
            error,
            SignError::InvalidByteRange {
                byte_range: prepared.byte_range,
                document_length: truncated.len(),
            }
        );
    }

    #[test]
    fn document_digest_rejects_wrong_length_bytes() {
        let error = DocumentDigest::new(DigestAlgorithm::Sha256, vec![0; 31])
            .expect_err("short SHA-256 digest should fail");

        assert_eq!(
            error,
            SignError::InvalidDigestLength {
                algorithm: DigestAlgorithm::Sha256,
                expected: DigestAlgorithm::Sha256.digest_len(),
                actual: 31,
            }
        );
    }

    #[test]
    fn document_digest_accepts_matching_length_bytes() {
        let digest = DocumentDigest::new(DigestAlgorithm::Sha512, vec![0xA5; 64])
            .expect("correct-length digest should wrap");

        assert_eq!(
            (digest.algorithm(), digest.as_bytes()),
            (DigestAlgorithm::Sha512, [0xA5; 64].as_slice())
        );
    }
}
