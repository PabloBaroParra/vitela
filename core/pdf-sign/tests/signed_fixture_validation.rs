//! Structural cross-validation of the committed signed-PDF corpus (T-079/T-080).

use cms::{content_info::ContentInfo, signed_data::SignedData};
use der::{
    asn1::{ObjectIdentifier, OctetString},
    Decode, Encode, Header, SliceReader,
};
use lopdf::{Document, Object};
use pdf_sign::{digest_byte_ranges, ByteRange, DigestAlgorithm};

const ID_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");

const FIXTURES: &[&[u8]] = &[
    include_bytes!("../../../tests/fixtures/signed/rsa2048_sha256.pdf"),
    include_bytes!("../../../tests/fixtures/signed/p256_sha256.pdf"),
    include_bytes!("../../../tests/fixtures/signed/p256_sha384.pdf"),
    include_bytes!("../../../tests/fixtures/signed/two_signatures_rsa2048_sha256.pdf"),
];

fn signatures(bytes: &[u8]) -> Vec<([u64; 4], Vec<u8>)> {
    let document = Document::load_mem(bytes).expect("fixture must be a loadable PDF");
    document
        .objects
        .values()
        .filter_map(|object| {
            let Object::Dictionary(dictionary) = object else {
                return None;
            };
            matches!(dictionary.get(b"SubFilter"), Ok(Object::Name(name)) if name == b"adbe.pkcs7.detached")
                .then(|| {
                    let Object::Array(values) = dictionary
                        .get(b"ByteRange")
                        .expect("signature dictionary must contain /ByteRange")
                    else {
                        panic!("/ByteRange must be an array");
                    };
                    let range: [u64; 4] = values
                        .iter()
                        .map(|value| value.as_i64().expect("range offsets must be integers") as u64)
                        .collect::<Vec<_>>()
                        .try_into()
                        .expect("/ByteRange must hold four offsets");
                    let Object::String(contents, _) = dictionary
                        .get(b"Contents")
                        .expect("signature dictionary must contain /Contents")
                    else {
                        panic!("/Contents must be a string");
                    };
                    (range, contents.clone())
                })
        })
        .collect()
}

fn cms_message_digest(contents: &[u8]) -> Vec<u8> {
    let mut reader = SliceReader::new(contents).expect("/Contents must fit a DER reader");
    let header = Header::decode(&mut reader).expect("/Contents must start with a DER header");
    let der_length = usize::try_from(
        (header.encoded_len().expect("DER header length") + header.length)
            .expect("DER value length"),
    )
    .expect("DER value length must fit usize");
    let content_info = ContentInfo::from_der(&contents[..der_length])
        .expect("/Contents must begin with DER CMS data");
    let signed_data: SignedData = content_info
        .content
        .decode_as()
        .expect("CMS must be SignedData");
    signed_data
        .signer_infos
        .0
        .iter()
        .next()
        .expect("CMS must contain one signer")
        .signed_attrs
        .as_ref()
        .expect("CMS signer must contain signed attributes")
        .iter()
        .find(|attribute| attribute.oid == ID_MESSAGE_DIGEST)
        .and_then(|attribute| attribute.values.iter().next())
        .expect("CMS signer must contain message-digest")
        .decode_as::<OctetString>()
        .expect("message-digest must be an OCTET STRING")
        .as_bytes()
        .to_vec()
}

fn digest_algorithm(message_digest: &[u8]) -> DigestAlgorithm {
    match message_digest.len() {
        32 => DigestAlgorithm::Sha256,
        48 => DigestAlgorithm::Sha384,
        length => panic!("unexpected CMS message-digest length: {length}"),
    }
}

#[test]
fn committed_fixtures_have_valid_byte_ranges_and_matching_cms_digests() {
    for fixture in FIXTURES {
        let signatures = signatures(fixture);
        let last_covered_end = signatures
            .iter()
            .map(|(values, _)| values[2] + values[3])
            .max()
            .expect("fixture must contain at least one signature");
        assert_eq!(
            last_covered_end,
            fixture.len() as u64,
            "the newest signature must cover the file up to its exact end"
        );

        for (values, contents) in signatures {
            let [start, first_length, second_offset, second_length] = values;
            assert_eq!(start, 0, "/ByteRange must start at the file start");
            assert!(
                first_length < second_offset,
                "/ByteRange must exclude /Contents"
            );
            assert_eq!(
                second_offset - first_length,
                2 * contents.len() as u64 + 2,
                "the /ByteRange gap must be exactly the /Contents token"
            );
            assert!(
                second_offset + second_length <= fixture.len() as u64,
                "/ByteRange must remain within the file"
            );

            let expected = cms_message_digest(&contents);
            let actual =
                digest_byte_ranges(fixture, ByteRange::new(values), digest_algorithm(&expected))
                    .expect("fixture /ByteRange must digest");
            assert_eq!(actual.as_bytes(), expected);
        }
    }
}

#[test]
fn second_signature_keeps_the_first_signature_digest_valid() {
    let fixture = FIXTURES[3];
    let signatures = signatures(fixture);
    assert_eq!(signatures.len(), 2);

    let (first_range, first_contents) = &signatures[0];
    assert!(
        first_range[2] + first_range[3] < fixture.len() as u64,
        "signatures[0] must be the earlier-revision signature, whose /ByteRange \
         ends before the incremental update that added the second signature"
    );
    let expected = cms_message_digest(first_contents);
    let actual = digest_byte_ranges(
        fixture,
        ByteRange::new(*first_range),
        digest_algorithm(&expected),
    )
    .expect("the first signature /ByteRange must still digest after an incremental update");
    assert_eq!(actual.as_bytes(), expected);
}
